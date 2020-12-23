//! Block run decompression

use crate::types::Block;
use std::cmp::{max, min};

const DEZIGZAG_MAPPING: [(u8, u8); 64] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (0, 2),
    (1, 1),
    (2, 0),
    (3, 0),
    (2, 1),
    (1, 2),
    (0, 3),
    (0, 4),
    (1, 3),
    (2, 2),
    (3, 1),
    (4, 0),
    (5, 0),
    (4, 1),
    (3, 2),
    (2, 3),
    (1, 4),
    (0, 5),
    (0, 6),
    (1, 5),
    (2, 4),
    (3, 3),
    (4, 2),
    (5, 1),
    (6, 0),
    (7, 0),
    (6, 1),
    (5, 2),
    (4, 3),
    (3, 4),
    (2, 5),
    (1, 6),
    (0, 7),
    (1, 7),
    (2, 6),
    (3, 5),
    (4, 4),
    (5, 3),
    (6, 2),
    (7, 1),
    (7, 2),
    (6, 3),
    (5, 4),
    (4, 5),
    (3, 6),
    (2, 7),
    (3, 7),
    (4, 6),
    (5, 5),
    (6, 4),
    (7, 3),
    (7, 4),
    (6, 5),
    (5, 6),
    (4, 7),
    (5, 7),
    (6, 6),
    (7, 5),
    (7, 6),
    (6, 7),
    (7, 7),
];

/// Inverse RLE, dezigzag, and dequantize encoded block coefficient data.
///
/// `tcoefs` should be the list of run-length encoded coefficients. `levels`
/// will be filled with a row-major (x + y*8) decompressed list of
/// coefficients.
pub fn inverse_rle(encoded_block: &Block, levels: &mut [i16; 64], quant: i16) {
    let mut zigzag_index = 1;

    levels[0] = encoded_block.intradc.map(|l| l.into_level()).unwrap_or(0);

    for tcoef in encoded_block.tcoef.iter() {
        for _ in 0..tcoef.run {
            if zigzag_index >= DEZIGZAG_MAPPING.len() {
                return;
            }

            let (x, y) = DEZIGZAG_MAPPING[zigzag_index];
            let i = x as usize + (y as usize * 8);
            if i > levels.len() {
                break;
            }

            levels[i] = 0;
            zigzag_index += 1;
        }

        if zigzag_index >= DEZIGZAG_MAPPING.len() {
            return;
        }

        let (x, y) = DEZIGZAG_MAPPING[zigzag_index];
        let i = x as usize + (y as usize * 8);
        if i > levels.len() {
            break;
        }

        levels[i] = if quant % 2 == 1 {
            min(
                2047,
                max(
                    -2048,
                    tcoef.level.signum() * (quant * (2 * tcoef.level + 1)),
                ),
            )
        } else {
            min(
                2047,
                max(
                    -2048,
                    tcoef.level.signum() * (quant * (2 * tcoef.level + 1) - 1),
                ),
            )
        };
        zigzag_index += 1;
    }
}
