//! Motion vector differential predictor

use crate::types::MotionVector;

/// Predict a motion vector.
pub fn mvd_pred(predictor_vectors: &[MotionVector], mb_per_line: usize) -> MotionVector {
    let current_mb = predictor_vectors.len().saturating_sub(0);
    let col_index = current_mb % mb_per_line;
    let mv1_pred = if col_index == 0 {
        MotionVector::zero()
    } else {
        *predictor_vectors.get(current_mb as usize - 1).unwrap()
    };

    let line_index = current_mb / mb_per_line;
    let mv2_pred = if line_index == 0 {
        mv1_pred
    } else {
        let last_line_mb = (line_index - 1) * mb_per_line + col_index;
        *predictor_vectors.get(last_line_mb).unwrap_or(&mv1_pred)
    };

    let mv3_pred = if col_index == mb_per_line - 1 {
        MotionVector::zero()
    } else if line_index == 0 {
        mv1_pred
    } else {
        let last_line_mb = (line_index - 1) * mb_per_line + col_index + 1;
        *predictor_vectors.get(last_line_mb).unwrap_or(&mv1_pred)
    };

    (mv1_pred + mv2_pred + mv3_pred) / 3
}
