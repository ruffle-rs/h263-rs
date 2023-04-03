//! Rust implementation of a deblocking filter inspired by ITU-T H.263 Annex J.
//! This is intended to be used as a postprocessing step, not as a loop filter.

/// Table J.2/H.263 - Relationship between QUANT and STRENGTH of filter; [0] is not to be used
pub const QUANT_TO_STRENGTH: [u8; 32] = [
    0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 10, 11, 11, 11,
    12, 12, 12,
];

mod scalar_impl {
    /// Figure J.2/H.263 – Parameter d1 as a function of parameter d for deblocking filter mode
    #[inline]
    fn up_down_ramp(x: i16, strength: i16) -> i16 {
        x.signum() * (x.abs() - (2 * (x.abs() - strength)).max(0)).max(0)
    }

    /// Clips x to the range +/- abs(lim)
    #[inline]
    fn clipd1(x: i16, lim: i16) -> i16 {
        x.clamp(-lim.abs(), lim.abs())
    }

    /// Operates the filter on a set of four (clipped) pixel values on a horizontal or
    /// vertical line of the picture, of which A and B belong to one block, and C and D
    /// belong to a neighbouring block which is to the right of or below the first block.
    /// Figure J.1 shows examples for the position of these pixels.
    #[allow(non_snake_case)]
    #[inline]
    pub fn process(A: &mut u8, B: &mut u8, C: &mut u8, D: &mut u8, strength: u8) {
        debug_assert!((1..=12).contains(&strength));

        let (a16, b16, c16, d16) = (*A as i16, *B as i16, *C as i16, *D as i16);

        let d: i16 = (a16 - 4 * b16 + 4 * c16 - d16) / 8;
        let d1: i16 = up_down_ramp(d, strength as i16);
        let d2: i16 = clipd1((a16 - d16) / 4, d1 / 2);

        *A = (a16 - d2) as u8;
        *B = (b16 + d1).clamp(0, 255) as u8;
        *C = (c16 - d1).clamp(0, 255) as u8;
        *D = (d16 + d2) as u8;
    }
}

mod simd_impl {
    use std::ops::Shr;
    use wide::{i16x8, CmpGt, CmpLt};

    /// Utility mimicking `i16::signum` for `i16x8` - see https://github.com/Lokathor/wide/issues/131
    #[inline]
    fn signum_simd(x: i16x8) -> i16x8 {
        // NOTE: The `true` return value of these comparisons is all `1` bits,
        // which is numerically `-1`, hence the reversed usage ot `lt` and `gt`,
        // as compared to scalar comparisons involving eg. `i16` and `bool`.
        x.cmp_lt(i16x8::ZERO) - x.cmp_gt(i16x8::ZERO)
    }

    /// Utility mimicking `i16::clamp` for `i16x8` - see: https://github.com/Lokathor/wide/issues/131
    #[inline]
    fn clamp_simd(x: i16x8, min: i16x8, max: i16x8) -> i16x8 {
        x.max(min).min(max)
    }

    /// Same as `scalar::up_down_ramp`, but operates on a vector of 8 values in parallel
    #[inline]
    fn up_down_ramp_simd(x: i16x8, strength: i16) -> i16x8 {
        signum_simd(x)
            * (x.abs() - (2 * (x.abs() - i16x8::splat(strength))).max(i16x8::ZERO)).max(i16x8::ZERO)
    }

    /// Same as `scalar::clipd1`, but operates on a vector of 8 values in parallel
    #[inline]
    fn clipd1_simd(x: i16x8, lim: i16x8) -> i16x8 {
        let la = lim.abs();
        clamp_simd(x, -la, la)
    }

    /// Utility to upcast and convert a slice of 8 `u8` values into a `i16x8` vector.
    #[inline]
    fn into_simd16(a: &mut [u8]) -> i16x8 {
        debug_assert!(a.len() == 8); // might even help the optimizer in release mode...?
        i16x8::from([
            a[0] as i16,
            a[1] as i16,
            a[2] as i16,
            a[3] as i16,
            a[4] as i16,
            a[5] as i16,
            a[6] as i16,
            a[7] as i16,
        ])
    }

