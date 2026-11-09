// SPDX-License-Identifier: GPL-3.0-or-later
//! Running a model: on the GPU when there is one (our own kernels,
//! `amb_gpu::nn`), else with tract on the CPU.
//!
//! On the GPU the model is kept in tract's simplified ("decluttered") form
//! and walked layer by layer. Each layer we have a kernel for runs there;
//! any other (small ones: shapes, integer maths) runs on the CPU with
//! tract's own code, its inputs read back and its outputs sent again. The
//! weights stay on the GPU between runs. A model is put on the GPU only if
//! all its convolutions and matrix products can run there, and only if it
//! is big enough to gain; when a run fails there, it runs on the CPU, now
//! and from then on. `AMB_AI_GPU=off` keeps every model on the CPU.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use amb_gpu::Gpu;
use amb_gpu::nn::{self, Binary, Operand, Pad as PadMode, Session, Unary, View};
use tract_onnx::prelude::*;
use tract_onnx::tract_core::ops;
use tract_onnx::tract_core::ops::cnn::{Conv as TConv, Deconv as TDeconv, KernelFormat, MaxPool};
use tract_onnx::tract_core::ops::nn::DataFormat;

/// Models with less work than this (multiply-adds) stay on the CPU: sending
/// work to the GPU costs more than it saves.
const WORTH_IT: f64 = 2e8;

/// The last failure of a model on the GPU.
static FAILED: Mutex<Option<String>> = Mutex::new(None);

fn gpu_off() -> bool {
    std::env::var("AMB_AI_GPU").is_ok_and(|v| v == "off" || v == "0")
}

/// Where models run, as a phrase: "on the GPU (name)" or "on the CPU (why)".
/// Small models stay on the CPU either way.
pub fn runs_on() -> String {
    if gpu_off() {
        return "on the CPU (AMB_AI_GPU=off)".into();
    }
    match amb_gpu::full::shared() {
        None => "on the CPU (no GPU found)".into(),
        Some(gpu) => match FAILED.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(e) => format!("on the CPU (the GPU failed: {e})"),
            None => format!("on the GPU ({})", gpu.name()),
        },
    }
}

/// A loaded model, ready to run.
pub struct Net {
    gpu: Option<OnGpu>,
    /// The CPU version, made when first needed (on a GPU, only if a run
    /// fails there).
    cpu: OnceLock<Result<Arc<TypedRunnableModel>, String>>,
    /// The simplified model, kept to make the CPU version from.
    typed: Mutex<Option<TypedModel>>,
}

/// Load a model with its inputs' sizes set.
pub fn build(model: InferenceModel) -> TractResult<Arc<Net>> {
    let typed = model.into_typed()?.into_decluttered()?;
    let gpu = if gpu_off() {
        None
    } else {
        amb_gpu::full::shared().and_then(|gpu| OnGpu::plan(&typed, gpu, false))
    };
    let net = Net {
        gpu,
        cpu: OnceLock::new(),
        typed: Mutex::new(Some(typed)),
    };
    if net.gpu.is_none() {
        net.cpu()?;
    }
    Ok(Arc::new(net))
}

impl Net {
    /// On a GPU, `typed` is simplified and kept as is; tests use it to run
    /// a model on a software GPU.
    #[cfg(test)]
    fn on(typed: TypedModel, gpu: &'static Gpu) -> Option<Net> {
        let on = OnGpu::plan(&typed, gpu, true)?;
        Some(Net {
            gpu: Some(on),
            cpu: OnceLock::new(),
            typed: Mutex::new(Some(typed)),
        })
    }

    fn cpu(&self) -> TractResult<&Arc<TypedRunnableModel>> {
        self.cpu
            .get_or_init(|| {
                let typed = self.typed.lock().unwrap_or_else(|e| e.into_inner()).take();
                typed
                    .ok_or_else(|| "the model is gone".to_string())
                    .and_then(|t| {
                        t.into_optimized()
                            .and_then(|m| m.into_runnable())
                            .map_err(|e| e.to_string())
                    })
            })
            .as_ref()
            .map_err(|e| err(e.to_string()))
    }

    pub fn run(&self, inputs: TVec<TValue>) -> TractResult<TVec<TValue>> {
        if let Some(g) = &self.gpu {
            let failed = *g.failed.lock().unwrap_or_else(|e| e.into_inner());
            if !failed {
                // A GPU error panics in wgpu: caught, it is a failed run.
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.run(&inputs)))
                    .unwrap_or_else(|_| Err(err("the GPU stopped".into())));
                match run {
                    Ok(out) => return Ok(out),
                    Err(e) => {
                        eprintln!("AI model on the GPU failed, so on the CPU from now: {e}");
                        *FAILED.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                        *g.failed.lock().unwrap_or_else(|e| e.into_inner()) = true;
                    }
                }
            }
        }
        self.cpu()?.run(inputs)
    }
}

/// What one layer does on the GPU.
#[derive(Clone, Debug)]
enum Step {
    /// The input as it is, seen with the output's shape.
    Alias,
    Copy(View),
    Concat(usize),
    Binary(Binary),
    Unary(Unary),
    MatMul(Box<MatMulPlan>),
    /// The bias used; channels last (NHWC).
    Conv(nn::Conv, bool, bool),
    Deconv(nn::Conv, [usize; 4], bool),
    Pad(Vec<usize>, Vec<usize>, PadMode),
    /// Along these axes, in turn.
    Reduce(Vec<(usize, nn::Reduce)>),
    Softmax(usize, bool),
    RmsNorm(usize, f32),
    MaxPool(nn::Conv, bool),
    Gather(usize, Vec<i64>),
}

/// A matrix product, with the copies that put its operands in order first
/// (and its output after) where their axes don't line up.
#[derive(Clone, Debug)]
struct MatMulPlan {
    a: Option<(View, Vec<usize>)>,
    b: Option<(View, Vec<usize>)>,
    mm: nn::MatMul,
    /// The product's own shape, and how the output reads it, when it is
    /// not already in the output's order.
    out: Option<(Vec<usize>, View)>,
}

struct OnGpu {
    model: TypedModel,
    order: Vec<usize>,
    steps: HashMap<usize, Step>,
    /// For each value, the place in `order` of the last layer reading it.
    last_use: HashMap<OutletId, usize>,
    weights: Mutex<HashMap<OutletId, nn::Tensor>>,
    gpu: &'static Gpu,
    failed: Mutex<bool>,
}

enum Value {
    Gpu(nn::Tensor),
    Cpu(TValue),
}

fn err(s: String) -> TractError {
    TractError::msg(s)
}

/// A tensor's values as f32 (cast if need be).
fn floats(t: &Tensor) -> Option<Vec<f32>> {
    let t = t.cast_to::<f32>().ok()?;
    let v = t.to_plain_array_view::<f32>().ok()?;
    Some(v.iter().copied().collect())
}

fn shape_of(model: &TypedModel, o: OutletId) -> Option<Vec<usize>> {
    model
        .outlet_fact(o)
        .ok()?
        .shape
        .as_concrete()
        .map(|s| s.to_vec())
}

fn is_f32(model: &TypedModel, o: OutletId) -> bool {
    model
        .outlet_fact(o)
        .is_ok_and(|f| f.datum_type == DatumType::F32)
}

fn konst(model: &TypedModel, o: OutletId) -> Option<Arc<Tensor>> {
    model.outlet_fact(o).ok()?.konst.clone()
}

mod plan;
mod run;
#[cfg(test)]
mod tests;
