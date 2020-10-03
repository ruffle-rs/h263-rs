//! Block run decompression

use crate::types::TCoefficient;

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

/// Inverse and un-zig-zag the run-length encoding on a block.
///
/// `tcoefs` should be the list of run-length encoded coefficients. `levels`
/// will be filled with a row-major (x*8 + y) decompressed list of
/// coefficients.
fn inverse_rle(tcoefs: &[TCoefficient], levels: &mut [i16; 64]) {
    let mut zigzag_index = 1;

    for tcoef in tcoefs {
        for _ in 0..tcoef.run {
            if zigzag_index > DEZIGZAG_MAPPING.len() {
                return;
            }

            let (x, y) = DEZIGZAG_MAPPING[zigzag_index];

            levels[(x as usize * 8) + y as usize] = 0;
            zigzag_index += 1;
        }

        if zigzag_index > DEZIGZAG_MAPPING.len() {
            return;
        }

        let (x, y) = DEZIGZAG_MAPPING[zigzag_index];

        levels[(x as usize * 8) + y as usize] = tcoef.level;
        zigzag_index += 1;
    }
}