    /// Same as `scalar::process`, but performs it on 8 independent sets of values in parallel.
    /// All slice parameters must have a length of 8 - this not enforced by the type system due
    /// to usage in chunked iteration below, see: https://github.com/rust-lang/rust/issues/74985
    #[allow(non_snake_case)]
    #[inline]
    pub fn process_simd(A: &mut [u8], B: &mut [u8], C: &mut [u8], D: &mut [u8], strength: u8) {
        debug_assert!((1..=12).contains(&strength));

        let a16 = into_simd16(A);
        let b16 = into_simd16(B);
        let c16 = into_simd16(C);
        let d16 = into_simd16(D);

        let d: i16x8 = (a16 - 4 * b16 + 4 * c16 - d16).shr(3);
        let d1: i16x8 = up_down_ramp_simd(d, strength as i16);
        let d2: i16x8 = clipd1_simd((a16 - d16).shr(2), d1.shr(1));

        let res_a = a16 - d2;
        let res_b = clamp_simd(b16 + d1, i16x8::ZERO, i16x8::splat(255));
        let res_c = clamp_simd(c16 - d1, i16x8::ZERO, i16x8::splat(255));
        let res_d = d16 + d2;

        let res_a = res_a.as_array_ref();
        let res_b = res_b.as_array_ref();
        let res_c = res_c.as_array_ref();
        let res_d = res_d.as_array_ref();

        for i in 0..8 {
            A[i] = res_a[i] as u8;
            B[i] = res_b[i] as u8;
            C[i] = res_c[i] as u8;
            D[i] = res_d[i] as u8;
        }
    }
}

use itertools::izip;
use scalar_impl::process;
use simd_impl::process_simd;

/// Applies the deblocking with the given strength to the horizontal block edges.
#[allow(non_snake_case)]
fn deblock_horiz(result: &mut [u8], width: usize, strength: u8) {
    let height = result.len() / width;

    let mut edge_y = 8; // the vertical index of the row with the "C" samples
    while edge_y <= height - 2 {
        // breaking out four rows, one for each of the ABCD samples
        let (_, rest) = result.split_at_mut((edge_y - 2) * width);
        let (row_a, rest) = rest.split_at_mut(width);
        let (row_b, rest) = rest.split_at_mut(width);
        let (row_c, rest) = rest.split_at_mut(width);
        let (row_d, _) = rest.split_at_mut(width);

        // the first N*8 samples (horizontally) are handled by the SIMD implementation
        let row_a_chunks = row_a.chunks_exact_mut(8);
        let row_b_chunks = row_b.chunks_exact_mut(8);
        let row_c_chunks = row_c.chunks_exact_mut(8);
        let row_d_chunks = row_d.chunks_exact_mut(8);

        // luckily the memory layout is advantageous here, no need for transposing
        // chunks into the SIMD lanes
        for (((A, B), C), D) in row_a_chunks
            .zip(row_b_chunks)
            .zip(row_c_chunks)
            .zip(row_d_chunks)
        {
            process_simd(A, B, C, D, strength);
        }

        // the remaining <=7 columns are handled by the scalar implementation
        let row_a_rem = row_a.chunks_exact_mut(8).into_remainder();
        let row_b_rem = row_b.chunks_exact_mut(8).into_remainder();
        let row_c_rem = row_c.chunks_exact_mut(8).into_remainder();
        let row_d_rem = row_d.chunks_exact_mut(8).into_remainder();

        for (((A, B), C), D) in row_a_rem
            .iter_mut()
            .zip(row_b_rem)
            .zip(row_c_rem)
            .zip(row_d_rem)
        {
            process(A, B, C, D, strength);
        }

        edge_y += 8;
    }
}

