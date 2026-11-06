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

impl OnGpu {
    /// The plan for running `model` on `gpu`, or `None` if it should stay
    /// on the CPU (`any_size` keeps small ones too).
    fn plan(model: &TypedModel, gpu: &'static Gpu, any_size: bool) -> Option<OnGpu> {
        let order = model.eval_order_opt_ram().ok()?;
        let mut steps = HashMap::new();
        let mut work = 0f64;
        let fits = |o: &OutletId| {
            shape_of(model, *o).is_some_and(|s| s.iter().product::<usize>() <= gpu.nn_max_floats())
        };
        for &id in &order {
            let node = model.node(id);
            if node.inputs.is_empty() {
                continue;
            }
            let heavy = node.op_is::<ops::einsum::EinSum>()
                || node.op_is::<TConv>()
                || node.op_is::<TDeconv>();
            let fit = fits(&OutletId::new(id, 0)) && node.inputs.iter().all(fits);
            match step(model, id).filter(|_| fit) {
                Some(s) => {
                    let out = shape_of(model, OutletId::new(id, 0))?;
                    work += match &s {
                        Step::MatMul(p) => p.mm.out_len as f64 * p.mm.k as f64,
                        Step::Conv(c, _, _) | Step::Deconv(c, _, _) => {
                            out.iter().product::<usize>() as f64
                                * (c.c / c.group.max(1) * c.kh * c.kw) as f64
                        }
                        _ => 0.0,
                    };
                    steps.insert(id, s);
                }
                None if heavy && fit => {
                    if std::env::var_os("AMB_NET_DEBUG").is_some() {
                        eprintln!("not on the GPU: {} {:?}", node.op.name(), node.op);
                    }
                    return None;
                }
                None => {
                    if std::env::var_os("AMB_NET_DEBUG").is_some() {
                        let dts: Vec<_> = node
                            .inputs
                            .iter()
                            .map(|i| model.outlet_fact(*i).map(|f| f.datum_type).ok())
                            .collect();
                        eprintln!("on the CPU: {} {dts:?} {:?}", node.op.name(), node.op);
                    }
                }
            }
        }
        if work < WORTH_IT && !any_size {
            return None;
        }
        let mut last_use = HashMap::new();
        for (at, &id) in order.iter().enumerate() {
            for i in &model.node(id).inputs {
                last_use.insert(*i, at);
            }
        }
        for o in model.output_outlets().ok()? {
            last_use.insert(*o, usize::MAX);
        }
        Some(OnGpu {
            model: model.clone(),
            order,
            steps,
            last_use,
            weights: Mutex::new(HashMap::new()),
            gpu,
            failed: Mutex::new(false),
        })
    }

    fn run(&self, inputs: &TVec<TValue>) -> TractResult<TVec<TValue>> {
        let model = &self.model;
        let mut s = self.gpu.nn();
        let mut values: HashMap<OutletId, Value> = HashMap::new();
        let sources = model.input_outlets()?;
        for (k, o) in sources.iter().enumerate() {
            let v = inputs
                .get(k)
                .ok_or_else(|| err(format!("input {k} missing")))?;
            values.insert(*o, Value::Cpu(v.clone()));
        }
        let mut weights = self.weights.lock().unwrap_or_else(|e| e.into_inner());
        let check = std::env::var_os("AMB_NET_CHECK").is_some();
        for (at, &id) in self.order.iter().enumerate() {
            let node = model.node(id);
            if node.inputs.is_empty() {
                if let Some(c) = node.op_as::<ops::konst::Const>() {
                    values.insert(
                        OutletId::new(id, 0),
                        Value::Cpu(c.val().clone().into_tvalue()),
                    );
                }
                continue;
            }
            let outs: Vec<Value> = match self.steps.get(&id) {
                Some(step) => {
                    let mut ins = Vec::new();
                    for (k, i) in node.inputs.iter().enumerate() {
                        // Indices and other inputs a kernel doesn't read.
                        if matches!(step, Step::Gather(..)) && k > 0
                            || matches!(step, Step::Conv(_, false, _) | Step::Deconv(_, _, false))
                                && k > 1
                        {
                            continue;
                        }
                        ins.push(gpu_value(&mut s, &mut values, &mut weights, model, *i)?);
                    }
                    let shape = shape_of(model, OutletId::new(id, 0))
                        .ok_or_else(|| err("unknown shape".into()))?;
                    let t = run_step(&mut s, step, &ins, &shape);
                    if check {
                        self.check(&mut s, &values, node, &t, step)?;
                    }
                    vec![Value::Gpu(t)]
                }
                None => {
                    let mut ins = TVec::new();
                    for i in &node.inputs {
                        ins.push(cpu_value(&mut s, &values, model, *i)?);
                    }
                    let out = node
                        .op
                        .eval_out_of_plan(ins)?
                        .ok_or_else(|| err(format!("{} can't run alone", node.op.name())))?;
                    out.into_iter().map(Value::Cpu).collect()
                }
            };
            for (k, v) in outs.into_iter().enumerate() {
                values.insert(OutletId::new(id, k), v);
            }
            // Free what nothing reads any more.
            for i in &node.inputs {
                if self.last_use.get(i).is_some_and(|&l| l == at) {
                    if let Some(Value::Gpu(t)) = values.remove(i) {
                        s.free(t);
                    }
                }
            }
        }
        let mut out = TVec::new();
        for o in model.output_outlets()? {
            out.push(cpu_value(&mut s, &values, model, *o)?);
        }
        Ok(out)
    }
}

