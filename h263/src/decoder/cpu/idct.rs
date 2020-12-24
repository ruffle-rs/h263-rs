//! Inverse discrete cosine transform

use std::cmp::{max, min};
use std::f32::consts::PI;

/// Given an IDCT block, transform it to the frequency domain.
///
/// The input of this function, `block_levels`, is an 8x8 block of
/// decompressed, dezigzagged transform coefficients in row-major (x + y*8)
/// order.
///
/// The output of this IDCT is represented as an 8x8 block of `u8`s, also in
/// row-major order. If this is an INTER block and reconstruction data exists
/// from the motion compensation / `gather` step, you should provide it here so
/// that the result of the IDCT is added to it here. Otherwise, you should
/// provide an array of zeroes.
pub fn idct_block(block_levels: &[i16; 64], output: &mut [u8; 64]) {
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0.0;

            for (i, coeff) in block_levels.iter().enumerate() {
                let u = i % 8;
                let v = i / 8;

                let cu = if u == 0 { 1.0 / f32::sqrt(2.0) } else { 1.0 };
                let cv = if v == 0 { 1.0 / f32::sqrt(2.0) } else { 1.0 };

                let cosx = f32::cos(PI * (2.0 * x as f32 + 1.0) * u as f32 / 16.0);
                let cosy = f32::cos(PI * (2.0 * y as f32 + 1.0) * v as f32 / 16.0);

                sum += cu * cv * *coeff as f32 * cosx * cosy;
            }

            let clipped_sum = min(255, max(-256, (sum / 4.0) as i16));
            let mocomp_pixel = output[x + (y * 8)] as u16 as i16;

            output[x + (y * 8)] = min(255, max(0, clipped_sum + mocomp_pixel)) as u8;
        }
    }
}
