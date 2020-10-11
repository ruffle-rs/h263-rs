//! Intra block data collection

use crate::decoder::macroblock::DecodedMacroblock;
use crate::decoder::picture::DecodedPicture;
use crate::types::{MacroblockType, MotionVector};

fn read_sample(pixel_array: &[u8], samples_per_row: usize, pos: (isize, isize)) -> u8 {
    let (x, y) = pos;

    let x = if x < 0 {
        0
    } else if x >= samples_per_row as isize {
        samples_per_row.saturating_sub(1)
    } else {
        x as usize
    };

    let height = pixel_array.len() / samples_per_row;

    let y = if y < 0 {
        0
    } else if y >= height as isize {
        height.saturating_sub(1)
    } else {
        y as usize
    };

    pixel_array
        .get(x * samples_per_row + y)
        .copied()
        .unwrap_or(0)
}

/// Linear interpolation between two values by some percentage.
fn lerp(sample_a: u8, sample_b: u8, amount_b: f32) -> u8 {
    (sample_a as f32 * (1.0 - amount_b) + sample_b as f32 * amount_b) as u8
}

/// Copy pixel data from a pixel array, motion-compensate it, and fill a block
/// with the given data.
fn gather_block(
    pixel_array: &[u8],
    samples_per_row: usize,
    pos: (u16, u16),
    mv: MotionVector,
    target: &mut [u8; 64],
) {
    let ((x_delta, x_interp), (y_delta, y_interp)) = mv.into_whole_and_fractional();

    let x = pos.0 as isize + x_delta as isize;
    let y = pos.1 as isize + y_delta as isize;

    for (i, u) in (x..x + 8).enumerate() {
        for (j, v) in (y..y + 8).enumerate() {
            let sample_0_0 = read_sample(pixel_array, samples_per_row, (u, v));
            let sample_1_0 = read_sample(pixel_array, samples_per_row, (u + 1, v));
            let sample_0_1 = read_sample(pixel_array, samples_per_row, (u, v + 1));
            let sample_1_1 = read_sample(pixel_array, samples_per_row, (u + 1, v + 1));

            let sample_mid_0 = lerp(sample_0_0, sample_1_0, x_interp);
            let sample_mid_1 = lerp(sample_0_1, sample_1_1, x_interp);

            target[i * 8 + j] = lerp(sample_mid_0, sample_mid_1, y_interp);
        }
    }
}

/// Copy macroblock data from a previously decoded reference picture into
/// blocks.
///
/// For `INTER` coded macroblocks, the gather process performs motion
/// compensation using the reference picture to produce the block data to be
/// mixed with the result of the IDCT.
///
/// For `INTRA` coded macroblocks, the returned set of blocks will be all
/// zeroes.
pub fn gather(
    mb_type: MacroblockType,
    reference_picture: &DecodedPicture,
    pos: (u16, u16),
    mv: [MotionVector; 4],
) -> DecodedMacroblock {
    let mut dmb = DecodedMacroblock::new();

    if mb_type.is_inter() {
        let luma_samples_per_row = reference_picture
            .format()
            .into_width_and_height()
            .unwrap()
            .0 as usize;

        gather_block(
            reference_picture.as_luma(),
            luma_samples_per_row,
            pos,
            mv[0],
            dmb.luma_mut(0),
        );
        gather_block(
            reference_picture.as_luma(),
            luma_samples_per_row,
            (pos.0 + 8, pos.1),
            mv[1],
            dmb.luma_mut(1),
        );
        gather_block(
            reference_picture.as_luma(),
            luma_samples_per_row,
            (pos.0, pos.1 + 8),
            mv[2],
            dmb.luma_mut(2),
        );
        gather_block(
            reference_picture.as_luma(),
            luma_samples_per_row,
            (pos.0 + 8, pos.1 + 8),
            mv[3],
            dmb.luma_mut(3),
        );

        let mv_chr = (mv[0] + mv[1] + mv[2] + mv[3]).average_sum_of_mvs();
        let chroma_samples_per_row = luma_samples_per_row / 2;

        gather_block(
            reference_picture.as_chroma_b(),
            chroma_samples_per_row,
            (pos.0 / 2, pos.1 / 2),
            mv_chr,
            dmb.chroma_b_mut(),
        );
        gather_block(
            reference_picture.as_chroma_r(),
            chroma_samples_per_row,
            (pos.0 / 2, pos.1 / 2),
            mv_chr,
            dmb.chroma_r_mut(),
        );
    }

    dmb
}
