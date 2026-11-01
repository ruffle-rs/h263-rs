//! H.263 decoder core

use crate::decoder::cpu::{gather, idct_channel, inverse_rle, mv_decode, predict_candidate};
use crate::decoder::picture::DecodedPicture;
use crate::decoder::types::DecoderOption;
use crate::error::{Error, Result};
use crate::parser::{decode_block, decode_gob, decode_macroblock, decode_picture, H263Reader};
use crate::types::{
    DecodedDctBlock, GroupOfBlocks, Macroblock, MacroblockType, MotionVector, Picture,
    PictureOption, PictureTypeCode, MPPTYPE_OPTIONS, OPPTYPE_OPTIONS,
};
use std::collections::HashMap;
use std::io::Read;

/// All state necessary to decode a successive series of H.263 pictures.
pub struct H263State {
    /// External decoder options enabled on this decoder.
    decoder_options: DecoderOption,

    /// The temporal reference of the last decoded picture.
    ///
    /// `None` indicates that no picture has been decoded yet.
    last_picture: Option<u16>,

    /// The temporal reference of the implicit reference picture for the next
    /// decode operation.
    ///
    /// Disposable frames do not update the reference picture index.
    ///
    /// `None` indicates that no reference picture has been decoded yet.
    reference_picture: Option<u16>,

    /// All currently in-force picture options as of the last decoded frame.
    running_options: PictureOption,

    /// All previously-encoded reference pictures.
    reference_states: HashMap<u16, DecodedPicture>,

    /// The last decoded picture, if it is disposable.
    ///
    /// Disposable pictures are kept apart from `reference_states`, as they can
    /// have the temporal reference of a reference picture, which they must not
    /// replace.
    disposable_picture: Option<DecodedPicture>,
}

impl H263State {
    /// Construct a new `H263State`.
    pub fn new(decoder_options: DecoderOption) -> Self {
        Self {
            decoder_options,
            last_picture: None,
            reference_picture: None,
            running_options: PictureOption::empty(),
            reference_states: HashMap::new(),
            disposable_picture: None,
        }
    }

    /// Determine if this decoder is in "Sorenson" H.263 mode.
    pub fn is_sorenson(&self) -> bool {
        self.decoder_options
            .contains(DecoderOption::SORENSON_SPARK_BITSTREAM)
    }

    /// Get the last picture decoded in the bitstream.
    ///
    /// If `None`, then no pictures have yet to be decoded.
    pub fn get_last_picture(&self) -> Option<&DecodedPicture> {
        if self.disposable_picture.is_some() {
            return self.disposable_picture.as_ref();
        }
        self.last_picture
            .and_then(|last_picture| self.reference_states.get(&last_picture))
    }

    /// Get the implicit reference picture decoded in the bitstream.
    ///
    /// If `None`, then no pictures have yet to be decoded.
    pub fn get_reference_picture(&self) -> Option<&DecodedPicture> {
        self.reference_picture
            .and_then(|reference_picture| self.reference_states.get(&reference_picture))
    }

    /// Remove all disposable pictures from the reference states list.
    pub fn cleanup_buffers(&mut self) {
        let last_picture = self
            .last_picture
            .and_then(|lp| self.reference_states.remove_entry(&lp));
        let reference_picture = self
            .reference_picture
            .and_then(|rp| self.reference_states.remove_entry(&rp));

        self.reference_states = HashMap::new();

        if let Some((k, lp)) = last_picture {
            self.reference_states.insert(k, lp);
        }

        if let Some((k, rp)) = reference_picture {
            self.reference_states.insert(k, rp);
        }
    }

    /// Parse a picture from the reader using the current state's decoder
    /// options.
    pub fn parse_picture<R>(
        &self,
        reader: &mut H263Reader<R>,
        previous_picture: Option<&Picture>,
    ) -> Result<Option<Picture>>
    where
        R: Read,
    {
        decode_picture(reader, self.decoder_options, previous_picture)
    }