impl OnGpu {
    /// Debugging (`AMB_NET_CHECK=1`): a layer's GPU output against tract's.
    fn check(
        &self,
        s: &mut Session,
        values: &HashMap<OutletId, Value>,
        node: &TypedNode,
        got: &nn::Tensor,
        step: &Step,
    ) -> TractResult<()> {
        let mut ins = TVec::new();
        for i in &node.inputs {
            ins.push(match values.get(i) {
                Some(_) => cpu_value(s, values, &self.model, *i)?,
                None => konst(&self.model, *i)
                    .ok_or_else(|| err("missing".into()))?
                    .into_tvalue(),
            });
        }
        let want = node
            .op
            .eval_out_of_plan(ins)?
            .ok_or_else(|| err("no eval".into()))?;
        let want = floats(&want[0]).unwrap_or_default();
        let got = s.download(got);
        let top = want.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
        let diff = want
            .iter()
            .zip(&got)
            .fold(0f32, |m, (a, b)| m.max((a - b).abs()))
            / top;
        if diff > 1e-3 || want.len() != got.len() {
            eprintln!(
                "{} ({}) differs by {diff}: {step:?}",
                node.name,
                node.op.name()
            );
        }
        Ok(())
    }
}

/// A value on the GPU: uploaded if it was on the CPU (weights once, kept).
fn gpu_value(
    s: &mut Session,
    values: &mut HashMap<OutletId, Value>,
    weights: &mut HashMap<OutletId, nn::Tensor>,
    model: &TypedModel,
    o: OutletId,
) -> TractResult<nn::Tensor> {
    if let Some(t) = weights.get(&o) {
        return Ok(t.clone());
    }
    match values.get(&o) {
        Some(Value::Gpu(t)) => Ok(t.clone()),
        Some(Value::Cpu(v)) => {
            let data = floats(v).ok_or_else(|| err("not numbers".into()))?;
            let data = &data[..];
            let shape = v.shape().to_vec();
            if model.node(o.node).op_is::<ops::konst::Const>() {
                let t = s.constant(data, &shape);
                weights.insert(o, t.clone());
                Ok(t)
            } else {
                let t = s.upload(data, &shape);
                values.insert(o, Value::Gpu(t.clone()));
                Ok(t)
            }
        }
        None => Err(err(format!("value {o:?} missing"))),
    }
}

fn cpu_value(
    s: &mut Session,
    values: &HashMap<OutletId, Value>,
    model: &TypedModel,
    o: OutletId,
) -> TractResult<TValue> {
    match values.get(&o) {
        Some(Value::Cpu(v)) => Ok(v.clone()),
        Some(Value::Gpu(t)) => {
            let data = s.download(t);
            let shape = shape_of(model, o).unwrap_or_else(|| t.shape.clone());
            Ok(Tensor::from_shape(&shape, &data)?.into_tvalue())
        }
        None => Err(err(format!("value {o:?} missing"))),
    }
}

