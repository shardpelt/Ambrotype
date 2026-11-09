// SPDX-License-Identifier: GPL-3.0-or-later
//! Running the GPU steps, with the CPU for layers that have no kernel.

use super::*;

impl OnGpu {
    pub(super) fn run(&self, inputs: &TVec<TValue>) -> TractResult<TVec<TValue>> {
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
