//! Internal reader adapter for reading H.263 bitstreams.

use crate::error::{Error, Result};
use crate::traits::BitReadable;
use crate::vlc::{Entry, Table};
use std::cmp::min;
use std::collections::VecDeque;
use std::io::Read;

/// Given a bit count, construct a bitmask with that many bits set to one.
///
/// Bits will be shifted in towards the left, and thus the resulting bitmask
/// will be aligned to the lowest bits in the type. For example, asking for six
/// bits in a u16 will result in a bitmask of 0x006F. This matches the behavior
/// of `H263Reader::peek_bits`.
fn bitmask_from_popcount<T: BitReadable>(bits_needed: u32) -> T {
    let mut result = T::zero();

    for _ in 0..bits_needed {
        result = result << 1 | T::one();
    }

    result
}

/// A reader that allows decoding an H.263 compliant bitstream.
///
/// This reader implements an internal buffer that can be read from as a series
/// of bits into a number of possible types.
pub struct H263Reader<R>
where
    R: Read,
{
    /// The data source to read bits from.
    source: R,

    /// Internal buffer of already-read bitstream data.
    buffer: VecDeque<u8>,

    /// How many bits of the buffer have already been read.
    ///
    /// If this value modulo eight is nonzero, then reads out of the internal
    /// buffer must read
    bits_read: usize,
}

