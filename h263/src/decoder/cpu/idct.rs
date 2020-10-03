//! Inverse discrete cosine transform

use std::f32::consts::PI;

/// Given an IDCT block, transform it to the frequency domain.
///
/// The input of this function, `block_levels`, is an 8x8 block of
/// decompressed, dezigzagged transform coefficients in row-major (x*8 + y)
/// order.
///
/// The output of this IDCT is represented as an 8x8 block of `i16`s, also in
/// row-major order, which can optionally be added to a previous frame's
/// prediction data before it is clipped to the range of a `u8`.
pub fn idct_block(block_levels: &[i16; 64], output: &mut [i16; 64]) {
    for x in 0..8 {
        for y in 0..8 {
            let mut sum = 0.0;

            for (i, coeff) in block_levels.iter().enumerate() {
                let cu = if i % 8 == 0 {
                    1.0 / f32::sqrt(2.0)
                } else {
                    0.0
                };
                let cv = if i / 8 == 0 {
                    1.0 / f32::sqrt(2.0)
                } else {
                    0.0
                };
                let cosx = f32::cos(PI * (2.0 * x as f32 + 1.0) * (i % 8) as f32 / 16.0);
                let cosy = f32::cos(PI * (2.0 * y as f32 + 1.0) * (i / 8) as f32 / 16.0);
                sum += cu * cv * *coeff as f32 * cosx * cosy;
            }

            output[x * 8 + y] = sum as i16;
        }
    }
}
