//! YUV-to-RGB decode

fn clamp(v: f32) -> u8 {
    (v + 0.5).max(0.0).min(255.0) as u8
}

pub fn clamped_index(width: i32, height: i32, x: i32, y: i32) -> usize {
    (x.max(0).min(width - 1) + (y.max(0).min(height - 1) * width)) as usize
}

pub fn sample_chroma_for_luma(
    chroma: &[u8],
    chroma_width: usize,
    luma_x: usize,
    luma_y: usize,
) -> u8 {
    let width = chroma_width as i32;
    let height = chroma.len() as i32 / width;

    let chroma_x = if luma_x == 0 {
        -1
    } else {
        (luma_x as i32 - 1) / 2
    };
    let chroma_y = if luma_y == 0 {
        -1
    } else {
        (luma_y as i32 - 1) / 2
    };

    let sample_00 = chroma
        .get(clamped_index(width, height, chroma_x, chroma_y))
        .copied()
        .unwrap_or(0) as u16;
    let sample_10 = chroma
        .get(clamped_index(width, height, chroma_x + 1, chroma_y))
        .copied()
        .unwrap_or(0) as u16;
    let sample_01 = chroma
        .get(clamped_index(width, height, chroma_x, chroma_y + 1))
        .copied()
        .unwrap_or(0) as u16;
    let sample_11 = chroma
        .get(clamped_index(width, height, chroma_x + 1, chroma_y + 1))
        .copied()
        .unwrap_or(0) as u16;

    let interp_left = luma_x % 2 != 0;
    let interp_top = luma_y % 2 != 0;

    let mut sample: u16 = 0;
    sample += sample_00 * if interp_left { 3 } else { 1 };
    sample += sample_10 * if interp_left { 1 } else { 3 };

    sample += sample_01 * if interp_left { 3 } else { 1 };
    sample += sample_11 * if interp_left { 1 } else { 3 };

    sample += sample_00 * if interp_top { 3 } else { 1 };
    sample += sample_01 * if interp_top { 1 } else { 3 };

    sample += sample_10 * if interp_top { 3 } else { 1 };
    sample += sample_11 * if interp_top { 1 } else { 3 };

    ((sample + 8) / 16) as u8
}

/// Convert YUV 4:2:0 data into RGB 1:1:1 data.
///
/// This function yields an RGBA picture with the same number of pixels as were
/// provided in the `y` picture. The `b` and `r` pictures will be resampled at
/// this stage, and the resulting picture will have color components mixed.
pub fn yuv420_to_rgba(
    y: &[u8],
    chroma_b: &[u8],
    chroma_r: &[u8],
    y_width: usize,
    br_width: usize,
) -> Vec<u8> {
    let y_height = y.len() / y_width;

    let mut rgba = Vec::new();
    rgba.resize(y.len() * 4, 0);

    for y_pos in 0..y_height {
        for x_pos in 0..y_width {
            let mut y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0) as f32;

            let mut b_sample = sample_chroma_for_luma(chroma_b, br_width, x_pos, y_pos) as f32;
            let mut r_sample = sample_chroma_for_luma(chroma_r, br_width, x_pos, y_pos) as f32;

            y_sample = (y_sample - 16.0) * (255.0 / (235.0 - 16.0));
            b_sample = (b_sample - 16.0) * (255.0 / (240.0 - 16.0));
            r_sample = (r_sample - 16.0) * (255.0 / (240.0 - 16.0));

            b_sample -= 128.0;
            r_sample -= 128.0;

            let r = y_sample + r_sample * 1.370705;
            let g = y_sample + r_sample * -0.698001 + b_sample * -0.337633;
            let b = y_sample + b_sample * 1.732446;

            rgba[x_pos * 4 + y_pos * y_width * 4] = clamp(r);
            rgba[x_pos * 4 + y_pos * y_width * 4 + 1] = clamp(g);
            rgba[x_pos * 4 + y_pos * y_width * 4 + 2] = clamp(b);
            rgba[x_pos * 4 + y_pos * y_width * 4 + 3] = 255;
        }
    }

    rgba
}
