//! YUV-to-RGB decode

use lazy_static::lazy_static;

fn clamped_index(width: i32, height: i32, x: i32, y: i32) -> usize {
    (x.clamp(0, width - 1) + (y.clamp(0, height - 1) * width)) as usize
}

fn unclamped_index(width: i32, x: i32, y: i32) -> usize {
    (x + y * width) as usize
}

fn sample_chroma_for_luma(
    chroma: &[u8],
    chroma_width: usize,
    chroma_height: usize,
    luma_x: usize,
    luma_y: usize,
    clamp: bool,
) -> u8 {
    let width = chroma_width as i32;
    let height = chroma_height as i32;

    let sample_00;
    let sample_01;
    let sample_10;
    let sample_11;

    if clamp {
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

        sample_00 = chroma
            .get(clamped_index(width, height, chroma_x, chroma_y))
            .copied()
            .unwrap_or(0) as u16;
        sample_10 = chroma
            .get(clamped_index(width, height, chroma_x + 1, chroma_y))
            .copied()
            .unwrap_or(0) as u16;
        sample_01 = chroma
            .get(clamped_index(width, height, chroma_x, chroma_y + 1))
            .copied()
            .unwrap_or(0) as u16;
        sample_11 = chroma
            .get(clamped_index(width, height, chroma_x + 1, chroma_y + 1))
            .copied()
            .unwrap_or(0) as u16;
    } else {
        let chroma_x = (luma_x as i32 - 1) / 2;
        let chroma_y = (luma_y as i32 - 1) / 2;

        let base = unclamped_index(width, chroma_x, chroma_y);
        sample_00 = chroma.get(base).copied().unwrap_or(0) as u16;
        sample_10 = chroma.get(base + 1).copied().unwrap_or(0) as u16;
        sample_01 = chroma.get(base + chroma_width).copied().unwrap_or(0) as u16;
        sample_11 = chroma.get(base + chroma_width + 1).copied().unwrap_or(0) as u16;
    }

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

/// Precomputes and stores the linear functions for converting YUV (YCb'Cr' to be precise)
/// colors to RGB (sRGB-like, with gamma) colors, in signed 12.4 fixed-point integer format.
///
/// Since the incoming components are u8, and there is only ever at most 3 of them added
/// at once (when computing the G channel), only about 10 bits would be used if they were
/// u8 - so to get some more precision (and reduce potential stepping artifacts), might
/// as well use about 14 of the 15 (not counting the sign bit) available in i16.
struct LUTs {
    /// the contribution of the Y component into all RGB channels
    pub y_to_gray: [i16; 256],
    /// the contribution of the V (Cr') component into the R channel
    pub cr_to_r: [i16; 256],
    /// the contribution of the V (Cr') component into the G channel
    pub cr_to_g: [i16; 256],
    /// the contribution of the U (Cb') component into the G channel
    pub cb_to_g: [i16; 256],
    /// the contribution of the U (Cb') component into the B channel
    pub cb_to_b: [i16; 256],
}

impl LUTs {
    pub fn new() -> LUTs {
        let mut y_to_gray = [0i16; 256];
        let mut cr_to_r = [0i16; 256];
        let mut cr_to_g = [0i16; 256];
        let mut cb_to_g = [0i16; 256];
        let mut cb_to_b = [0i16; 256];

        // - Y needs to be remapped linearly from 16..235 to 0..255
        // - Cr' and Cb' (a.k.a. V and U) need to be remapped linearly from 16..240 to 0..255,
        //     then shifted to -128..127, and then scaled by the appropriate coefficients
        // - Finally all values are multiplied by 16 (1<<4) to turn them into 12.4 format, and rounded to integer.

        for i in 0..256 {
            let f = i as f32;

            // According to Wikipedia, these are the exact values from the
            // ITU-R BT.601 standard. See the last group of equations on:
            // https://en.wikipedia.org/wiki/YCbCr#ITU-R_BT.601_conversion
            let y2gray = (255.0 / 219.0) * (f - 16.0);
            let cr2r = (255.0 / 224.0) * 1.402 * (f - 128.0);
            let cr2g = -(255.0 / 224.0) * 1.402 * (0.299 / 0.587) * (f - 128.0);
            let cb2g = -(255.0 / 224.0) * 1.772 * (0.114 / 0.587) * (f - 128.0);
            let cb2b = (255.0 / 224.0) * 1.772 * (f - 128.0);

            // Converting to 12.4 format and rounding before storing
            y_to_gray[i] = (y2gray * 16.0).round() as i16;
            cr_to_r[i] = (cr2r * 16.0).round() as i16;
            cr_to_g[i] = (cr2g * 16.0).round() as i16;
            cb_to_g[i] = (cb2g * 16.0).round() as i16;
            cb_to_b[i] = (cb2b * 16.0).round() as i16;
        }

        LUTs {
            y_to_gray,
            cr_to_r,
            cr_to_g,
            cb_to_g,
            cb_to_b,
        }
    }
}

lazy_static! {
    static ref LUTS: LUTs = LUTs::new();
}

#[inline]
fn yuv_to_rgb(yuv: (u8, u8, u8), luts: &LUTs) -> (u8, u8, u8) {
    // We rely on the optimizers in rustc/LLVM to eliminate the bounds checks when indexing
    // into the fixed 256-long arrays in `luts` with indices coming in as `u8` parameters.
    // This is crucial for performance, as this function runs in a fairly tight loop, on all pixels.
    // I verified that this is actually happening, see here: https://rust.godbolt.org/z/vWzesYzbq
    // And benchmarking showed no time difference from an `unsafe` + `get_unchecked()` solution.

    let y = luts.y_to_gray[yuv.0 as usize];

    // The `(... + 8) >> 4` parts convert back from 12.4 fixed-point to `u8` with correct rounding.
    // (At least for positive numbers - any negative numbers that might occur will be clamped to 0 anyway.)
    let r = (y + luts.cr_to_r[yuv.2 as usize] + 8) >> 4;
    let g = (y + luts.cr_to_g[yuv.2 as usize] + luts.cb_to_g[yuv.1 as usize] + 8) >> 4;
    let b = (y + luts.cb_to_b[yuv.1 as usize] + 8) >> 4;

    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

#[inline]
fn convert_and_write_pixel(
    yuv: (u8, u8, u8),
    rgba: &mut Vec<u8>,
    width: usize,
    x_pos: usize,
    y_pos: usize,
    luts: &LUTs,
) {
    let (r, g, b) = yuv_to_rgb(yuv, luts);

    let base = (x_pos + y_pos * width) * 4;
    rgba[base] = r;
    rgba[base + 1] = g;
    rgba[base + 2] = b;
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
    let br_height = chroma_b.len() / br_width;

    // prefilling with 255, so the tight loop won't need to write to the alpha channel
    let mut rgba = vec![255; y.len() * 4];

    // making sure that the "is it initialized already?" check is only done once per frame by getting a direct reference
    let luts: &LUTs = &*LUTS;

    // do the bulk of the pixels faster, with no clamping, leaving out the edges
    for y_pos in 1..y_height - 1 {
        for x_pos in 1..y_width - 1 {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, x_pos, y_pos, false);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, x_pos, y_pos, false);

            convert_and_write_pixel(
                (y_sample, b_sample, r_sample),
                &mut rgba,
                y_width,
                x_pos,
                y_pos,
                luts,
            );
        }
    }

    // doing the sides with clamping
    for y_pos in 0..y_height {
        for x_pos in [0, y_width - 1].iter() {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, *x_pos, y_pos, true);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, *x_pos, y_pos, true);

            convert_and_write_pixel(
                (y_sample, b_sample, r_sample),
                &mut rgba,
                y_width,
                *x_pos,
                y_pos,
                luts,
            );
        }
    }

    // doing the top and bottom edges with clamping
    for y_pos in [0, y_height - 1].iter() {
        for x_pos in 0..y_width {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, x_pos, *y_pos, true);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, x_pos, *y_pos, true);

            convert_and_write_pixel(
                (y_sample, b_sample, r_sample),
                &mut rgba,
                y_width,
                x_pos,
                *y_pos,
                luts,
            );
        }
    }

    rgba
}