impl<R> H263Reader<R>
where
    R: Read,
{
    /// Wrap a source file in a reader.
    pub fn from_source(source: R) -> Self {
        Self {
            source,
            buffer: VecDeque::new(),
            bits_read: 0,
        }
    }

    /// Fill the internal read buffer with a given number of bytes.
    ///
    /// This function will yield all I/O errors wrapped inside of the
    /// `UnhandledIoError` variant type.
    fn buffer_bytes(&mut self, bytes_needed: usize) -> Result<()> {
        let mut byte = [0];
        for _ in 0..bytes_needed {
            //TODO: Get a byte, get a byte, get a byte, byte, byte!
            self.source.read_exact(&mut byte[..])?;
            self.buffer.push_back(byte[0]);
        }

        Ok(())
    }

    /// Given a certain number of needed bits, return how many bytes would need
    /// to be buffered to read it.
    fn needed_bytes_for_bits(&mut self, bits_needed: u32) -> usize {
        let bits_available = (self.buffer.len() * 8).saturating_sub(self.bits_read);
        let bits_short = (bits_needed as usize).saturating_sub(bits_available);

        (bits_short / 8) + if bits_short % 8 != 0 { 1 } else { 0 }
    }

    /// Ensure that at least a certain number of additional bits can be read
    /// from the internal buffer.
    fn ensure_bits(&mut self, bits_needed: u32) -> Result<()> {
        let bytes = self.needed_bytes_for_bits(bits_needed);
        self.buffer_bytes(bytes)
    }

    /// Copy an arbitrary number of bits from the stream out into a type.
    ///
    /// The bits will be returned such that the read-out bits start from the
    /// least significant bit of the returned type. This means that, say,
    /// reading two bits from the bitstream will result in a value that has
    /// been zero-extended. This can be useful for populating structs with
    /// bitstream data which matches the binary representation of the type the
    /// struct uses.
    ///
    /// This function does not remove bits from the buffer. Repeated calls to
    /// `peek_bits` return the same bits.
    ///
    /// The `bits_needed` must not exceed the maximum width of the type. Any
    /// attempt to do so will result in an error.
    pub fn peek_bits<T: BitReadable>(&mut self, mut bits_needed: u32) -> Result<T> {
        if (T::zero().checked_shl(bits_needed)).is_none() {
            return Err(Error::InternalDecoderError);
        }

        self.ensure_bits(bits_needed)?;

        let mut accum = T::zero();
        let bytes_read = self.bits_read / 8;
        let mut bits_read = self.bits_read % 8;
        for byte in self.buffer.iter().skip(bytes_read) {
            let byte = byte << bits_read;
            let bits_in_byte = (8 as u32).saturating_sub(bits_read as u32);

            let bits_to_shift_in = min(bits_in_byte, bits_needed);

            accum = (accum << bits_to_shift_in) | (byte >> (8 - bits_to_shift_in)).into();

            bits_read = 0;
            bits_needed = bits_needed.saturating_sub(bits_to_shift_in);
        }

        assert_eq!(
            0, bits_needed,
            "return type accumulator should have been filled"
        );

        Ok(accum)
    }

    /// Skip forward a certain number of bits in the stream buffer.
    ///
    /// If more bits are requested to be skipped than exist within the buffer,
    /// then they will be read in. If this process generates an IO error of any
    /// kind, it will be returned, and no skipping will take place.
    fn skip_bits(&mut self, bits_to_skip: u32) -> Result<()> {
        self.ensure_bits(bits_to_skip)?;

        self.bits_read += bits_to_skip as usize;

        Ok(())
    }

    /// Move an arbitrary number of bits from the stream out into a type.
    ///
    /// This function operates similar to `peek_bits`, but the internal buffer
    /// of this reader will be advanced by the same number of bits that have
    /// been returned.
    pub fn read_bits<T: BitReadable>(&mut self, bits_needed: u32) -> Result<T> {
        let r = self.peek_bits(bits_needed)?;
        self.skip_bits(bits_needed)?;

        Ok(r)
    }

    /// Read a `u8` from the bitstream.
    pub fn read_u8(&mut self) -> Result<u8> {
        self.read_bits(8)
    }

    /// Determine how many bits we need to skip forward to realign the stream
    /// pointer with the next byte boundary.
    fn realignment_bits(&self) -> u32 {
        (8 - (self.bits_read % 3) as u32) % 3
    }

    /// Recognize a start code in the bitstream.
    ///
    /// H.263 start codes are particularly annoying because they are optionally
    /// aligned: encoders are free to insert up to eight bits of stuffing in
    /// order to achieve byte alignment. This function recognizes a start code
    /// up to eight bits ahead, taking specific care to ensure that the amount
    /// of stuffing bits present before the start code does not exceed the
    /// number of bits necessary to realign the bitstream to the next byte
    /// boundary.
    ///
    /// If the start code is recognized, then this function returns `true`, and
    /// also skips forward in the bitstream to the end of the start code.
    /// Otherwise, it returns `false`, and keeps the bitstream position where
    /// it is.
    pub fn recognize_start_code<T: BitReadable>(
        &mut self,
        code: T,
        bits_needed: u32,
    ) -> Result<bool> {
        let mut bits_needed_w_stuffing = bits_needed + self.realignment_bits();
        let mut maybe_code: T = self.peek_bits(bits_needed_w_stuffing)?;
        let mask: T = bitmask_from_popcount(bits_needed);

        while bits_needed_w_stuffing >= bits_needed {
            if (maybe_code & mask) == code {
                self.skip_bits(bits_needed_w_stuffing)?;
                return Ok(true);
            }

            bits_needed_w_stuffing -= 1;
            maybe_code = maybe_code >> 1;
        }

        Ok(false)
    }

    /// Read a variable-length code.
    ///
    /// The table consists of a list of `Entry`s. All `Fork`s in the table must
    /// have valid indicies and all links in the table must form a directed
    /// acyclic graph.
    ///
    /// This function yields `Error::InternalDecoderError` in the event that
    /// the given table is invalid, as well as all other unhandled I/O errors.
    /// In the event that an error is returned, the position of the bitstream
    /// is undefined. This is in contrast to fixed-length read functions which
    /// consistently leave the bitstream in the same position if enough bits
    /// for the type could not be read.
    pub fn read_vlc<T: Clone>(&mut self, table: &Table<T>) -> Result<T> {
        let mut index = 0;

        Ok(loop {
            match table.get(index) {
                Some(Entry::End(t)) => break t.clone(),
                Some(Entry::Fork(zero, one)) => {
                    let next_bit: u8 = self.read_bits(1)?;

                    if next_bit == 0 {
                        index = *zero;
                    } else {
                        index = *one;
                    }
                }
                None => return Err(Error::InternalDecoderError),
            }
        })
    }

    /// Yield a checkpoint value that can be used to abort a complex read
    /// operation.
    ///
    /// In the event that a read operation fails, the prior state of the
    /// internal buffer may be restored using the returned checkpoint.
    ///
    /// This is not an arbitrary seek mechanism: checkpoints are only valid
    /// for as long as the internal buffer retains the same amount of data, or
    /// more.
    fn checkpoint(&self) -> usize {
        self.bits_read
    }

    /// Restore a previously-created checkpoint.
    ///
    /// Upon restoring a checkpoint, all bits read from this reader will be
    ///
    /// Checkpoints handed to this function must be valid. Specifically, the
    /// internal buffer must not have been cleared (e.g. via `commit`) between
    /// the creation and use of this checkpoint.
    fn rollback(&mut self, checkpoint: usize) -> Result<()> {
        if checkpoint >= (self.buffer.len() * 8) {
            return Err(Error::InternalDecoderError);
        }

        self.bits_read = checkpoint;

        Ok(())
    }

    /// Invalidate any previous checkpoints and discard the internal buffer.
    ///
    /// This should only be called once all of the data necessary to represent
    /// a user-facing object has been read. All existing checkpoints will be
    /// invalidated.
    fn commit(&mut self) {
        self.buffer.drain(0..self.bits_read / 8);
        self.bits_read %= 8;
    }

    /// Run some struct-parsing code in such a way that it will not advance the
    /// bitstream position unless it successfully parses a value.
    ///
    /// Closures passed to this function must yield a `Result`. The buffer
    /// position will not be modified if the function yields an `Err`.
    ///
    /// TODO: This function does not discard successfully parsed buffer data
    /// via `commit` due to the lack of safety tracking on checkpoints. This
    /// function should be reentrant.
    pub fn with_transaction<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self) -> Result<T>,
    {
        let checkpoint = self.checkpoint();

        let result = f(self);

        if result.is_err() {
            self.rollback(checkpoint)?;
        }

        result
    }

    /// Run some struct-parsing code in such a way that it will not advance the
    /// bitstream position unless it successfully parses a value.
    ///
    /// Closures passed to this function must yield an `Option`, wrapped in a
    /// `Result`. The buffer position will not be modified if the function
    /// yields `Err` or `None`. Use `None` to signal that the desired data does
    /// not exist in the bitstream. The intended usage of this function is to
    /// allow parsing data that may be one of multiple types; ergo, in this
    /// case a `None` value means "try some other type".
    ///
    /// TODO: This function does not discard successfully parsed buffer data
    /// via `commit` due to the lack of safety tracking on checkpoints. This
    /// function should be reentrant.
    pub fn with_transaction_union<F, T>(&mut self, f: F) -> Result<Option<T>>
    where
        F: FnOnce(&mut Self) -> Result<Option<T>>,
    {
        let checkpoint = self.checkpoint();

        let result = f(self);

        match &result {
            Ok(None) | Err(_) => self.rollback(checkpoint)?,
            _ => {}
        };

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::decoder::reader::H263Reader;

    #[test]
    fn read_unaligned_bits() {
        let data = [0xFF, 0x72, 0x1C, 0x1F];
        let mut reader = H263Reader::from_source(&data[..]);

        assert_eq!(0x07, reader.read_bits(3).unwrap());
        assert_eq!(0x3E, reader.read_bits(6).unwrap());
        assert_eq!(0x721C1F, reader.read_bits(23).unwrap());
        reader.read_bits::<u8>(1).unwrap_err();
    }

    #[test]
    fn peek_bits() {
        let data = [0xFF, 0x72, 0x1C, 0x1F];
        let mut reader = H263Reader::from_source(&data[..]);

        assert_eq!(0x07, reader.peek_bits(3).unwrap());
        assert_eq!(0x3F, reader.peek_bits(6).unwrap());
        assert_eq!(0x7FB90E, reader.peek_bits(23).unwrap());
        reader.peek_bits::<u64>(64).unwrap_err();
    }

    #[test]
    fn bitmask_from_popcount() {
        assert_eq!(super::bitmask_from_popcount::<u8>(5), 0x1F);
        assert_eq!(super::bitmask_from_popcount::<u16>(5), 0x001F);
        assert_eq!(super::bitmask_from_popcount::<u8>(0), 0x0);
        assert_eq!(super::bitmask_from_popcount::<u16>(16), 0xFFFF);
        assert_eq!(super::bitmask_from_popcount::<u8>(16), 0xFF);
    }

    #[test]
    fn aligned_start_code() {
        let data = [0x00, 0x00, 0x80, 0x00];
        let mut reader = H263Reader::from_source(&data[..]);

        assert!(reader.recognize_start_code(0x000020, 22).unwrap());
    }

    #[test]
    fn stuffed_start_code() {
        let data = [0x00, 0x00, 0x08, 0x00];
        let mut reader = H263Reader::from_source(&data[..]);

        reader.skip_bits(4).unwrap();

        assert!(reader.recognize_start_code(0x000020, 22).unwrap());
    }

    #[test]
    fn misaligned_start_code() {
        let data = [0x00, 0x00, 0x08, 0x00];
        let mut reader = H263Reader::from_source(&data[..]);

        assert!(!reader.recognize_start_code(0x000020, 22).unwrap());
    }

    #[test]
    fn wrong_start_code() {
        let data = [0x00, 0x00, 0x80, 0x00];
        let mut reader = H263Reader::from_source(&data[..]);

        assert!(!reader.recognize_start_code(0x010020, 22).unwrap());
    }
}