    /// Decode the next picture in the bitstream.
    ///
    /// This does not yield any picture data: it merely advances the state of
    /// the encoder, if possible, such that the next picture in the stream can
    /// be retrieved from it. Bits are retrieved from the `reader`, which must
    /// be pointing to an optionally-aligned picture start code.
    ///
    /// In the event that an error occurs, previously existing decoder state
    /// and underlying reader state will remain. You may inspect the error in
    /// order to determine how to proceed. When doing so, it will be likely
    /// that you will need to replace the reader or it's bitstream. The rules
    /// for how you can change out readers are as follows:
    ///
    /// 1. A reader is semantically related to a previously-used reader if a
    ///    successful read of the previous reader would have yielded the same
    ///    bits as a read on the current one.
    /// 2. A reader remains semantically related to a previously-used reader if
    ///    unsuccessful reads of the previous reader are successful in the new
    ///    reader and the resulting additional bitstream correctly forms a
    ///    syntactically and semantically valid bitstream for this decoder.
    ///
    /// In practice, this means that streaming additional bits into the reader
    /// is OK, but seeking the reader to a new position is not. In order to
    /// seek to a new position, you must discard all existing decoder state,
    /// then seek to the position of a valid I frame and begin decoding anew.
    pub fn decode_next_picture<R>(&mut self, reader: &mut H263Reader<R>) -> Result<()>
    where
        R: Read,
    {
        reader.with_transaction(|reader| {
            let next_picture = self
                .parse_picture(reader, self.get_last_picture().map(|p| p.as_header()))?
                .ok_or(Error::MiddleOfBitstream)?;

            let next_running_options = if next_picture.has_plusptype && next_picture.has_opptype {
                next_picture.options
            } else if next_picture.has_plusptype {
                (next_picture.options & !OPPTYPE_OPTIONS) | (self.running_options & OPPTYPE_OPTIONS)
            } else {
                (next_picture.options & !OPPTYPE_OPTIONS & !MPPTYPE_OPTIONS)
                    | (self.running_options & (OPPTYPE_OPTIONS | MPPTYPE_OPTIONS))
            };

            let format = if let Some(format) = next_picture.format {
                format
            } else if matches!(next_picture.picture_type, PictureTypeCode::IFrame) {
                return Err(Error::PictureFormatMissing);
            } else if let Some(ref_format) = self.get_last_picture().map(|rp| rp.format()) {
                ref_format
            } else {
                return Err(Error::PictureFormatMissing);
            };

            let reference_picture = self.get_reference_picture();

            let output_dimensions = format
                .into_width_and_height()
                .ok_or(Error::PictureFormatInvalid)?;

            let mb_per_line = (output_dimensions.0 as f64 / 16.0).ceil() as usize;
            let mb_height = (output_dimensions.1 as f64 / 16.0).ceil() as usize;

            let level_dimensions = (mb_per_line * 16, mb_height * 16);

            let mut in_force_quantizer = next_picture.quantizer;
            let mut predictor_vectors = Vec::with_capacity(mb_per_line * mb_height); // all previously decoded MVDs
            let mut macroblock_types = Vec::with_capacity(mb_per_line * mb_height);
            let mut macroblocks_after_gob = 0; //reset after every GOB header

            let mut next_decoded_picture =
                DecodedPicture::new(next_picture, format).ok_or(Error::PictureFormatInvalid)?;

            let mut luma_levels =
                vec![DecodedDctBlock::Zero; level_dimensions.0 * level_dimensions.1 / 64];
            let mut chroma_b_levels =
                vec![DecodedDctBlock::Zero; level_dimensions.0 * level_dimensions.1 / 4 / 64];
            let mut chroma_r_levels =
                vec![DecodedDctBlock::Zero; level_dimensions.0 * level_dimensions.1 / 4 / 64];

            let is_sorenson = self.is_sorenson();
            loop {
                // A Sorenson picture ends with its last macroblock. What
                // follows isn't part of it: some encoders pad Sorenson
                // pictures with a zero byte, which would otherwise be read as
                // an invalid macroblock header. Standard H.263 can have
                // macroblock stuffing there, which the loop consumes.
                if is_sorenson && macroblock_types.len() >= mb_per_line * mb_height {
                    break;
                }

                let mb = decode_macroblock(
                    reader,
                    next_decoded_picture.as_header(),
                    next_running_options,
                );
                let pos = (
                    (macroblock_types.len() % mb_per_line) * 16,
                    (macroblock_types.len() / mb_per_line) * 16,
                );
                let mut motion_vectors = [MotionVector::zero(); 4];

                let mb_type = match mb {
                    Ok(Macroblock::Stuffing) => continue,
                    Ok(Macroblock::Uncoded) => {
                        if matches!(
                            next_decoded_picture.as_header().picture_type,
                            PictureTypeCode::IFrame
                        ) {
                            return Err(Error::UncodedIFrameBlocks);
                        }

                        MacroblockType::Inter
                    }
                    Ok(Macroblock::Coded {
                        mb_type,
                        coded_block_pattern,
                        coded_block_pattern_b: _coded_block_pattern_b,
                        d_quantizer,
                        motion_vector,
                        addl_motion_vectors,
                        motion_vectors_b: _motion_vectors_b,
                    }) => {
                        let quantizer = in_force_quantizer as i8 + d_quantizer.unwrap_or(0);
                        in_force_quantizer = quantizer.clamp(1, 31) as u8;

                        if mb_type.is_inter() {
                            let mv1 = motion_vector.unwrap_or_else(MotionVector::zero);
                            let mpred1 = predict_candidate(
                                &predictor_vectors[macroblocks_after_gob..],
                                &motion_vectors,
                                mb_per_line,
                                0,
                            );

                            motion_vectors[0] =
                                mv_decode(&next_decoded_picture, next_running_options, mpred1, mv1);

                            if let Some([mv2, mv3, mv4]) = addl_motion_vectors {
                                let mpred2 = predict_candidate(
                                    &predictor_vectors[macroblocks_after_gob..],
                                    &motion_vectors,
                                    mb_per_line,
                                    1,
                                );
                                motion_vectors[1] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    mpred2,
                                    mv2,
                                );

                                let mpred3 = predict_candidate(
                                    &predictor_vectors[macroblocks_after_gob..],
                                    &motion_vectors,
                                    mb_per_line,
                                    2,
                                );
                                motion_vectors[2] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    mpred3,
                                    mv3,
                                );

                                let mpred4 = predict_candidate(
                                    &predictor_vectors[macroblocks_after_gob..],
                                    &motion_vectors,
                                    mb_per_line,
                                    3,
                                );
                                motion_vectors[3] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    mpred4,
                                    mv4,
                                );
                            } else {
                                motion_vectors[1] = motion_vectors[0];
                                motion_vectors[2] = motion_vectors[0];
                                motion_vectors[3] = motion_vectors[0];
                            };
                        };

                        let luma0 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[0],
                        )?;
                        inverse_rle(
                            &luma0,
                            &mut luma_levels,
                            pos,
                            level_dimensions.0 / 8,
                            in_force_quantizer,
                        );

                        let luma1 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[1],
                        )?;
                        inverse_rle(
                            &luma1,
                            &mut luma_levels,
                            (pos.0 + 8, pos.1),
                            level_dimensions.0 / 8,
                            in_force_quantizer,
                        );

                        let luma2 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[2],
                        )?;
                        inverse_rle(
                            &luma2,
                            &mut luma_levels,
                            (pos.0, pos.1 + 8),
                            level_dimensions.0 / 8,
                            in_force_quantizer,
                        );