fn run_step(s: &mut Session, step: &Step, ins: &[nn::Tensor], shape: &[usize]) -> nn::Tensor {
    match step {
        Step::Alias => ins[0].reshaped(shape),
        Step::Copy(view) => s.copy(&ins[0], view, shape),
        Step::Concat(axis) => {
            let out = s.empty(shape);
            let strides = nn::contiguous(shape);
            let mut at = 0;
            for t in ins {
                let view = View::strided(0, &nn::contiguous(&t.shape));
                s.copy_into(
                    &out,
                    at * strides[*axis] as usize,
                    &strides,
                    t,
                    &view,
                    &t.shape,
                );
                at += t.shape[*axis];
            }
            out
        }
        Step::Binary(op) => s.binary(*op, &ins[0], &ins[1], shape),
        Step::Unary(op) => s.unary(*op, &ins[0]),
        Step::MatMul(p) => {
            let mut a = ins[0].clone();
            let mut b = ins[1].clone();
            let mut made = Vec::new();
            if let Some((view, shape)) = &p.a {
                a = s.copy(&a, view, shape);
                made.push(a.clone());
            }
            if let Some((view, shape)) = &p.b {
                b = s.copy(&b, view, shape);
                made.push(b.clone());
            }
            let out = match &p.out {
                None => s.matmul(&a, &b, &p.mm, shape),
                Some((inner, view)) => {
                    let c = s.matmul(&a, &b, &p.mm, inner);
                    let out = s.copy(&c, view, shape);
                    s.free(c);
                    out
                }
            };
            drop((a, b));
            for t in made {
                s.free(t);
            }
            out
        }
        Step::Conv(c, bias, last) => {
            let x = channels_first(s, &ins[0], c, *last);
            let out = s.conv(&x, &ins[1], bias.then(|| &ins[2]), c);
            s.free(x);
            channels_back(s, out, c.o, c, *last, shape)
        }
        Step::Deconv(c, strides, bias) => {
            let x = ins[0].reshaped(&[c.n, c.c, c.h, c.w]);
            let out = s.deconv(&x, &ins[1], *strides, bias.then(|| &ins[2]), c);
            out.reshaped(shape)
        }
        Step::Pad(before, after, mode) => s.pad(&ins[0], before, after, *mode),
        Step::Reduce(axes) => {
            let mut t = ins[0].clone();
            for (k, (axis, op)) in axes.iter().enumerate() {
                let next = s.reduce(*op, &t, *axis);
                if k > 0 {
                    s.free(t);
                }
                t = next;
            }
            t.reshaped(shape)
        }
        Step::Softmax(axis, log) => s.softmax(&ins[0], *axis, *log),
        Step::RmsNorm(axis, eps) => s.rms_norm(&ins[0], *axis, *eps),
        Step::MaxPool(c, last) => {
            let x = channels_first(s, &ins[0], c, *last);
            let out = s.max_pool(&x, c);
            s.free(x);
            channels_back(s, out, c.c, c, *last, shape)
        }
        Step::Gather(axis, indices) => s.gather(&ins[0], *axis, indices, shape),
    }
}

/// A convolution's input as NCHW: a copy if its channels come last.
fn channels_first(s: &mut Session, x: &nn::Tensor, c: &nn::Conv, last: bool) -> nn::Tensor {
    if !last {
        return x.reshaped(&[c.n, c.c, c.h, c.w]);
    }
    let st = nn::contiguous(&[c.n, c.h, c.w, c.c]);
    s.copy(
        x,
        &View::strided(0, &[st[0], st[3], st[1], st[2]]),
        &[c.n, c.c, c.h, c.w],
    )
}

/// A convolution's NCHW output (`o` channels) in the layout asked for.
fn channels_back(
    s: &mut Session,
    out: nn::Tensor,
    o: usize,
    c: &nn::Conv,
    last: bool,
    shape: &[usize],
) -> nn::Tensor {
    if !last {
        return out.reshaped(shape);
    }
    let st = nn::contiguous(&[c.n, o, c.oh, c.ow]);
    let t = s.copy(
        &out,
        &View::strided(0, &[st[0], st[2], st[3], st[1]]),
        &[c.n, c.oh, c.ow, o],
    );
    s.free(out);
    t.reshaped(shape)
}

