use super::ops::{Binary, Conv, MatMul, Operand, Pad, Reduce, Unary, View};
use crate::device::tests::gpu;

/// Small pseudo-random values.
fn values(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s % 2000) as f32 / 1000.0 - 1.0
        })
        .collect()
}

fn close(a: &[f32], b: &[f32], tol: f32) {
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!((x - y).abs() <= tol * (1.0 + y.abs()), "at {i}: {x} vs {y}");
    }
}

#[test]
fn matmul_matches_the_cpu_with_odd_sizes_and_a_transpose() {
    let Some(gpu) = gpu() else { return };
    let (m, n, k) = (70, 33, 45);
    let a = values(m * k, 1);
    // B stored transposed ([n, k]), read through strides.
    let bt = values(n * k, 2);
    let mut s = gpu.nn();
    let (ta, tb) = (s.upload(&a, &[m, k]), s.upload(&bt, &[n, k]));
    let mm = MatMul {
        m,
        n,
        k,
        batch: [1, 1, 1],
        a: Operand {
            offset: 0,
            rows: k as isize,
            cols: 1,
            batch: [0; 3],
        },
        b: Operand {
            offset: 0,
            rows: 1,
            cols: k as isize,
            batch: [0; 3],
        },
        c: Operand {
            offset: 0,
            rows: n as isize,
            cols: 1,
            batch: [0; 3],
        },
        out_len: m * n,
    };
    let c = s.matmul(&ta, &tb, &mm, &[m, n]);
    let got = s.download(&c);
    let mut want = vec![0.0; m * n];
    for i in 0..m {
        for j in 0..n {
            want[i * n + j] = (0..k).map(|q| a[i * k + q] * bt[j * k + q]).sum();
        }
    }
    close(&got, &want, 1e-4);
}

#[test]
fn conv_matches_the_cpu_both_ways() {
    let Some(gpu) = gpu() else { return };
    // Grouped, strided, dilated and padded; then depthwise.
    for (c_in, o, group) in [(8usize, 32usize, 2usize), (6, 6, 6)] {
        let conv = Conv {
            n: 2,
            c: c_in,
            h: 9,
            w: 11,
            o,
            oh: 5,
            ow: 5,
            kh: 3,
            kw: 3,
            stride: [2, 2],
            pad: [1, 0],
            dilation: [1, 2],
            group,
        };
        let x = values(conv.n * c_in * 9 * 11, 3);
        let cg = c_in / group;
        let kernel = values(o * cg * 9, 4);
        let bias = values(o, 5);
        let mut s = gpu.nn();
        let tx = s.upload(&x, &[2, c_in, 9, 11]);
        let tk = s.upload(&kernel, &[o, cg, 3, 3]);
        let tbias = s.upload(&bias, &[o]);
        let out = s.conv(&tx, &tk, Some(&tbias), &conv);
        let got = s.download(&out);
        let mut want = vec![0.0; 2 * o * 25];
        for n in 0..2 {
            for oc in 0..o {
                let g = oc / (o / group);
                for oy in 0..5 {
                    for ox in 0..5 {
                        let mut sum = bias[oc];
                        for ci in 0..cg {
                            for ky in 0..3 {
                                for kx in 0..3 {
                                    let iy = (oy * 2 + ky) as isize - 1;
                                    let ix = (ox * 2 + kx * 2) as isize;
                                    if !(0..9).contains(&iy) || ix >= 11 {
                                        continue;
                                    }
                                    let ch = g * cg + ci;
                                    sum += x
                                        [((n * c_in + ch) * 9 + iy as usize) * 11 + ix as usize]
                                        * kernel[((oc * cg + ci) * 3 + ky) * 3 + kx];
                                }
                            }
                        }
                        want[((n * o + oc) * 5 + oy) * 5 + ox] = sum;
                    }
                }
            }
        }
        close(&got, &want, 1e-4);
    }
}

#[test]
fn copies_broadcasts_pads_and_reductions() {
    let Some(gpu) = gpu() else { return };
    let mut s = gpu.nn();
    let a: Vec<f32> = (0..6).map(|v| v as f32).collect();
    let t = s.upload(&a, &[2, 3]);
    // A transpose.
    let tr = s.copy(&t, &View::strided(0, &[1, 3]), &[3, 2]);
    assert_eq!(s.download(&tr), [0.0, 3.0, 1.0, 4.0, 2.0, 5.0]);
    // A broadcast add of a row.
    let row = s.upload(&[10.0, 20.0, 30.0], &[1, 3]);
    let sum = s.binary(Binary::Add, &t, &row, &[2, 3]);
    assert_eq!(s.download(&sum), [10.0, 21.0, 32.0, 13.0, 24.0, 35.0]);
    // Reflect padding of a row.
    let p = s.pad(&t, &[0, 2], &[0, 1], Pad::Reflect);
    assert_eq!(&s.download(&p)[..6], &[2.0, 1.0, 0.0, 1.0, 2.0, 1.0]);
    // Sums along each axis, softmax and a nearest upsample.
    let r0 = s.reduce(Reduce::Sum, &t, 0);
    assert_eq!(s.download(&r0), [3.0, 5.0, 7.0]);
    let r1 = s.reduce(Reduce::Max, &t, 1);
    assert_eq!(s.download(&r1), [2.0, 5.0]);
    let sm = s.softmax(&t, 1, false);
    let got = s.download(&sm);
    assert!((got[..3].iter().sum::<f32>() - 1.0).abs() < 1e-5 && got[2] > got[1]);
    let mut up = View::strided(0, &[3, 1]);
    up.div = vec![2, 2];
    let u = s.copy(&t, &up, &[4, 6]);
    assert_eq!(&s.download(&u)[..6], &[0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    let e = s.unary(Unary::GeluExact, &t);
    assert!((s.download(&e)[1] - 0.841_344_7).abs() < 1e-5);
}
