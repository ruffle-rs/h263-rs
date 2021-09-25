//! YUV-to-RGB decode

use lazy_static::lazy_static;

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
    let (y, cb, cr) = yuv;

    // We rely on the optimizers in rustc/LLVM to eliminate the bounds checks when indexing
    // into the fixed 256-long arrays in `luts` with indices coming in as `u8` parameters.
    // This is crucial for performance, as this function runs in a fairly tight loop, on all pixels.
    // I verified that this is actually happening, see here: https://rust.godbolt.org/z/vWzesYzbq
    // And benchmarking showed no time difference from an `unsafe` + `get_unchecked()` solution.
    let gray = luts.y_to_gray[y as usize];

    // The `(... + 8) >> 4` parts convert back from 12.4 fixed-point to `u8` with correct rounding.
    // (At least for positive numbers - any negative numbers that might occur will be clamped to 0 anyway.)
    let r = (gray + luts.cr_to_r[cr as usize] + 8) >> 4;
    let g = (gray + luts.cr_to_g[cr as usize] + luts.cb_to_g[cb as usize] + 8) >> 4;
    let b = (gray + luts.cb_to_b[cb as usize] + 8) >> 4;

    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

/// Convert planar YUV 4:2:0 data into interleaved RGBA 8888 data.
///
/// This function yields an RGBA picture with the same number of pixels as were
/// provided in the `y` picture. The `chroma_b` and `chroma_r` samples are
/// simply reused without any interpolation for all four corresponding pixels.
/// This is not the most correct, or nicest, but it's what Flash Player does.
///
/// Preconditions:
///  - `y.len()` must be an integer multiple of `y_width`
///  - `chroma_b.len()` and `chroma_r.len()` must both be integer multiples of `br_width`
///  - `chroma_b` and `chroma_r` must be the same size
///  - `br_width` must be half of `y_width`, rounded up
///  - With `y_height` computed as `y.len() / y_width`, and `br_height` as `chroma_b.len() / br_width`:
///    `br_height` must be half of `y_height`, rounded up
pub fn yuv420_to_rgba(
    y: &[u8],
    chroma_b: &[u8],
    chroma_r: &[u8],
    y_width: usize,
    br_width: usize,
) -> Vec<u8> {
    // Shortcut for the no-op case to avoid all kinds of overflows below
    if y.is_empty() {
        debug_assert_eq!(chroma_b.len(), 0);
        debug_assert_eq!(chroma_r.len(), 0);
        debug_assert_eq!(y_width, 0);
        debug_assert_eq!(br_width, 0);
        return vec![];
    }

    debug_assert_eq!(y.len() % y_width, 0);
    debug_assert_eq!(chroma_b.len() % br_width, 0);
    debug_assert_eq!(chroma_r.len() % br_width, 0);
    debug_assert_eq!(chroma_b.len(), chroma_r.len());

    let y_height = y.len() / y_width;
    let br_height = chroma_b.len() / br_width;

    // the + 1 is for rounding odd numbers up
    debug_assert_eq!((y_width + 1) / 2, br_width);
    debug_assert_eq!((y_height + 1) / 2, br_height);

    let mut rgba = vec![0; y.len() * 4];
    let rgba_stride = y_width * 4; // 4 bytes per pixel, interleaved

    // making sure that the "is it initialized already?" check is only done once per frame by getting a direct reference
    let luts: &LUTs = &*LUTS;

    // Iteration is done in a row-major order to fit the slice layouts.
    for luma_rowindex in 0..y_height {
        let chroma_rowindex = luma_rowindex / 2;

        let y_row = &y[luma_rowindex * y_width..(luma_rowindex + 1) * y_width];
        let cb_row = &chroma_b[chroma_rowindex * br_width..(chroma_rowindex + 1) * br_width];
        let cr_row = &chroma_r[chroma_rowindex * br_width..(chroma_rowindex + 1) * br_width];
        let rgba_row = &mut rgba[luma_rowindex * rgba_stride..(luma_rowindex + 1) * rgba_stride];

        // Iterating on 2 pixels at a time, leaving off the last one if width is odd.
        let y_iter = y_row.chunks_exact(2);
        let cb_iter = cb_row.iter();
        let cr_iter = cr_row.iter();
        // Similar to how Y is iterated on, but with 4 channels per pixel
        let rgba_iter = rgba_row.chunks_exact_mut(8);

        for (((y, cb), cr), rgba) in y_iter.zip(cb_iter).zip(cr_iter).zip(rgba_iter) {
            let rgb0 = yuv_to_rgb((y[0], *cb, *cr), luts);
            let rgb1 = yuv_to_rgb((y[1], *cb, *cr), luts);
            // The output alpha values are fixed
            rgba.copy_from_slice(&[rgb0.0, rgb0.1, rgb0.2, 255, rgb1.0, rgb1.1, rgb1.2, 255]);
        }

        // On odd wide pictures, the last pixel is not covered by the iteration above,
        // but is included in y_row and rgba_row.
        if y_width % 2 == 1 {
            let y = y_row.last().unwrap();
            let cb = cb_row.last().unwrap();
            let cr = cr_row.last().unwrap();

            let rgb = yuv_to_rgb((*y, *cb, *cr), luts);

            rgba_row[rgba_stride - 4..rgba_stride].copy_from_slice(&[rgb.0, rgb.1, rgb.2, 255])
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

#[test]
fn test_yuv420_to_rgba() {
    // empty picture
    assert_eq!(yuv420_to_rgba(&[], &[], &[], 0, 0), vec![0u8; 0]);

    // a single pixel picture
    assert_eq!(
        yuv420_to_rgba(&[125u8], &[128u8], &[128u8], 1, 1),
        vec![127u8, 127u8, 127u8, 255u8]
    );

    // a 2x2 grey picture with a single chroma sample (well, one Cb and one Cr)
    #[rustfmt::skip]
    assert_eq!(
        yuv420_to_rgba(&[125u8, 125u8, 125u8, 125u8], &[128u8], &[128u8], 2, 1),
        vec![
            127u8, 127u8, 127u8, 255u8, 127u8, 127u8, 127u8, 255u8,
            127u8, 127u8, 127u8, 255u8, 127u8, 127u8, 127u8, 255u8,
        ]
    );

    // a 2x2 black-and-white checkerboard picture
    #[rustfmt::skip]
    assert_eq!(
        yuv420_to_rgba(&[16u8, 235u8, 235u8, 16u8], &[128u8], &[128u8], 2, 1),
        vec![
              0u8,   0u8,   0u8, 255u8, 255u8, 255u8, 255u8, 255u8,
            255u8, 255u8, 255u8, 255u8,   0u8,   0u8,   0u8, 255u8,
        ]
    );

    // a 3x2 picture, black on the left, white on the right, grey in the middle
    #[rustfmt::skip]
    assert_eq!(
        yuv420_to_rgba(&[0u8, 125u8, 235u8,  0u8, 125u8, 235u8], &[128u8, 128u8, ], &[128u8, 128u8,], 3, 2),
        vec![
              0u8,   0u8,   0u8, 255u8,  127u8, 127u8, 127u8, 255u8,  255u8, 255u8, 255u8, 255u8,
              0u8,   0u8,   0u8, 255u8,  127u8, 127u8, 127u8, 255u8,  255u8, 255u8, 255u8, 255u8,
        ]
    );

    // notes:
    // (81, 90, 240) is full red in YUV
    // (145, 54, 34) is full green in YUV

    // A 3x3 picture, red on the top, green on the bottom.
    #[rustfmt::skip]
    assert_eq!(
        yuv420_to_rgba(
            &[ 81u8,  81u8,  81u8,
              125u8, 125u8, 125u8,
              145u8, 145u8, 145u8],
            &[ 90u8,  90u8,
               54u8,  54u8],
            &[240u8,  240u8,
               34u8,  34u8],
            3, 2),
        vec![
            254u8,   0u8,   0u8, 255u8,  254u8,   0u8,   0u8, 255u8,  254u8,   0u8,   0u8, 255u8, // red, with rounding error
            255u8,  51u8,  50u8, 255u8,  255u8,  51u8,  50u8, 255u8,  255u8,  51u8,  50u8, 255u8, // orangish
              0u8, 255u8,   1u8, 255u8,    0u8, 255u8,   1u8, 255u8,    0u8, 255u8,   1u8, 255u8, // green, with rounding error
        ]
    );
    // The middle row looks fairly off when converted back to YUV: should be (125, 90, 240), but is (112, 97, 218)
    // However, when converted back again to RGB, these are (255, 51, 50) and (255, 51, 49), respectively. So, close enough.

    // A 3x3 picture, red on the left, green on the right. Transpose of the above.
    #[rustfmt::skip]
    assert_eq!(
        yuv420_to_rgba(
            &[ 81u8, 125u8, 145u8,
               81u8, 125u8, 145u8,
               81u8, 125u8, 145u8],
            &[ 90u8,  54u8,
               90u8,  54u8],
            &[240u8,   34u8,
              240u8,   34u8],
            3, 2),
        vec![
            254u8,   0u8,   0u8, 255u8,  255u8,  51u8,  50u8, 255u8,   0u8, 255u8,   1u8, 255u8,
            254u8,   0u8,   0u8, 255u8,  255u8,  51u8,  50u8, 255u8,   0u8, 255u8,   1u8, 255u8,
            254u8,   0u8,   0u8, 255u8,  255u8,  51u8,  50u8, 255u8,   0u8, 255u8,   1u8, 255u8,
        ]
    );

    // The middle row/column of pixels use the top/left row/column of chroma samples:
    assert_eq!(yuv_to_rgb((125, 90, 240), &LUTS), (255, 51, 50));
}
