//! Inverse discrete cosine transform

use lazy_static::lazy_static;
use std::cmp::{max, min};
use std::f32::consts::{FRAC_1_SQRT_2, PI};

/// The 1D basis function of the H.263 IDCT.
///
/// `spatial` is the spatial-domain position of the basis function, while
/// `freq` is the frequency-domain position the LEVEL came from.
fn basis(spatial: f32, freq: f32) -> f32 {
    f32::cos(PI * (2.0 * spatial + 1.0) * freq / 16.0)
}

lazy_static! {
    /// Lookup table for `basis`.
    ///
    /// The outer parameter represents all valid `spatial` inputs, while the
    /// inner represents all valid `freq` inputs.
    static ref BASIS_TABLE : [[f32; 8]; 8] = [
        [basis(0.0, 0.0), basis(0.0, 1.0), basis(0.0, 2.0), basis(0.0, 3.0),basis(0.0, 4.0),basis(0.0, 5.0),basis(0.0, 6.0),basis(0.0, 7.0)],
        [basis(1.0, 0.0), basis(1.0, 1.0), basis(1.0, 2.0), basis(1.0, 3.0),basis(1.0, 4.0),basis(1.0, 5.0),basis(1.0, 6.0),basis(1.0, 7.0)],
        [basis(2.0, 0.0), basis(2.0, 1.0), basis(2.0, 2.0), basis(2.0, 3.0),basis(2.0, 4.0),basis(2.0, 5.0),basis(2.0, 6.0),basis(2.0, 7.0)],
        [basis(3.0, 0.0), basis(3.0, 1.0), basis(3.0, 2.0), basis(3.0, 3.0),basis(3.0, 4.0),basis(3.0, 5.0),basis(3.0, 6.0),basis(3.0, 7.0)],
        [basis(4.0, 0.0), basis(4.0, 1.0), basis(4.0, 2.0), basis(4.0, 3.0),basis(4.0, 4.0),basis(4.0, 5.0),basis(4.0, 6.0),basis(4.0, 7.0)],
        [basis(5.0, 0.0), basis(5.0, 1.0), basis(5.0, 2.0), basis(5.0, 3.0),basis(5.0, 4.0),basis(5.0, 5.0),basis(5.0, 6.0),basis(5.0, 7.0)],
        [basis(6.0, 0.0), basis(6.0, 1.0), basis(6.0, 2.0), basis(6.0, 3.0),basis(6.0, 4.0),basis(6.0, 5.0),basis(6.0, 6.0),basis(6.0, 7.0)],
        [basis(7.0, 0.0), basis(7.0, 1.0), basis(7.0, 2.0), basis(7.0, 3.0),basis(7.0, 4.0),basis(7.0, 5.0),basis(7.0, 6.0),basis(7.0, 7.0)],
    ];

    static ref CUV_TABLE : [[f32; 8]; 8] = [
        [0.5, FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        [FRAC_1_SQRT_2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    ];
}

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
    block_levels: &[[[f32; 8]; 8]],
    output: &mut [u8],
    blk_per_line: usize,
    output_samples_per_line: usize,
) {
    let output_height = output.len() / output_samples_per_line;
    let basis_table = *BASIS_TABLE;
    let cuv_table = *CUV_TABLE;

    for y_base in 0..blk_per_line {
        for x_base in 0..=output_samples_per_line / 8 {
            let block_id = x_base + (y_base * blk_per_line);
            if block_id >= block_levels.len() {
                continue;
            }

            let block = &block_levels[block_id];

            for y_offset in 0..8 {
                for x_offset in 0..8 {
                    let x = x_base * 8 + x_offset;
                    let y = y_base * 8 + y_offset;
                    let mut sum = 0.0;

                    if x >= output_samples_per_line {
                        continue;
                    }

                    if y >= output_height {
                        continue;
                    }

                    for (u, coeff_line) in block.iter().enumerate() {
                        for (v, coeff) in coeff_line.iter().enumerate() {
                            let cuv = cuv_table[u][v];

                            let cosx = basis_table[x_offset][u];
                            let cosy = basis_table[y_offset][v];

                            sum += cuv * *coeff * cosx * cosy;
                        }
                    }

                    let clipped_sum = min(255, max(-256, (sum / 4.0).round() as i16));
                    let mocomp_pixel = output[x + (y * output_samples_per_line)] as u16 as i16;

                    output[x + (y * output_samples_per_line)] =
                        min(255, max(0, clipped_sum + mocomp_pixel)) as u8;
                }
            }
        }
    }
}
