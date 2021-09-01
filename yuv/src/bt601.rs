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
        // - Y needs to be remapped linearly from 16..235 to 0..255
        // - Cr' and Cb' (a.k.a. V and U) need to be remapped linearly from 16..240 to 0..255,
        //     then shifted to -128..127, and then scaled by the appropriate coefficients
        // - Finally all values are multiplied by 16 (1<<4) to turn them into 12.4 format, and rounded to integer.
        fn remap_luma(luma: f32) -> i16 {
            ((luma - 16.0) * (255.0 / (235.0 - 16.0)) * 16.0).round() as i16
        }
        fn remap_chroma(chroma: f32, coeff: f32) -> i16 {
            (((chroma - 16.0) * (255.0 / (240.0 - 16.0)) - 128.0) * coeff * 16.0).round() as i16
        }

        let mut y_to_gray = [0i16; 256];
        let mut cr_to_r = [0i16; 256];
        let mut cr_to_g = [0i16; 256];
        let mut cb_to_g = [0i16; 256];
        let mut cb_to_b = [0i16; 256];

        for i in 0..256 {
            let f = i as f32;
            y_to_gray[i] = remap_luma(f);
            cr_to_r[i] = remap_chroma(f, 1.370705); // sanity check: Cr' contributes "positively" to R
            cr_to_g[i] = remap_chroma(f, -0.698001); // sanity check: Cr' contributes "negatively" to G
            cb_to_g[i] = remap_chroma(f, -0.337633); // sanity check: Cb' contributes "negatively" to G
            cb_to_b[i] = remap_chroma(f, 1.732446); // sanity check: Cb' contributes "positively" to B
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
fn convert_and_write_pixel(yuv: (u8, u8, u8), rgba: &mut Vec<u8>, base: usize, luts: &LUTs) {
    let (y_sample, b_sample, r_sample) = yuv;

    // We rely on the optimizers in rustc/LLVM to eliminate the bounds checks when indexing
    // into the fixed 256-long arrays in `luts` with indices coming in as `u8` parameters.
    // This is crucial for performance, as this function runs in a fairly tight loop, on all pixels.
    // I verified that this is actually happening, see here: https://rust.godbolt.org/z/vWzesYzbq
    // And benchmarking showed no time difference from an `unsafe` + `get_unchecked()` solution.

    let y = luts.y_to_gray[y_sample as usize];

    // The `(... + 8) >> 4` parts convert back from 12.4 fixed-point to `u8` with correct rounding.
    // (At least for positive numbers - any negative numbers that might occur will be clamped to 0 anyway.)
    let r = (y + luts.cr_to_r[r_sample as usize] + 8) >> 4;
    let g = (y + luts.cr_to_g[r_sample as usize] + luts.cb_to_g[b_sample as usize] + 8) >> 4;
    let b = (y + luts.cb_to_b[b_sample as usize] + 8) >> 4;

    // the unsafes down here rely on the fact that base will not overflow rgba
    debug_assert!(base + 4 <= rgba.len()); // the + 4 is for the alpha channel, even though we're not writing that here
    *unsafe { rgba.get_unchecked_mut(base) } = r.clamp(0, 255) as u8;
    *unsafe { rgba.get_unchecked_mut(base + 1) } = g.clamp(0, 255) as u8;
    *unsafe { rgba.get_unchecked_mut(base + 2) } = b.clamp(0, 255) as u8;
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

    // This is a running index, pointing to the R component of the RGBA pixel to be written next.
    // It is advanced with additions, instead of recomputed with multiplications when addressing each pixel.
    let mut base: usize;

    // do the bulk of the pixels faster, with no clamping, leaving out the edges
    base = y_width * 4 + 4; // starting with the pixel on the second row and column (with indices of 1)
    for y_pos in 1..y_height - 1 {
        for x_pos in 1..y_width - 1 {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, x_pos, y_pos, false);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, x_pos, y_pos, false);

            convert_and_write_pixel((y_sample, b_sample, r_sample), &mut rgba, base, luts);
            base += 4; // advancing by one RGBA pixel
        }
        base += 8; // skipping the rightmost pixel, and the leftmost pixel in the next row
    }

    // doing the sides with clamping
    for y_pos in 0..y_height {
        for x_pos in [0, y_width - 1].iter() {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, *x_pos, y_pos, true);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, *x_pos, y_pos, true);

            // just recomputing for every pixel, as there aren't any long continuous runs here
            base = (x_pos + y_pos * y_width) * 4;

            convert_and_write_pixel((y_sample, b_sample, r_sample), &mut rgba, base, luts);
        }
    }

    // doing the top and bottom edges with clamping
    for y_pos in [0, y_height - 1].iter() {
        base = y_pos * y_width * 4; // resetting to the leftmost pixel of the rows
        for x_pos in 0..y_width {
            let y_sample = y.get(x_pos + y_pos * y_width).copied().unwrap_or(0);
            let b_sample =
                sample_chroma_for_luma(chroma_b, br_width, br_height, x_pos, *y_pos, true);
            let r_sample =
                sample_chroma_for_luma(chroma_r, br_width, br_height, x_pos, *y_pos, true);

            convert_and_write_pixel((y_sample, b_sample, r_sample), &mut rgba, base, luts);
            base += 4; // advancing by one RGBA pixel
        }
    }

    rgba
}
