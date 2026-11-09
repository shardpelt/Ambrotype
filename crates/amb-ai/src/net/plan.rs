// SPDX-License-Identifier: GPL-3.0-or-later
//! Planning: turning a tract graph into GPU steps.

use super::*;

impl OnGpu {
    /// The plan for running `model` on `gpu`, or `None` if it should stay
    /// on the CPU (`any_size` keeps small ones too).
    pub(super) fn plan(model: &TypedModel, gpu: &'static Gpu, any_size: bool) -> Option<OnGpu> {
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