                        let luma3 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[3],
                        )?;
                        inverse_rle(
                            &luma3,
                            &mut luma_levels,
                            (pos.0 + 8, pos.1 + 8),
                            level_dimensions.0 / 8,
                            in_force_quantizer,
                        );

                        let chroma_b = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_chroma_b,
                        )?;
                        inverse_rle(
                            &chroma_b,
                            &mut chroma_b_levels,
                            (pos.0 / 2, pos.1 / 2),
                            mb_per_line,
                            in_force_quantizer,
                        );

                        let chroma_r = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_chroma_r,
                        )?;
                        inverse_rle(
                            &chroma_r,
                            &mut chroma_r_levels,
                            (pos.0 / 2, pos.1 / 2),
                            mb_per_line,
                            in_force_quantizer,
                        );

                        mb_type
                    }

                    //Attempt to recover from macroblock errors if possible
                    Err(ref e) if e.is_macroblock_error() && !is_sorenson => {
                        match decode_gob(reader, self.decoder_options) {
                            //Resynchronized to end of picture.
                            Ok(None) => break,

                            //Resynchronized to end of GOB.
                            Ok(Some(GroupOfBlocks {
                                group_number: _group_number,
                                multiplex_bitstream: _multiplex_bitstream,
                                frame_id: _frame_id,
                                quantizer,
                            })) => {
                                in_force_quantizer = quantizer;
                                macroblocks_after_gob = macroblock_types.len();
                                continue;
                            }

                            // Treat EOF/GOB errors as end of picture
                            Err(ref e) if e.is_eof_error() || e.is_gob_error() => break,
                            Err(e) => return Err(e),
                        }
                    }

                    //Treat EOF errors as end of picture
                    Err(ref e) if e.is_eof_error() => break,
                    Err(e) => return Err(e),
                };

                predictor_vectors.push(motion_vectors);
                macroblock_types.push(mb_type);
            }

            //If the picture ended early, assume all the remaining blocks are
            //empty INTER blocks with motion vector (0,0)
            if predictor_vectors.len() < predictor_vectors.capacity() {
                predictor_vectors.resize(predictor_vectors.capacity(), [MotionVector::zero(); 4]);
            }

            if macroblock_types.len() < macroblock_types.capacity() {
                macroblock_types.resize(macroblock_types.capacity(), MacroblockType::Inter);
            }

            //We have now read out all of the macroblock and block data and
            //queued it up into the various internal buffers we allocated for
            //this purpose. Time to decode it all in one go.
            gather(
                &macroblock_types,
                reference_picture,
                &predictor_vectors,
                mb_per_line,
                &mut next_decoded_picture,
            )?;
            idct_channel(
                &luma_levels,
                next_decoded_picture.as_luma_mut(),
                mb_per_line * 2,
                (output_dimensions.0).into(),
            );

            let chroma_samples_per_row = next_decoded_picture.chroma_samples_per_row();
            idct_channel(
                &chroma_b_levels,
                next_decoded_picture.as_chroma_b_mut(),
                mb_per_line,
                chroma_samples_per_row,
            );
            idct_channel(
                &chroma_r_levels,
                next_decoded_picture.as_chroma_r_mut(),
                mb_per_line,
                chroma_samples_per_row,
            );

            //At this point, all decoding should be complete, and we should
            //have a fresh picture to put into the reference pile. We treat YUV
            //encoded pictures as "decoded" since the referencing scheme used
            //in H.263 demands it. Ask a GPU for help.
            if matches!(
                next_decoded_picture.as_header().picture_type,
                PictureTypeCode::IFrame
            ) {
                //You cannot backwards predict across iframes
                self.reference_picture = None;
            }

            let this_tr = next_decoded_picture.as_header().temporal_reference;
            self.last_picture = Some(this_tr);
            if next_decoded_picture
                .as_header()
                .picture_type
                .is_disposable()
            {
                self.disposable_picture = Some(next_decoded_picture);
            } else {
                self.reference_picture = Some(this_tr);
                self.disposable_picture = None;
                self.reference_states.insert(this_tr, next_decoded_picture);
            }
            self.cleanup_buffers();

            reader.commit();

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16x16 Sorenson Spark I-frame of a flat color (one macroblock),
    /// encoded by FFmpeg 9.0 with:
    ///
    /// `ffmpeg -f lavfi -i color=c=0x3080c0:s=16x16 -frames:v 1 -c:v flv1 -q:v 10 -f rawvideo frame.bin`
    ///
    /// It decodes to Y 112, Cb 168, Cr 88 everywhere.
    const SORENSON_FRAME: [u8; 14] = [
        0x00, 0x00, 0x84, 0x00, 0x08, 0x08, 0x15, 0x26, 0xe0, 0xe0, 0xe0, 0xe1, 0x50, 0xb0,
    ];

    #[test]
    fn picture_ends_with_last_macroblock() {
        for trailing in [&[][..], &[0x00], &[0x00, 0x00]] {
            let mut data = SORENSON_FRAME.to_vec();
            data.extend_from_slice(trailing);

            let mut state = H263State::new(DecoderOption::SORENSON_SPARK_BITSTREAM);
            let mut reader = H263Reader::from_source(&data[..]);
            state.decode_next_picture(&mut reader).unwrap_or_else(|e| {
                panic!("{} trailing bytes: {e:?}", trailing.len());
            });
            let picture = state.get_last_picture().expect("no picture decoded");
            let (luma, chroma_b, chroma_r) = picture.as_yuv();
            assert!(luma.iter().all(|&y| y == 112), "luma {:?}", luma);
            assert!(
                chroma_b.iter().all(|&cb| cb == 168),
                "chroma b {:?}",
                chroma_b
            );
            assert!(
                chroma_r.iter().all(|&cr| cr == 88),
                "chroma r {:?}",
                chroma_r
            );
        }
    }
}

