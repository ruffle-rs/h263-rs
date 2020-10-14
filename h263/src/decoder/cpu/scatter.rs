//! Decoded macroblock storage

use crate::decoder::macroblock::DecodedMacroblock;
use crate::decoder::picture::DecodedPicture;

/// Scatter an individual block into a pixel data array.
///
/// Pixel data and block data are assumed to be in row-major (x + y*width)
/// order.
fn scatter_block(
    pixel_data: &mut [u8],
    samples_per_row: usize,
    pos: (u16, u16),
    block_data: &[u8; 64],
) {
    for (u, x) in (pos.0 as usize..pos.0 as usize + 8).enumerate() {
        for (v, y) in (pos.1 as usize..pos.1 as usize + 8).enumerate() {
            if x < samples_per_row {
                if let Some(pixel) = pixel_data.get_mut(x + y * samples_per_row) {
                    *pixel = block_data[u + v * 8];
                }
            }
        }
    }
}

/// Copy decoded macroblock data back into a picture.
pub fn scatter(
    new_picture: &mut DecodedPicture,
    new_macroblock: DecodedMacroblock,
    pos: (u16, u16),
) {
    let luma_samples_per_row = new_picture.luma_samples_per_row();
    scatter_block(
        new_picture.as_luma_mut(),
        luma_samples_per_row,
        pos,
        new_macroblock.as_luma(0),
    );
    scatter_block(
        new_picture.as_luma_mut(),
        luma_samples_per_row,
        (pos.0 + 8, pos.1),
        new_macroblock.as_luma(1),
    );
    scatter_block(
        new_picture.as_luma_mut(),
        luma_samples_per_row,
        (pos.0, pos.1 + 8),
        new_macroblock.as_luma(2),
    );
    scatter_block(
        new_picture.as_luma_mut(),
        luma_samples_per_row,
        (pos.0 + 8, pos.1 + 8),
        new_macroblock.as_luma(3),
    );

    let chroma_samples_per_row = new_picture.chroma_samples_per_row();
    scatter_block(
        new_picture.as_chroma_b_mut(),
        chroma_samples_per_row,
        (pos.0 / 2, pos.1 / 2),
        new_macroblock.as_chroma_b(),
    );
    scatter_block(
        new_picture.as_chroma_r_mut(),
        chroma_samples_per_row,
        (pos.0 / 2, pos.1 / 2),
        new_macroblock.as_chroma_r(),
    );
}
