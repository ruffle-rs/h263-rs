//! Motion vector differential predictor

use crate::decoder::picture::DecodedPicture;
use crate::types::{HalfPel, MotionVector, MotionVectorRange, PictureOption};

/// Produce a candidate motion vector predictor from the current set of decoded
/// motion vectors.
pub fn predict_candidate(predictor_vectors: &[MotionVector], mb_per_line: usize) -> MotionVector {
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

/// Decode a single component of a motion vector.
pub fn halfpel_decode(
    current_picture: &DecodedPicture,
    running_options: PictureOption,
    predictor: HalfPel,
    mvd: HalfPel,
    is_x: bool,
) -> HalfPel {
    let mut range = HalfPel::STANDARD_RANGE;
    let mut out = mvd + predictor;

    if running_options.contains(PictureOption::UnrestrictedMotionVectors)
        && !current_picture.as_header().has_plusptype
    {
        if predictor.is_mv_within_range(HalfPel::STANDARD_RANGE) {
            return out;
        } else {
            range = HalfPel::EXTENDED_RANGE;
        }
    } else if running_options.contains(PictureOption::UnrestrictedMotionVectors)
        && matches!(
            current_picture.as_header().motion_vector_range,
            Some(MotionVectorRange::Extended)
        )
    {
        if is_x {
            range = match current_picture.format().into_width_and_height() {
                Some((0..=352, _)) => HalfPel::EXTENDED_RANGE,
                Some((356..=704, _)) => HalfPel::EXTENDED_RANGE_QUADCIF,
                Some((708..=1408, _)) => HalfPel::EXTENDED_RANGE_SIXTEENCIF,
                Some((1412..=u16::MAX, _)) => HalfPel::EXTENDED_RANGE_BEYONDCIF,
                _ => HalfPel::EXTENDED_RANGE, // this is actually an error condition.
            };
        } else {
            range = match current_picture.format().into_width_and_height() {
                Some((_, 0..=288)) => HalfPel::EXTENDED_RANGE,
                Some((_, 292..=576)) => HalfPel::EXTENDED_RANGE_QUADCIF,
                Some((_, 580..=u16::MAX)) => HalfPel::EXTENDED_RANGE_SIXTEENCIF,
                _ => HalfPel::EXTENDED_RANGE, // this is actually an error condition.
            };
        }
    } else if matches!(
        current_picture.as_header().motion_vector_range,
        Some(MotionVectorRange::Unlimited)
    ) {
        // Note that we explicitly allow the Unlimited flag to exist without
        // the presence of the UMV option. This is because Sorenson doesn't use
        // the UMV option.
        return out;
    }

    if !out.is_mv_within_range(range) {
        out = mvd.invert() + predictor;
    }

    out
}

/// Given an encoded motion vector and it's predictor, produce the decoded,
/// ready-to-use motion vector.
pub fn mv_decode(
    current_picture: &DecodedPicture,
    running_options: PictureOption,
    predictor: MotionVector,
    mvd: MotionVector,
) -> MotionVector {
    let (mvx, mvy) = mvd.into();
    let (cpx, cpy) = predictor.into();

    let out_x = halfpel_decode(current_picture, running_options, cpx, mvx, true);
    let out_y = halfpel_decode(current_picture, running_options, cpy, mvy, false);

    (out_x, out_y).into()
}