/// The GPU step for node `id`, if there is one.
fn step(model: &TypedModel, id: usize) -> Option<Step> {
    let node = model.node(id);
    let out = OutletId::new(id, 0);
    let shape = shape_of(model, out)?;
    if node.outputs.len() != 1 || !is_f32(model, out) || shape.len() > nn::RANK {
        return None;
    }
    let ins: Vec<Vec<usize>> = node
        .inputs
        .iter()
        .map(|&i| shape_of(model, i))
        .collect::<Option<_>>()?;
    if ins.iter().any(|s| s.len() > nn::RANK) {
        return None;
    }
    let all_f32 = node.inputs.iter().all(|&i| is_f32(model, i));
    let first_f32 = node.inputs.first().is_some_and(|&i| is_f32(model, i));
    let op = &node.op;

    if let Some(a) = op.downcast_ref::<ops::change_axes::AxisOp>() {
        if !first_f32 {
            return None;
        }
        return Some(match a {
            ops::change_axes::AxisOp::Move(from, to) => {
                let s = nn::contiguous(&ins[0]);
                let mut axes: Vec<usize> = (0..ins[0].len()).collect();
                let ax = axes.remove(*from);
                axes.insert(*to, ax);
                Step::Copy(View::strided(
                    0,
                    &axes.iter().map(|&i| s[i]).collect::<Vec<_>>(),
                ))
            }
            _ => Step::Alias,
        });
    }
    if let Some(c) = op.downcast_ref::<ops::cast::Cast>() {
        return (c.to == DatumType::F32 && first_f32).then_some(Step::Alias);
    }
    if !all_f32 && !op.is::<ops::array::Gather>() {
        return None;
    }
    if let Some(sl) = op.downcast_ref::<ops::array::Slice>() {
        let start = sl.start.to_i64().ok()? as usize;
        let s = nn::contiguous(&ins[0]);
        return Some(Step::Copy(View::strided(start * s[sl.axis] as usize, &s)));
    }
    if let Some(d) = op.downcast_ref::<ops::Downsample>() {
        let mut s = nn::contiguous(&ins[0]);
        let step = s[d.axis];
        // A negative stride reads from the end backwards.
        let first = if d.stride > 0 {
            d.modulo
        } else {
            ins[0][d.axis].checked_sub(1)?
        };
        s[d.axis] *= d.stride;
        return Some(Step::Copy(View::strided(first * step as usize, &s)));
    }
    if let Some(u) = op.downcast_ref::<ops::nn::NearestUpsample>() {
        if u.scales.len() != ins[0].len() {
            return None;
        }
        let mut v = View::strided(0, &nn::broadcast(&ins[0], &shape));
        v.div = u.scales.to_vec();
        return Some(Step::Copy(v));
    }
    if let Some(t) = op.downcast_ref::<ops::array::Tile>() {
        let _ = t;
        let mut v = View::strided(0, &nn::contiguous(&ins[0]));
        v.modulo = ins[0].clone();
        return Some(Step::Copy(v));
    }
    if op.is::<ops::array::MultiBroadcastTo>() {
        if ins[0].len() != shape.len() {
            return None;
        }
        return Some(Step::Copy(View::strided(
            0,
            &nn::broadcast(&ins[0], &shape),
        )));
    }
    if let Some(c) = op.downcast_ref::<ops::array::TypedConcat>() {
        return Some(Step::Concat(c.axis));
    }
    if let Some(b) = op.downcast_ref::<ops::binary::TypedBinOp>() {
        if b.1.is_some_and(|dt| dt != DatumType::F32)
            || ins[0].len() != shape.len()
            || ins[1].len() != shape.len()
        {
            return None;
        }
        let op = match b.0.name() {
            "Add" => Binary::Add,
            "Sub" => Binary::Sub,
            "SubF" => Binary::SubFlipped,
            "Mul" => Binary::Mul,
            "Div" => Binary::Div,
            "Max" => Binary::Max,
            "Min" => Binary::Min,
            "Pow" => Binary::Pow,
            _ => return None,
        };
        return Some(Step::Binary(op));
    }
    if let Some(e) = op.downcast_ref::<ops::element_wise::ElementWiseOp>() {
        if e.1.is_some_and(|dt| dt != DatumType::F32) {
            return None;
        }
        let mini = &e.0;
        let op = if let Some(l) = mini.downcast_ref::<ops::nn::LeakyRelu>() {
            Unary::LeakyRelu(l.alpha)
        } else if let Some(p) = mini.downcast_ref::<ops::math::PowConst>() {
            Unary::PowConst(p.exponent)
        } else {
            match mini.name().as_str() {
                "Neg" => Unary::Neg,
                "Sigmoid" => Unary::Sigmoid,
                "Exp" => Unary::Exp,
                "Ln" => Unary::Ln,
                "Sqrt" => Unary::Sqrt,
                "Rsqrt" => Unary::Rsqrt,
                "Recip" => Unary::Recip,
                "Abs" => Unary::Abs,
                "Tanh" => Unary::Tanh,
                "Erf" => Unary::Erf,
                "GeluExact" => Unary::GeluExact,
                "Square" => Unary::Square,
                "RoundHalfToEven" => Unary::Round,
                "Floor" => Unary::Floor,
                "Ceil" => Unary::Ceil,
                "Sin" => Unary::Sin,
                "Cos" => Unary::Cos,
                _ => return None,
            }
        };
        return Some(Step::Unary(op));
    }
    if let Some(p) = op.downcast_ref::<ops::array::Pad>() {
        let mode = match &p.mode {
            ops::array::PadMode::Constant(v) => PadMode::Constant(v.cast_to_scalar::<f32>().ok()?),
            ops::array::PadMode::Reflect => PadMode::Reflect,
            ops::array::PadMode::Edge => PadMode::Edge,
        };
        let (before, after) = p.pads.iter().copied().unzip();
        return Some(Step::Pad(before, after, mode));
    }
    if let Some(r) = op.downcast_ref::<ops::nn::Reduce>() {
        use ops::nn::Reducer;
        let mut axes = Vec::new();
        for (k, &axis) in r.axes.iter().enumerate() {
            let first = k == 0;
            axes.push((
                axis,
                match (&r.reducer, first) {
                    (Reducer::Sum, _) => nn::Reduce::Sum,
                    (Reducer::Max, _) => nn::Reduce::Max,
                    (Reducer::Min, _) => nn::Reduce::Min,
                    (Reducer::Prod, _) => nn::Reduce::Product,
                    (Reducer::MeanOfSquares, true) => nn::Reduce::MeanOfSquares,
                    (Reducer::MeanOfSquares, false) => nn::Reduce::Mean,
                    _ => return None,
                },
            ));
        }
        return Some(Step::Reduce(axes));
    }
    if let Some(sm) = op.downcast_ref::<ops::nn::Softmax>() {
        if sm.axes.len() != 1 || sm.quant_output_dt.is_some() {
            return None;
        }
        return Some(Step::Softmax(
            sm.axes[0],
            sm.kind == ops::nn::SoftmaxKind::LogSoftmax,
        ));
    }
    if let Some(r) = op.downcast_ref::<ops::nn::RmsNorm>() {
        return Some(Step::RmsNorm(r.axis, r.eps.cast_to_scalar::<f32>().ok()?));
    }
    if let Some(g) = op.downcast_ref::<ops::array::Gather>() {
        let indices = konst(model, node.inputs[1])?;
        if !first_f32 || g.output_type.is_some_and(|dt| dt != DatumType::F32) {
            return None;
        }
        let indices = indices.cast_to::<i64>().ok()?;
        let indices = indices
            .to_plain_array_view::<i64>()
            .ok()?
            .iter()
            .copied()
            .collect();
        return Some(Step::Gather(g.axis, indices));
    }
    if let Some(e) = op.downcast_ref::<ops::einsum::EinSum>() {
        if e.q_params.is_some() || e.operating_dt != DatumType::F32 || ins.len() != 2 {
            return None;
        }
        return matmul(e, &ins[0], &ins[1], &shape).map(|p| Step::MatMul(Box::new(p)));
    }
    if let Some(c) = op.downcast_ref::<TConv>() {
        if c.q_params.is_some() || c.kernel_fmt != KernelFormat::OIHW {
            return None;
        }
        let (spec, last) = conv_spec(&c.pool_spec, &ins[0], &shape, c.group, None)?;
        if ins[1] != [spec.o, spec.c / spec.group, spec.kh, spec.kw]
            && ins[1] != [spec.o, spec.c / spec.group, spec.kw]
        {
            return None;
        }
        let bias = bias(model, node.inputs.get(2).copied(), &ins, spec.o)?;
        return Some(Step::Conv(spec, bias, last));
    }
    if let Some(d) = op.downcast_ref::<TDeconv>() {
        if d.kernel_format != KernelFormat::OIHW {
            return None;
        }
        let (spec, last) = conv_spec(&d.pool_spec, &ins[0], &shape, d.group, Some(&d.adjustments))?;
        if last {
            return None;
        }
        let (og, cg) = (spec.o / spec.group, spec.c / spec.group);
        // OIHW here is the ONNX layout: [input channels, outputs per group, h, w].
        // Its kernel as [input channels, outputs per group, h, w], or (one
        // group) as [outputs, inputs, h, w].
        let k = &ins[1];
        if k.len() != 4 || k[2] != spec.kh || k[3] != spec.kw {
            return None;
        }
        let s = nn::contiguous(k);
        let strides = if k[0] == spec.c && k[1] == og {
            [s[0] as usize, s[1] as usize, s[2] as usize, s[3] as usize]
        } else if spec.group == 1 && k[0] == spec.o && k[1] == cg {
            [s[1] as usize, s[0] as usize, s[2] as usize, s[3] as usize]
        } else {
            return None;
        };
        let bias = bias(model, node.inputs.get(2).copied(), &ins, spec.o)?;
        return Some(Step::Deconv(spec, strides, bias));
    }
    if let Some(m) = op.downcast_ref::<MaxPool>() {
        if m.with_index_outputs.is_some() {
            return None;
        }
        let (mut spec, last) = conv_spec(&m.pool_spec, &ins[0], &shape, 1, None)?;
        spec.o = spec.c;
        return Some(Step::MaxPool(spec, last));
    }
    None
}

