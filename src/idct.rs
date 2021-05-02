//! The fastest (and correctly implemented IDCT) I could find on the internet
//!
//! ![IDCT on each pixel](https://i.gyazo.com/5e8fb7c26af9867b7230511cc2813fbf.png)
//!
//! Borrowed it from [here](https://www.nayuki.io/page/fast-discrete-cosine-transform-algorithms)
//!
/*
 * Fast discrete cosine transform algorithms (Rust)
 *
 * Copyright (c) 2020 Project Nayuki. (MIT License)
 * https://www.nayuki.io/page/fast-discrete-cosine-transform-algorithms
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 * - The above copyright notice and this permission notice shall be included in
 *   all copies or substantial portions of the Software.
 * - The Software is provided "as is", without warranty of any kind, express or
 *   implied, including but not limited to the warranties of merchantability,
 *   fitness for a particular purpose and non infringement. In no event shall the
 *   authors or copyright holders be liable for any claim, damages or other
 *   liability, whether in an action of contract, tort or otherwise, arising from,
 *   out of or in connection with the Software or the use or other dealings in the
 *   Software.
 */
#![allow(clippy::excessive_precision, clippy::unreadable_literal)]
use ndarray::{Array2, ArrayViewMut1};
/*---- Tables of constants ----*/

const S: [f64; 8] = [
    0.353553390593273762200422,
    0.254897789552079584470970,
    0.270598050073098492199862,
    0.300672443467522640271861,
    0.353553390593273762200422,
    0.449988111568207852319255,
    0.653281482438188263928322,
    1.281457723870753089398043,
];

const A: [f64; 6] = [
    // Honestly idk why 0.0 is here
    0.0,
    0.707106781186547524400844,
    0.541196100146196984399723,
    0.707106781186547524400844,
    1.306562964876376527856643,
    0.382683432365089771728460,
];
/// Compute the 2 dimensional IDCT II(Or DCT III) on an
/// 8 by 8 array of f32's. This performs the conversion
/// in place instead of referencing and returning
///
/// It runs in O(N log N ) time
/// # Note
/// The time taken here varies depending on CPU architecture
/// On `x86/x86_64` platforms  with avx/sse CPU instructions it runs faster than on platforms lacking such instructions
///
/// # Panics
/// If the array is not 8 by 8
pub fn idct(array: &mut Array2<f64>) {
    // 2 Dimension IDCT-II can be classified

    // as applying DCT-III (or DCT-II) on the rows

    // and then applying it on the columns

    // apply on rows
    for mut i in array.rows_mut() {
        inverse_transform(&mut i);
    }
    // apply on columns
    for mut i in array.columns_mut() {
        inverse_transform(&mut i)
    }
}
/// Computes the scaled DCT type III on the given length-8 array in place.
///The inverse of this function is transform(), except for rounding errors.
fn inverse_transform(vector: &mut ArrayViewMut1<f64>) {
    assert_eq!(vector.len(), 8, "Inverse DCT works only on 8 by 8 vectors");

    // A straightforward inverse of the forward algorithm
    let v15 = vector[0] / S[0];
    let v26 = vector[1] / S[1];
    let v21 = vector[2] / S[2];
    let v28 = vector[3] / S[3];
    let v16 = vector[4] / S[4];
    let v25 = vector[5] / S[5];
    let v22 = vector[6] / S[6];
    let v27 = vector[7] / S[7];

    let v19 = (v25 - v28) / 2.0;
    let v20 = (v26 - v27) / 2.0;
    let v23 = (v26 + v27) / 2.0;
    let v24 = (v25 + v28) / 2.0;

    let v7 = (v23 + v24) / 2.0;
    let v11 = (v21 + v22) / 2.0;
    let v13 = (v23 - v24) / 2.0;
    let v17 = (v21 - v22) / 2.0;

    let v8 = (v15 + v16) / 2.0;
    let v9 = (v15 - v16) / 2.0;

    let v18 = (v19 - v20) * A[5]; // Different from original
    let v12 = (v19 * A[4] - v18) / -1.0;
    let v14 = (v18 - v20 * A[2]) / -1.0;

    let v6 = v14 - v7;
    let v5 = v13 / A[3] - v6;
    let v4 = -v5 - v12;
    let v10 = v17 / A[1] - v11;

    let v0 = (v8 + v11) / 2.0;
    let v1 = (v9 + v10) / 2.0;
    let v2 = (v9 - v10) / 2.0;
    let v3 = (v8 - v11) / 2.0;

    vector[0] = (v0 + v7) / 2.0;
    vector[1] = (v1 + v6) / 2.0;
    vector[2] = (v2 + v5) / 2.0;
    vector[3] = (v3 + v4) / 2.0;
    vector[4] = (v3 - v4) / 2.0;
    vector[5] = (v2 - v5) / 2.0;
    vector[6] = (v1 - v6) / 2.0;
    vector[7] = (v0 - v7) / 2.0;
}
