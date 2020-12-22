//! YUV-to-RGB decode

fn clamp(v: f32) -> u8 {
    if v.is_nan() {
        return 0;
    }

    if v < 0.0 {
        return 0;
    }

    if v > 255.0 {
        return 255;
    }

    v as u8
}

/// Convert BT.601 YUV 4:2:2 data into RGB 1:1:1 data.
///
/// This function yields an RGBA picture with the same number of pixels as were
/// provided in the `y` picture. The `b` and `r` pictures will be resampled at
/// this stage, and the resulting picture will have color components mixed.
pub fn yuv422_to_rgba(y: &[u8], chroma_b: &[u8], chroma_r: &[u8], y_width: usize) -> Vec<u8> {
    let y_height = y.len() / y_width;
    let br_width = y_width / 2;

    let mut rgba = Vec::new();
    rgba.resize(y.len() * 4, 0);

    for y_pos in 0..y_height {
        for x_pos in 0..y_width {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0) as f32;
            let b_sample = chroma_b
                .get((x_pos / 2) + ((y_pos / 2) * br_width))
                .copied()
                .unwrap_or(0) as f32;
            let r_sample = chroma_r
                .get((x_pos / 2) + ((y_pos / 2) * br_width))
                .copied()
                .unwrap_or(0) as f32;

            let r = y_sample + r_sample * 1.13983;
            let g = y_sample + b_sample * -0.39465 + r_sample * 1.13983;
            let b = y_sample + b_sample * 2.03211;

            rgba[x_pos * 4 + y_pos * y_width * 4] = clamp(r);
            rgba[x_pos * 4 + y_pos * y_width * 4 + 1] = clamp(g);
            rgba[x_pos * 4 + y_pos * y_width * 4 + 2] = clamp(b);
        }
    }

    rgba
}