/// Whether a convolution's bias input is used: `Some(false)` for none or
/// zeros, `None` for one the kernels can't take.
fn bias(model: &TypedModel, input: Option<OutletId>, ins: &[Vec<usize>], o: usize) -> Option<bool> {
    let Some(i) = input else { return Some(false) };
    if let Some(k) = konst(model, i) {
        if floats(&k)?.iter().all(|v| *v == 0.0) {
            return Some(false);
        }
    }
    let n: usize = ins[2].iter().product();
    (n == o).then_some(true)
}

/// A convolution's (or pooling's) sizes from tract's description, and
/// whether its channels come last: NCHW, CHW, NHWC or HWC input, one or two
/// spatial axes.
fn conv_spec(
    pool: &ops::cnn::PoolSpec,
    input: &[usize],
    output: &[usize],
    group: usize,
    deconv: Option<&[usize]>,
) -> Option<(nn::Conv, bool)> {
    let (batch, last) = match pool.data_format {
        DataFormat::NCHW => (true, false),
        DataFormat::CHW => (false, false),
        DataFormat::NHWC => (true, true),
        DataFormat::HWC => (false, true),
    };
    let spatial = pool.rank();
    if spatial == 0 || spatial > 2 || input.len() != spatial + 1 + batch as usize {
        return None;
    }
    let first = batch as usize + !last as usize;
    let at = |s: &[usize], i: usize| s[first + i];
    let n = if batch { input[0] } else { 1 };
    let ch = if last {
        input.len() - 1
    } else {
        batch as usize
    };
    let c = input[ch];
    let o = output[ch];
    let hw_in: Vec<usize> = (0..spatial).map(|i| at(input, i)).collect();
    let hw_out: Vec<usize> = (0..spatial).map(|i| at(output, i)).collect();
    let pads = if let Some(adjustments) = deconv {
        pool.padding
            .compute_for_deconv(
                &hw_in,
                &pool.kernel_shape,
                &pool.dilations(),
                &pool.strides(),
                adjustments,
            )
            .ok()?
    } else {
        pool.computed_padding(&hw_in)
    };
    let two = |v: &[usize], d: usize| {
        if spatial == 2 {
            [v[0], v[1]]
        } else {
            [d, v[0]]
        }
    };
    let before: Vec<usize> = pads.iter().map(|p| p.pad_before).collect();
    let (h, w) = (two(&hw_in, 1)[0], two(&hw_in, 1)[1]);
    let (oh, ow) = (two(&hw_out, 1)[0], two(&hw_out, 1)[1]);
    let k = two(&pool.kernel_shape, 1);
    Some((
        nn::Conv {
            n,
            c,
            h,
            w,
            o,
            oh,
            ow,
            kh: k[0],
            kw: k[1],
            stride: two(&pool.strides(), 1),
            pad: two(&before, 0),
            dilation: two(&pool.dilations(), 1),
            group: group.max(1),
        },
        last,
    ))
}

