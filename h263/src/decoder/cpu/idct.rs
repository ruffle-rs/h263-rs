//! Inverse discrete cosine transform

use std::cmp::{max, min};
use std::f32::consts::PI;

/// Given a list of reconstructed IDCT levels, transform it out of the
/// frequency domain.
///
/// The input of this function, `block_levels`, is an arbitrarily-sized block of
/// decompressed, dezigzagged transform coefficients in row-major (x + y*8)
/// order. It must have a width equal to `samples_per_line` and dimensions
/// divisible by 8.
///
/// The `output` of this IDCT is represented as an arbitrarily-sized list of
/// `u8`s, also in row-major order and formatted in the same way as
/// `block_levels`. If this is an INTER block and predicted pixel data already
/// exists from the motion compensation step, you should pre-initialize the
/// output array with the result of said step so that the IDCT and summation
/// step can happen simultaneously. Otherwise, you should provide an array of
/// zeroes.
pub fn idct_channel(
    block_levels: &[i16],
    output: &mut [u8],
    samples_per_line: usize,
    output_samples_per_line: usize,
) {
    let output_height = output.len() / output_samples_per_line;

    for y in 0..output_height {
        for x in 0..output_samples_per_line {
            let mut sum = 0.0;
            let x_base = x & !0x7;
            let y_base = y & !0x7;

            for v in 0..8 {
                for u in 0..8 {
                    let coeff = block_levels[x_base + u + ((y_base + v) * samples_per_line)];

                    let cu = if u == 0 { 1.0 / f32::sqrt(2.0) } else { 1.0 };
                    let cv = if v == 0 { 1.0 / f32::sqrt(2.0) } else { 1.0 };

                    let cosx = f32::cos(PI * (2.0 * (x - x_base) as f32 + 1.0) * u as f32 / 16.0);
                    let cosy = f32::cos(PI * (2.0 * (y - y_base) as f32 + 1.0) * v as f32 / 16.0);

                    sum += cu * cv * coeff as f32 * cosx * cosy;
                }
            }

            let clipped_sum = min(255, max(-256, (sum / 4.0) as i16));
            let mocomp_pixel = output[x + (y * output_samples_per_line)] as u16 as i16;

            output[x + (y * output_samples_per_line)] =
                min(255, max(0, clipped_sum + mocomp_pixel)) as u8;
        }
    }
}
