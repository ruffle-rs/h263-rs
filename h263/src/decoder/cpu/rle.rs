//! Block run decompression

use crate::types::Block;
use std::cmp::{max, min};

const DEZIGZAG_MAPPING: [u8; 64] = [
    0,  //(0, 0)
    1,  //(1, 0)
    8,  //(0, 1)
    16, //(0, 2)
    9,  //(1, 1)
    2,  //(2, 0)
    3,  //(3, 0)
    10, //(2, 1)
    17, //(1, 2)
    24, //(0, 3)
    32, //(0, 4)
    25, //(1, 3)
    18, //(2, 2)
    11, //(3, 1)
    4,  //(4, 0)
    5,  //(5, 0)
    12, //(4, 1)
    19, //(3, 2)
    26, //(2, 3)
    33, //(1, 4)
    40, //(0, 5)
    48, //(0, 6)
    41, //(1, 5)
    34, //(2, 4)
    27, //(3, 3)
    20, //(4, 2)
    13, //(5, 1)
    6,  //(6, 0)
    7,  //(7, 0)
    14, //(6, 1)
    21, //(5, 2)
    28, //(4, 3)
    35, //(3, 4)
    42, //(2, 5)
    48, //(1, 6)
    56, //(0, 7)
    57, //(1, 7)
    49, //(2, 6)
    43, //(3, 5)
    36, //(4, 4)
    29, //(5, 3)
    22, //(6, 2)
    15, //(7, 1)
    23, //(7, 2)
    30, //(6, 3)
    37, //(5, 4)
    44, //(4, 5)
    50, //(3, 6)
    58, //(2, 7)
    59, //(3, 7)
    51, //(4, 6)
    45, //(5, 5)
    38, //(6, 4)
    31, //(7, 3)
    39, //(7, 4)
    46, //(6, 5)
    52, //(5, 6)
    60, //(4, 7)
    61, //(5, 7)
    53, //(6, 6)
    47, //(7, 5)
    54, //(7, 6)
    62, //(6, 7)
    63, //(7, 7)
];

/// Inverse RLE, dezigzag, and dequantize encoded block coefficient data.
///
/// `tcoefs` should be the list of run-length encoded coefficients. `levels`
/// will be filled with a row-major (x + y*8) decompressed list of
/// coefficients.
pub fn inverse_rle(encoded_block: &Block, levels: &mut [i16; 64], quant: u8) {
    let mut zigzag_index = 1;

    *levels = [0; 64];
    levels[0] = encoded_block.intradc.map(|l| l.into_level()).unwrap_or(0);

    for tcoef in encoded_block.tcoef.iter() {
        for _ in 0..tcoef.run {
            if zigzag_index >= DEZIGZAG_MAPPING.len() {
                return;
            }

            let i: usize = DEZIGZAG_MAPPING[zigzag_index].into();

            levels[i] = 0;
            zigzag_index += 1;
        }

        if zigzag_index >= DEZIGZAG_MAPPING.len() {
            return;
        }

        let i: usize = DEZIGZAG_MAPPING[zigzag_index].into();
        let dequantized_level = quant as i16 * ((2 * tcoef.level.abs()) + 1);
        let parity = if quant % 2 == 1 { 0 } else { -1 };

        levels[i] = min(
            2047,
            max(-2048, tcoef.level.signum() * (dequantized_level + parity)),
        );
        zigzag_index += 1;
    }
}