/// Applies the deblocking with the given strength to the vertical block edges.
#[allow(non_snake_case)]
fn deblock_vert(result: &mut [u8], width: usize, strength: u8) {
    /// Holds a bundle of 8 mutable byte slice references.
    /// This is a tuple instead of an array due to `izip!` usage below.
    type ByteSliceMutRefOctet<'a> = (
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
        &'a mut [u8],
    );

    /// Indexes into each slice in a `ByteSliceMutRefOctet`, returning an array of the values.
    #[inline]
    fn extract_column(arrays: &ByteSliceMutRefOctet, i: usize) -> [u8; 8] {
        [
            arrays.0[i],
            arrays.1[i],
            arrays.2[i],
            arrays.3[i],
            arrays.4[i],
            arrays.5[i],
            arrays.6[i],
            arrays.7[i],
        ]
    }

    /// Sets a single value in each slice in a `ByteSliceMutRefOctet` to the corresponding value in `a`.
    #[inline]
    fn set_column(arrays: &mut ByteSliceMutRefOctet, i: usize, a: [u8; 8]) {
        arrays.0[i] = a[0];
        arrays.1[i] = a[1];
        arrays.2[i] = a[2];
        arrays.3[i] = a[3];
        arrays.4[i] = a[4];
        arrays.5[i] = a[5];
        arrays.6[i] = a[6];
        arrays.7[i] = a[7];
    }

    // So the `[2..]`s below don't panic, also, not enough pixels to process any vertical edges otherwise.
    if width >= 10 {
        // Handling the top N*8 rows with the SIMD implementation,
        // iterating on 8 (the SIMD width) rows worth of data at a time.
        for rows in result.chunks_exact_mut(width * 8) {
            // Splitting into separate rows (doing it this way to satisfy the borrow checker),
            // each row will supply one SIMD lane.
            let (row_0, rows) = rows.split_at_mut(width);
            let (row_1, rows) = rows.split_at_mut(width);
            let (row_2, rows) = rows.split_at_mut(width);
            let (row_3, rows) = rows.split_at_mut(width);
            let (row_4, rows) = rows.split_at_mut(width);
            let (row_5, rows) = rows.split_at_mut(width);
            let (row_6, rows) = rows.split_at_mut(width);
            let (row_7, rows) = rows.split_at_mut(width);
            debug_assert!(rows.is_empty()); // should have exactly consumed the 8 rows

            // In parallel over each of the 8 rows (this is the SIMD width),
            // iterating over 8-wide chunks (this is the spacing between processed edges),
            // such that the second half of each chunk is the ABCD sample quartet
            // (this offset is done by the `[2..]` part). In the first halves of these
            // chunks are the "middle columns" of the 8x8 blocks, not to be touched.
            // This was easier than iterating over 4-wide chunks and skipping every odd one.
            let parallel_iter = izip!(
                row_0[2..].chunks_exact_mut(8),
                row_1[2..].chunks_exact_mut(8),
                row_2[2..].chunks_exact_mut(8),
                row_3[2..].chunks_exact_mut(8),
                row_4[2..].chunks_exact_mut(8),
                row_5[2..].chunks_exact_mut(8),
                row_6[2..].chunks_exact_mut(8),
                row_7[2..].chunks_exact_mut(8)
            );

            // Transposing the (vertical) sample tuples into SIMD vectors, processing them,
            // then untransposing and storing.
            for mut arrays in parallel_iter {
                let mut As = extract_column(&arrays, 4);
                let mut Bs = extract_column(&arrays, 5);
                let mut Cs = extract_column(&arrays, 6);
                let mut Ds = extract_column(&arrays, 7);

                process_simd(&mut As, &mut Bs, &mut Cs, &mut Ds, strength);

                set_column(&mut arrays, 4, As);
                set_column(&mut arrays, 5, Bs);
                set_column(&mut arrays, 6, Cs);
                set_column(&mut arrays, 7, Ds);
            }
        }

        // The remaining <=7 rows at the bottom are handled by the scalar implementation,
        // with a similar iteration pattern as above, but with one row at a time, not in
        // parallel over an octet of rows.
        for row in result
            .chunks_exact_mut(width * 8)
            .into_remainder()
            .chunks_exact_mut(width)
        {
            for chunk in row[2..].chunks_exact_mut(8) {
                let mut A = chunk[4];
                let mut B = chunk[5];
                let mut C = chunk[6];
                let mut D = chunk[7];
                process(&mut A, &mut B, &mut C, &mut D, strength);
                chunk[4] = A;
                chunk[5] = B;
                chunk[6] = C;
                chunk[7] = D;
            }
        }
    }
}

/// Applies the deblocking filter to the horizontal and vertical block edges
/// of the given image data with the given strength, assuming 8x8 block size.
#[allow(non_snake_case)]
#[allow(clippy::identity_op)]
pub fn deblock(data: &[u8], width: usize, strength: u8) -> Vec<u8> {
    debug_assert!(data.len() % width == 0);

    let mut result = data.to_vec();

    // According to the spec, the horizontal deblocking filter is applied first.
    deblock_horiz(result.as_mut(), width, strength);
    deblock_vert(result.as_mut(), width, strength);

    result
}