#[test]
fn test_yuv_to_rgb() {
    // From the H.263 Rec.:
    // Black = 16
    // White = 235
    // Zero colour difference = 128
    // Peak colour difference = 16 and 240

    // not quite black
    assert_eq!(yuv_to_rgb((17, 128, 128), &LUTS), (1, 1, 1));
    // exactly black
    assert_eq!(yuv_to_rgb((16, 128, 128), &LUTS), (0, 0, 0));
    // and clamping also works
    assert_eq!(yuv_to_rgb((15, 128, 128), &LUTS), (0, 0, 0));
    assert_eq!(yuv_to_rgb((0, 128, 128), &LUTS), (0, 0, 0));

    // not quite white
    assert_eq!(yuv_to_rgb((234, 128, 128), &LUTS), (254, 254, 254));
    // exactly white
    assert_eq!(yuv_to_rgb((235, 128, 128), &LUTS), (255, 255, 255));
    // and clamping also works
    assert_eq!(yuv_to_rgb((236, 128, 128), &LUTS), (255, 255, 255));
    assert_eq!(yuv_to_rgb((255, 128, 128), &LUTS), (255, 255, 255));

    // (16 + 235) / 2 = 125.5, for middle grays
    assert_eq!(yuv_to_rgb((125, 128, 128), &LUTS), (127, 127, 127));
    assert_eq!(yuv_to_rgb((126, 128, 128), &LUTS), (128, 128, 128));
}