#[cfg(test)]
mod disposable_tests {
    use super::*;

    /// Bits written most significant first.
    fn bits(fields: &[(u32, u32)]) -> Vec<u8> {
        let mut out = vec![];
        let mut n = 0;
        for &(value, len) in fields {
            for i in (0..len).rev() {
                if n % 8 == 0 {
                    out.push(0);
                }
                *out.last_mut().unwrap() |= (((value >> i) & 1) as u8) << (7 - n % 8);
                n += 1;
            }
        }
        out
    }

    /// A 16x16 Sorenson Spark picture header with temporal reference 0.
    fn header(picture_type: u32) -> Vec<(u32, u32)> {
        vec![
            (1, 17),           // picture start code
            (1, 5),            // version
            (0, 8),            // temporal reference
            (0, 3),            // custom size, 8 bits each
            (16, 8),           // width
            (16, 8),           // height
            (picture_type, 2), // 0: I, 2: disposable P
            (0, 1),            // deblocking
            (10, 5),           // quantizer
            (0, 1),            // no extra information
        ]
    }

    #[test]
    fn disposable_picture_is_no_reference() {
        let mut state = H263State::new(DecoderOption::SORENSON_SPARK_BITSTREAM);

        // An intra macroblock with only a DC coefficient: MCBPC 1 (intra, no
        // chroma), CBPY 0011 (no luma), then four luma and two chroma DCs.
        let mut i_frame = header(0);
        i_frame.extend([(1, 1), (0b0011, 4)]);
        i_frame.extend([(64, 8); 6]);
        let data = bits(&i_frame);
        state
            .decode_next_picture(&mut H263Reader::from_source(&data[..]))
            .unwrap();

        // A disposable P-picture with the same temporal reference, whose only
        // macroblock is coded: COD 0, MCBPC 1 (inter, no chroma), CBPY 11 (no
        // luma), and a zero motion vector.
        let mut d_frame = header(2);
        d_frame.extend([(0, 1), (1, 1), (0b11, 2), (1, 1), (1, 1)]);
        let data = bits(&d_frame);
        state
            .decode_next_picture(&mut H263Reader::from_source(&data[..]))
            .unwrap();

        let last = state.get_last_picture().unwrap().as_header().picture_type;
        assert!(matches!(last, PictureTypeCode::DisposablePFrame));
        let reference = state.get_reference_picture().unwrap();
        assert!(matches!(
            reference.as_header().picture_type,
            PictureTypeCode::IFrame
        ));
    }
}
