// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for running a model on the GPU against tract.

use super::*;

/// A GPU for tests: a software one (lavapipe) is enough.
fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| Gpu::new(true)).as_ref()
}

/// Small pseudo-random values.
fn values(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(7);
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s % 2000) as f32 / 1000.0 - 1.0
        })
        .collect()
}

/// The model run on the GPU and on the CPU: the largest difference of
/// the first output, against its largest value.
fn compare(typed: TypedModel, inputs: TVec<TValue>) -> Option<f32> {
    let gpu = gpu()?;
    let cpu = typed
        .clone()
        .into_optimized()
        .unwrap()
        .into_runnable()
        .unwrap();
    let want = cpu.run(inputs.clone()).unwrap();
    let net = Net::on(typed, gpu).expect("runs on the GPU");
    eprintln!(
        "{} layers on the GPU",
        net.gpu.as_ref().unwrap().steps.len()
    );
    let got = net.gpu.as_ref().unwrap().run(&inputs).unwrap();
    let (w, g) = (floats(&want[0]).unwrap(), floats(&got[0]).unwrap());
    assert_eq!(w.len(), g.len());
    let top = w.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
    Some(
        w.iter()
            .zip(&g)
            .fold(0f32, |m, (a, b)| m.max((a - b).abs()))
            / top,
    )
}

#[test]
fn a_small_network_matches_the_cpu() {
    // Conv (grouped, padded) → LeakyRelu → transpose → batched matmul
    // with its weights → softmax → reduce, built with tract's own ops.
    use tract_onnx::tract_core::ops::cnn::{PaddingSpec, PoolSpec};
    let mut m = TypedModel::default();
    let x = m.add_source("x", f32::fact([1, 4, 10, 12])).unwrap();
    let k = m
        .add_const(
            "k",
            Tensor::from_shape(&[8, 2, 3, 3], &values(144, 1)).unwrap(),
        )
        .unwrap();
    let bias = m
        .add_const("b", Tensor::from_shape(&[8], &values(8, 2)).unwrap())
        .unwrap();
    let pool = PoolSpec::new(
        DataFormat::NCHW,
        tvec![3, 3],
        PaddingSpec::Explicit(tvec![1, 1], tvec![1, 1]),
        None,
        None,
        4,
        8,
    );
    let conv = TConv::new(pool, KernelFormat::OIHW, 2, None);
    let y = m.wire_node("conv", conv, &[x, k, bias]).unwrap()[0];
    let y = m
        .wire_node("lrelu", ops::nn::leaky_relu(0.1), &[y])
        .unwrap()[0];
    // [1, 8, 10, 12] → [1, 8, 120] → [1, 120, 8]
    let y = m
        .wire_node(
            "flat",
            ops::change_axes::AxisOp::Reshape(
                2,
                tvec![10.to_dim(), 12.to_dim()],
                tvec![120.to_dim()],
            ),
            &[y],
        )
        .unwrap()[0];
    let y = m
        .wire_node("t", ops::change_axes::AxisOp::Move(1, 2), &[y])
        .unwrap()[0];
    let w = m
        .add_const("w", Tensor::from_shape(&[8, 5], &values(40, 3)).unwrap())
        .unwrap();
    let mm = ops::einsum::EinSum::new("bmk,kn->bmn".parse().unwrap(), DatumType::F32);
    let y = m.wire_node("mm", mm, &[y, w]).unwrap()[0];
    let sm = ops::nn::Softmax::new(tvec![2], None, ops::nn::SoftmaxKind::Softmax);
    let y = m.wire_node("sm", sm, &[y]).unwrap()[0];
    m.select_output_outlets(&[y]).unwrap();
    let input = Tensor::from_shape(&[1, 4, 10, 12], &values(480, 4)).unwrap();
    let Some(err) = compare(m.into_decluttered().unwrap(), tvec![input.into_tvalue()]) else {
        eprintln!("no GPU: skipped");
        return;
    };
    assert!(err < 1e-4, "{err}");
}

/// A real model on the GPU against the CPU: `AMB_NET=model.onnx
/// AMB_NET_SHAPE=1x3x512x512[,1x1x512x512] cargo test -p amb-ai --release
/// -- --ignored net` (inputs are random, 0-1; `AMB_NET_BYTES=1` for u8).
#[test]
#[ignore]
fn a_model_matches_the_cpu() {
    let (Some(path), Some(shapes)) = (
        std::env::var_os("AMB_NET"),
        std::env::var("AMB_NET_SHAPE").ok(),
    ) else {
        return;
    };
    let bytes = std::env::var("AMB_NET_BYTES").is_ok();
    let mut m = tract_onnx::onnx().model_for_path(path).unwrap();
    let mut inputs = TVec::new();
    for (i, s) in shapes.split(',').enumerate() {
        let dims: Vec<usize> = s.split('x').map(|d| d.parse().unwrap()).collect();
        let n: usize = dims.iter().product();
        let v: Vec<f32> = values(n, i as u32 + 9)
            .iter()
            .map(|x| x * 0.5 + 0.5)
            .collect();
        let t = if bytes {
            let b: Vec<u8> = v.iter().map(|x| (x * 255.0) as u8).collect();
            m = m.with_input_fact(i, u8::fact(&dims).into()).unwrap();
            Tensor::from_shape(&dims, &b).unwrap()
        } else {
            m = m.with_input_fact(i, f32::fact(&dims).into()).unwrap();
            Tensor::from_shape(&dims, &v).unwrap()
        };
        inputs.push(t.into_tvalue());
    }
    let typed = m.into_typed().unwrap().into_decluttered().unwrap();
    let t = std::time::Instant::now();
    let err = compare(typed, inputs).expect("a GPU");
    eprintln!("largest difference {err} (both runs {:?})", t.elapsed());
    assert!(err < 1e-2, "{err}");
}