// Inverse conversion, for testing purposes only
#[cfg(test)]
fn rgb_to_yuv(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    let (red, green, blue) = rgb;
    let (red, green, blue) = (red as f32, green as f32, blue as f32);

    // From the same Wikipedia article as LUTs::new()
    let y = 16.0 + (65.481 * red) / 255.0 + (128.553 * green) / 255.0 + (24.966 * blue) / 255.0;
    let u = 128.0 - (37.797 * red) / 255.0 - (74.203 * green) / 255.0 + (112.0 * blue) / 255.0;
    let v = 128.0 + (112.0 * red) / 255.0 - (93.786 * green) / 255.0 - (18.214 * blue) / 255.0;

    (y.round() as u8, u.round() as u8, v.round() as u8)
}

// The function used for testing should also have its own tests :)
#[test]
fn test_rgb_to_yuv() {
    // black is Y=16
    assert_eq!(rgb_to_yuv((0, 0, 0)), (16, 128, 128));
    assert_eq!(rgb_to_yuv((1, 1, 1)), (17, 128, 128));

    // white is Y=235
    assert_eq!(rgb_to_yuv((254, 254, 254)), (234, 128, 128));
    assert_eq!(rgb_to_yuv((255, 255, 255)), (235, 128, 128));

    assert_eq!(
        rgb_to_yuv((255, 0, 0)),
        (81, 90, 240) // 240 is the full color difference
    );
    assert_eq!(rgb_to_yuv((0, 255, 0)), (145, 54, 34));
    assert_eq!(
        rgb_to_yuv((0, 0, 255)),
        (41, 240, 110) // 240 is the full color difference
    );

    assert_eq!(
        rgb_to_yuv((0, 255, 255)),
        (170, 166, 16) // 16 is the full color difference
    );
    assert_eq!(rgb_to_yuv((255, 0, 255)), (106, 202, 222));
    assert_eq!(
        rgb_to_yuv((255, 255, 0)),
        (210, 16, 146) // 16 is the full color difference
    );
}

#[test]
fn test_rgb_yuv_rgb_roundtrip_sanity() {
    assert_eq!(yuv_to_rgb(rgb_to_yuv((0, 0, 0)), &LUTS), (0, 0, 0));
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((127, 127, 127)), &LUTS),
        (127, 127, 127)
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((128, 128, 128)), &LUTS),
        (128, 128, 128)
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((255, 255, 255)), &LUTS),
        (255, 255, 255)
    );

    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((255, 0, 0)), &LUTS),
        (254, 0, 0) // !!! there is a rounding error here
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((0, 255, 0)), &LUTS),
        (0, 255, 1) // !!! there is a rounding error here
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((0, 0, 255)), &LUTS),
        (0, 0, 255) // there is NO rounding error here
    );

    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((0, 255, 255)), &LUTS),
        (1, 255, 255) // !!! there is a rounding error here
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((255, 0, 255)), &LUTS),
        (255, 0, 254) // !!! there is a rounding error here
    );
    assert_eq!(
        yuv_to_rgb(rgb_to_yuv((255, 255, 0)), &LUTS),
        (255, 255, 0) // there is NO rounding error here
    );

    // the "tab10" palette:
    for rgb in [
        (31, 119, 180),
        (255, 127, 14),
        (44, 160, 44),
        (219, 39, 40),
        (148, 103, 189),
        (140, 86, 75),
        (227, 119, 194),
        (127, 127, 127),
        (188, 189, 34),
        (23, 190, 207),
    ] {
        let rgb2 = yuv_to_rgb(rgb_to_yuv(rgb), &LUTS);
        // Allowing for a difference of at most 1 on each component in both directions,
        // to account for the limited precision in YUV form, and two roundings
        assert!((rgb.0 as i32 - rgb2.0 as i32).abs() <= 1);
        assert!((rgb.1 as i32 - rgb2.1 as i32).abs() <= 1);
        assert!((rgb.2 as i32 - rgb2.2 as i32).abs() <= 1);
    }
}