/// One axis of an einsum: its size and place in each operand.
#[derive(Clone, Copy, Debug)]
struct Ax {
    size: usize,
    a: Option<usize>,
    b: Option<usize>,
    c: Option<usize>,
}

/// An einsum as a matrix product: axes in A, B and the output are batch
/// axes, in A and the output rows (m), in B and the output columns (n), in
/// A and B the shared axis (k).
fn matmul(e: &ops::einsum::EinSum, sa: &[usize], sb: &[usize], sc: &[usize]) -> Option<MatMulPlan> {
    let one = |v: &[usize]| -> Option<Option<usize>> {
        match v.len() {
            0 => Some(None),
            1 => Some(Some(v[0])),
            _ => None,
        }
    };
    let mut batch = Vec::new();
    let (mut m, mut n, mut k) = (Vec::new(), Vec::new(), Vec::new());
    for axis in e.axes.iter_all_axes() {
        let a = one(&axis.inputs[0])?;
        let b = one(&axis.inputs[1])?;
        let c = one(&axis.outputs[0])?;
        let size = [a.map(|p| sa[p]), b.map(|p| sb[p]), c.map(|p| sc[p])]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(1);
        let ax = Ax { size, a, b, c };
        if size == 1 {
            continue;
        }
        match (a.is_some(), b.is_some(), c.is_some()) {
            (true, true, true) => batch.push(ax),
            (true, false, true) => m.push(ax),
            (false, true, true) => n.push(ax),
            (true, true, false) => k.push(ax),
            _ => return None,
        }
    }
    // In the output's order (the shared axis in A's).
    let by = |v: &mut Vec<Ax>, f: fn(&Ax) -> Option<usize>| v.sort_by_key(f);
    by(&mut batch, |x| x.c);
    by(&mut m, |x| x.c);
    by(&mut n, |x| x.c);
    by(&mut k, |x| x.a);
    fn strides(shape: &[usize], pos: fn(&Ax) -> Option<usize>) -> impl Fn(&Ax) -> isize + '_ {
        let s = nn::contiguous(shape);
        move |x: &Ax| -> isize { pos(x).map_or(0, |p| if shape[p] == 1 { 0 } else { s[p] }) }
    }
    let (st_a, st_b, st_c) = (
        strides(sa, |x| x.a),
        strides(sb, |x| x.b),
        strides(sc, |x| x.c),
    );
    // Axes merged into one when one stride steps over the others.
    let merged = |axes: &[Ax], st: &dyn Fn(&Ax) -> isize| -> Option<isize> {
        for w in axes.windows(2) {
            if st(&w[0]) != st(&w[1]) * w[1].size as isize {
                return None;
            }
        }
        Some(axes.last().map_or(0, st))
    };
    let size = |axes: &[Ax]| axes.iter().map(|x| x.size).product::<usize>();
    // An operand's copy in the order (batch, rows, cols), contiguous.
    let canonical = |groups: [&[Ax]; 3], st: &dyn Fn(&Ax) -> isize, has: fn(&Ax) -> bool| {
        let axes: Vec<Ax> = groups.concat();
        let shape: Vec<usize> = axes
            .iter()
            .map(|x| if has(x) { x.size } else { 1 })
            .collect();
        let view = View::strided(
            0,
            &axes
                .iter()
                .map(|x| if has(x) { st(x) } else { 0 })
                .collect::<Vec<_>>(),
        );
        (view, shape)
    };
    // Batch axes: merged where all three allow, at most three left.
    let mut groups: Vec<Vec<Ax>> = Vec::new();
    for x in &batch {
        if let Some(g) = groups.last_mut() {
            let mut t = g.clone();
            t.push(*x);
            if merged(&t, &st_a).is_some()
                && merged(&t, &st_b).is_some()
                && merged(&t, &st_c).is_some()
            {
                *g = t;
                continue;
            }
        }
        groups.push(vec![*x]);
    }
    if groups.len() > 3 || batch.len() > nn::RANK {
        return None;
    }
    let (ma, ka) = (merged(&m, &st_a), merged(&k, &st_a));
    let (kb, nb) = (merged(&k, &st_b), merged(&n, &st_b));
    let (mc, nc) = (merged(&m, &st_c), merged(&n, &st_c));
    let mut plan_a = None;
    let mut plan_b = None;
    let mut out = None;
    // Batch strides for a contiguous (batch, rows, cols) copy.
    let packed = |rows: usize, cols: usize, has: &dyn Fn(&Ax) -> bool| -> [isize; 3] {
        let mut s = [0isize; 3];
        let mut step = (rows * cols) as isize;
        let at = 3 - groups.len();
        for (i, g) in groups.iter().enumerate().rev() {
            let n: usize = g.iter().map(|x| if has(x) { x.size } else { 1 }).product();
            s[at + i] = if n == 1 { 0 } else { step };
            step *= n as isize;
        }
        s
    };
    let gstride = |st: &dyn Fn(&Ax) -> isize| -> [isize; 3] {
        let mut s = [0isize; 3];
        let at = 3 - groups.len();
        for (i, g) in groups.iter().enumerate() {
            s[at + i] = merged(g, st).unwrap_or(0);
        }
        s
    };
    let (mm_, nn_, kk) = (size(&m), size(&n), size(&k));
    let a = match (ma, ka) {
        (Some(r), Some(c)) => Operand {
            offset: 0,
            rows: r,
            cols: c,
            batch: gstride(&st_a),
        },
        _ => {
            plan_a = Some(canonical([&batch, &m, &k], &st_a, |x| x.a.is_some()));
            Operand {
                offset: 0,
                rows: kk as isize,
                cols: 1,
                batch: packed(mm_, kk, &|x: &Ax| x.a.is_some()),
            }
        }
    };
    let b = match (kb, nb) {
        (Some(r), Some(c)) => Operand {
            offset: 0,
            rows: r,
            cols: c,
            batch: gstride(&st_b),
        },
        _ => {
            plan_b = Some(canonical([&batch, &k, &n], &st_b, |x| x.b.is_some()));
            Operand {
                offset: 0,
                rows: nn_ as isize,
                cols: 1,
                batch: packed(kk, nn_, &|x: &Ax| x.b.is_some()),
            }
        }
    };
    let c = match (mc, nc) {
        (Some(r), Some(c)) => Operand {
            offset: 0,
            rows: r,
            cols: c,
            batch: gstride(&st_c),
        },
        _ => {
            // The product contiguous as (batch, m, n), then read in the
            // output's order.
            let axes: Vec<Ax> = [&batch[..], &m, &n].concat();
            let inner: Vec<usize> = axes.iter().map(|x| x.size).collect();
            let s = nn::contiguous(&inner);
            let mut st = vec![0isize; sc.len()];
            for (i, x) in axes.iter().enumerate() {
                st[x.c?] = s[i];
            }
            out = Some((inner, View::strided(0, &st)));
            Operand {
                offset: 0,
                rows: nn_ as isize,
                cols: 1,
                batch: packed(mm_, nn_, &|_| true),
            }
        }
    };
    let mut bshape = [1usize; 3];
    let at = 3 - groups.len();
    for (i, g) in groups.iter().enumerate() {
        bshape[at + i] = size(g);
    }
    Some(MatMulPlan {
        a: plan_a,
        b: plan_b,
        mm: nn::MatMul {
            m: mm_,
            n: nn_,
            k: kk,
            batch: bshape,
            a,
            b,
            c,
            out_len: sc.iter().product(),
        },
        out,
    })
}

#[cfg(test)]
mod tests {
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
}
