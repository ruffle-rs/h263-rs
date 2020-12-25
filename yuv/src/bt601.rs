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
            let mut y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0) as f32;
            let mut b_sample = chroma_b
                .get((x_pos / 2) + ((y_pos / 2) * br_width))
                .copied()
                .unwrap_or(0) as f32;
            let mut r_sample = chroma_r
                .get((x_pos / 2) + ((y_pos / 2) * br_width))
                .copied()
                .unwrap_or(0) as f32;

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
