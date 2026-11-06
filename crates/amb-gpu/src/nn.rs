// SPDX-License-Identifier: GPL-3.0-or-later
//! Running AI models (neural networks) on the GPU: tensors of f32 and the
//! layers the models we offer are made of (`shaders/nn.wgsl`). Which layer
//! runs when is up to the caller (`amb-ai` walks a model's graph); this
//! module only knows tensors and kernels, not model files.
//!
//! A [`Session`] records work into one command list and sends it to the GPU
//! now and then, and when a result is read back. Buffers freed in it are
//! kept for reuse until it ends.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::Gpu;

/// The most axes a tensor may have here.
pub const RANK: usize = 6;

/// A contiguous f32 tensor on the GPU (row-major: the last axis is
/// contiguous). Clones share the buffer, as a reshape does.
#[derive(Clone)]
pub struct Tensor {
    buf: Arc<wgpu::Buffer>,
    pub shape: Vec<usize>,
}

impl Tensor {
    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The same values seen with another shape of the same size.
    pub fn reshaped(&self, shape: &[usize]) -> Tensor {
        assert_eq!(shape.iter().product::<usize>(), self.len());
        Tensor {
            buf: self.buf.clone(),
            shape: shape.to_vec(),
        }
    }
}

/// Element-wise operations of two tensors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
    Add = 0,
    Sub = 1,
    Mul = 2,
    Div = 3,
    Max = 4,
    Min = 5,
    Pow = 6,
    /// `b - a`.
    SubFlipped = 7,
}

/// Element-wise operations of one tensor; some take a parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unary {
    Neg,
    Sigmoid,
    Exp,
    Ln,
    Sqrt,
    Rsqrt,
    Recip,
    Abs,
    Tanh,
    Erf,
    GeluExact,
    LeakyRelu(f32),
    Square,
    /// Rounding halves to even.
    Round,
    PowConst(f32),
    Floor,
    Ceil,
    Sin,
    Cos,
}

impl Unary {
    fn code(self) -> (i32, f32) {
        use Unary::*;
        match self {
            Neg => (0, 0.0),
            Sigmoid => (1, 0.0),
            Exp => (2, 0.0),
            Ln => (3, 0.0),
            Sqrt => (4, 0.0),
            Rsqrt => (5, 0.0),
            Recip => (6, 0.0),
            Abs => (7, 0.0),
            Tanh => (8, 0.0),
            Erf => (9, 0.0),
            GeluExact => (10, 0.0),
            LeakyRelu(a) => (11, a),
            Square => (12, 0.0),
            Round => (13, 0.0),
            PowConst(e) => (14, e),
            Floor => (15, 0.0),
            Ceil => (16, 0.0),
            Sin => (17, 0.0),
            Cos => (18, 0.0),
        }
    }
}

/// Reductions along an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduce {
    Sum = 0,
    Mean = 1,
    Max = 2,
    Min = 3,
    Product = 4,
    MeanOfSquares = 5,
}

/// How padding is filled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pad {
    Constant(f32),
    Reflect,
    Edge,
}

/// Where a strided copy reads: element `i` (an index per axis of the
/// copy's shape) comes from `offset + Σ ((i / div) % modulo) · strides`.
#[derive(Clone, Debug)]
pub struct View {
    pub offset: usize,
    pub strides: Vec<isize>,
    /// 1 on every axis but a nearest upsample's.
    pub div: Vec<usize>,
    /// Large on every axis but a tile's.
    pub modulo: Vec<usize>,
}

impl View {
    /// Reading with these strides from `offset`.
    pub fn strided(offset: usize, strides: &[isize]) -> View {
        View {
            offset,
            strides: strides.to_vec(),
            div: vec![1; strides.len()],
            modulo: vec![i32::MAX as usize; strides.len()],
        }
    }
}

/// One operand of a matrix product, by strides in its buffer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Operand {
    pub offset: usize,
    /// Strides along its two matrix axes (rows, columns).
    pub rows: isize,
    pub cols: isize,
    /// Strides along the up to three batch axes (0 where it is broadcast).
    pub batch: [isize; 3],
}

/// `C[b, m, n] = Σk A[b, m, k] · B[b, k, n]`, each addressed by strides; the
/// output `C` is a new buffer of `out_len` floats.
#[derive(Clone, Copy, Debug)]
pub struct MatMul {
    pub m: usize,
    pub n: usize,
    pub k: usize,
    /// The batch shape, padded with 1s at the front.
    pub batch: [usize; 3],
    pub a: Operand,
    pub b: Operand,
    pub c: Operand,
    pub out_len: usize,
}

/// A 2-D convolution (or pooling) of an NCHW input; a 1-D one has `h = 1`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Conv {
    pub n: usize,
    pub c: usize,
    pub h: usize,
    pub w: usize,
    pub o: usize,
    pub oh: usize,
    pub ow: usize,
    pub kh: usize,
    pub kw: usize,
    pub stride: [usize; 2],
    /// Padding before (top, left); after is implied by the output size.
    pub pad: [usize; 2],
    pub dilation: [usize; 2],
    pub group: usize,
}

impl Conv {
    fn params(&self) -> Vec<i32> {
        let g = self.group.max(1);
        let mut p = vec![
            self.n,
            self.c,
            self.h,
            self.w,
            self.o,
            self.oh,
            self.ow,
            self.kh,
            self.kw,
            self.stride[0],
            self.stride[1],
            self.pad[0],
            self.pad[1],
            self.dilation[0],
            self.dilation[1],
            0,
            self.c / g,
            self.o / g,
        ]
        .into_iter()
        .map(|v| v as i32)
        .collect::<Vec<_>>();
        p.resize(20, 0);
        p[19] = (self.n * self.o * self.oh * self.ow) as i32;
        p
    }
}

/// The compiled kernels, all of one layout.
pub(crate) struct Kernels {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    /// Made when first used: most models need only a few.
    pipelines: std::sync::Mutex<HashMap<&'static str, wgpu::ComputePipeline>>,
    /// Bound where a kernel takes fewer inputs.
    empty: wgpu::Buffer,
}

const ENTRIES: [&str; 14] = [
    "copy",
    "fill",
    "binary",
    "unary",
    "matmul",
    "conv",
    "conv_direct",
    "deconv",
    "pad",
    "reduce",
    "softmax",
    "rms_norm",
    "max_pool",
    "gather",
];

impl Kernels {
    pub(crate) fn new(gpu: &Gpu) -> Kernels {
        let buffer = |i: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding: i,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let entries = [
            buffer(0, false),
            buffer(1, true),
            buffer(2, true),
            buffer(3, true),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nn"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nn"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nn"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/nn.wgsl").into()),
        });
        let pipelines = std::sync::Mutex::new(HashMap::new());
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Kernels {
            layout,
            pipeline_layout,
            module,
            pipelines,
            empty,
        }
    }

    fn pipeline(&self, gpu: &Gpu, entry: &str) -> wgpu::ComputePipeline {
        let name = ENTRIES
            .iter()
            .find(|e| **e == entry)
            .expect("a kernel in nn.wgsl");
        let mut made = self.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        made.entry(name)
            .or_insert_with(|| {
                gpu.device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(name),
                        layout: Some(&self.pipeline_layout),
                        module: &self.module,
                        entry_point: Some(name),
                        compilation_options: Default::default(),
                        cache: None,
                    })
            })
            .clone()
    }
}

/// Dispatches recorded before the list is sent to the GPU.
const BATCH: usize = 64;

/// Work on the GPU for one run of a model.
pub struct Session<'g> {
    gpu: &'g Gpu,
    encoder: Option<wgpu::CommandEncoder>,
    queued: usize,
    /// Freed buffers by size, for reuse.
    pool: HashMap<u64, Vec<Arc<wgpu::Buffer>>>,
}

impl Gpu {
    /// Start recording work for a model run.
    pub fn nn(&self) -> Session<'_> {
        Session {
            gpu: self,
            encoder: None,
            queued: 0,
            pool: HashMap::new(),
        }
    }

    /// The most floats one tensor may hold (one buffer binding).
    pub fn nn_max_floats(&self) -> usize {
        let l = self.device.limits();
        (l.max_storage_buffer_binding_size as u64).min(l.max_buffer_size) as usize / 4
    }

    fn nn_kernels(&self) -> &Kernels {
        self.nn.get_or_init(|| Kernels::new(self))
    }
}

/// Buffers are made in sizes a little above what was asked, so freed ones
/// fit later tensors (at most an eighth wasted).
fn size_class(bytes: u64) -> u64 {
    let bytes = bytes.max(16);
    let top = 63 - bytes.leading_zeros();
    let step = (1u64 << top.saturating_sub(3)).max(16);
    bytes.div_ceil(step) * step
}

impl Session<'_> {
    fn new_buffer(&mut self, floats: usize) -> Arc<wgpu::Buffer> {
        let size = size_class(floats as u64 * 4);
        if let Some(b) = self.pool.get_mut(&size).and_then(|v| v.pop()) {
            return b;
        }
        self.gpu.count(size as usize);
        Arc::new(self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tensor"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }))
    }

    /// A new tensor with undefined values.
    pub fn empty(&mut self, shape: &[usize]) -> Tensor {
        let len = shape.iter().product::<usize>();
        Tensor {
            buf: self.new_buffer(len),
            shape: shape.to_vec(),
        }
    }

    /// Values from the CPU.
    pub fn upload(&mut self, data: &[f32], shape: &[usize]) -> Tensor {
        assert_eq!(data.len(), shape.iter().product::<usize>());
        // A queue write lands before work recorded but not yet sent, which
        // may still read the reused buffer: send that work first.
        self.flush();
        let t = self.empty(shape);
        if !data.is_empty() {
            self.gpu
                .queue
                .write_buffer(&t.buf, 0, bytemuck::cast_slice(data));
        }
        t
    }

    /// A tensor kept outside the session (a model's weights): a buffer of its
    /// own, not from the pool.
    pub fn constant(&self, data: &[f32], shape: &[usize]) -> Tensor {
        assert_eq!(data.len(), shape.iter().product::<usize>());
        let mut padded = data.to_vec();
        padded.resize(data.len().max(4), 0.0);
        self.gpu.count(padded.len() * 4);
        let buf = self
            .gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weights"),
                contents: bytemuck::cast_slice(&padded),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        Tensor {
            buf: Arc::new(buf),
            shape: shape.to_vec(),
        }
    }

    /// Give a tensor's buffer back for reuse, once nothing else shares it.
    pub fn free(&mut self, t: Tensor) {
        if Arc::strong_count(&t.buf) == 1 {
            self.pool.entry(t.buf.size()).or_default().push(t.buf);
        }
    }

    /// Send what is recorded to the GPU.
    pub fn flush(&mut self) {
        if let Some(e) = self.encoder.take() {
            self.gpu.queue.submit([e.finish()]);
        }
        self.queued = 0;
    }

    /// Read a tensor back (waits for the work before it).
    pub fn download(&mut self, t: &Tensor) -> Vec<f32> {
        self.flush();
        if t.is_empty() {
            return Vec::new();
        }
        self.gpu.download(&t.buf, t.len())
    }

    fn dispatch(
        &mut self,
        entry: &str,
        bufs: [Option<&Tensor>; 4],
        params: &[i32],
        groups: [u32; 3],
    ) {
        let kernels = self.gpu.nn_kernels();
        let pipeline = kernels.pipeline(self.gpu, entry);
        let mut p = [0i32; 64];
        p[..params.len()].copy_from_slice(params);
        let uniform = self
            .gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nn params"),
                contents: bytemuck::cast_slice(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let mut entries: Vec<wgpu::BindGroupEntry> = bufs
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.map_or(&kernels.empty, |t| &t.buf).as_entire_binding(),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: uniform.as_entire_binding(),
        });
        let bind = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &kernels.layout,
                entries: &entries,
            });
        let encoder = self
            .encoder
            .get_or_insert_with(|| self.gpu.device.create_command_encoder(&Default::default()));
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        }
        self.queued += 1;
        if self.queued >= BATCH {
            self.flush();
        }
    }

    /// A flat dispatch over `items` threads.
    fn flat(&mut self, entry: &str, bufs: [Option<&Tensor>; 4], params: &[i32], items: usize) {
        if items == 0 {
            return;
        }
        let groups = (items as u32).div_ceil(256);
        let x = groups.min(65535);
        self.dispatch(entry, bufs, params, [x, groups.div_ceil(x), 1]);
    }

    /// A tensor of `shape` read from `src` through `view`.
    pub fn copy(&mut self, src: &Tensor, view: &View, shape: &[usize]) -> Tensor {
        let out = self.empty(shape);
        let strides = contiguous(shape);
        self.copy_into(&out, 0, &strides, src, view, shape);
        out
    }

    /// Write `src` (read through `view`, over `shape`) into `dst` at
    /// `offset` with `strides` per axis.
    pub fn copy_into(
        &mut self,
        dst: &Tensor,
        offset: usize,
        strides: &[isize],
        src: &Tensor,
        view: &View,
        shape: &[usize],
    ) {
        let n: usize = shape.iter().product();
        let k = RANK - shape.len();
        let mut p = vec![0i32; 34];
        p[0] = n as i32;
        p[1] = view.offset as i32;
        p[2] = offset as i32;
        for i in 0..RANK {
            p[4 + i] = 1;
            p[22 + i] = 1;
            p[28 + i] = i32::MAX;
        }
        for (i, &d) in shape.iter().enumerate() {
            p[4 + k + i] = d as i32;
            p[10 + k + i] = view.strides[i] as i32;
            p[16 + k + i] = strides[i] as i32;
            p[22 + k + i] = view.div[i] as i32;
            p[28 + k + i] = view.modulo[i].min(i32::MAX as usize) as i32;
        }
        self.flat("copy", [Some(dst), Some(src), None, None], &p, n);
    }

    /// A tensor of `shape` holding `value`.
    pub fn fill(&mut self, shape: &[usize], value: f32) -> Tensor {
        let out = self.empty(shape);
        let n = out.len();
        self.flat(
            "fill",
            [Some(&out), None, None, None],
            &[n as i32, value.to_bits() as i32],
            n,
        );
        out
    }

    /// `a ∘ b`, broadcast to `shape` (both of its rank).
    pub fn binary(&mut self, op: Binary, a: &Tensor, b: &Tensor, shape: &[usize]) -> Tensor {
        let out = self.empty(shape);
        let n = out.len();
        let k = RANK - shape.len();
        let mut p = vec![0i32; 22];
        p[0] = n as i32;
        p[1] = op as i32;
        for i in 0..RANK {
            p[4 + i] = 1;
        }
        let (sa, sb) = (broadcast(&a.shape, shape), broadcast(&b.shape, shape));
        for i in 0..shape.len() {
            p[4 + k + i] = shape[i] as i32;
            p[10 + k + i] = sa[i] as i32;
            p[16 + k + i] = sb[i] as i32;
        }
        self.flat("binary", [Some(&out), Some(a), Some(b), None], &p, n);
        out
    }

    pub fn unary(&mut self, op: Unary, a: &Tensor) -> Tensor {
        let out = self.empty(&a.shape);
        let n = out.len();
        let (code, alpha) = op.code();
        self.flat(
            "unary",
            [Some(&out), Some(a), None, None],
            &[n as i32, code, alpha.to_bits() as i32],
            n,
        );
        out
    }

    /// A matrix product (see [`MatMul`]), into a new tensor of `shape`.
    pub fn matmul(&mut self, a: &Tensor, b: &Tensor, mm: &MatMul, shape: &[usize]) -> Tensor {
        assert!(shape.iter().product::<usize>() >= mm.out_len);
        let out = self.empty(shape);
        let mut p = vec![0i32; 32];
        p[0] = mm.m as i32;
        p[1] = mm.n as i32;
        p[2] = mm.k as i32;
        for i in 0..3 {
            p[4 + i] = mm.batch[i] as i32;
        }
        for (at, o) in [(8, &mm.a), (16, &mm.b), (24, &mm.c)] {
            p[at] = o.offset as i32;
            p[at + 1] = o.rows as i32;
            p[at + 2] = o.cols as i32;
            for i in 0..3 {
                p[at + 4 + i] = o.batch[i] as i32;
            }
        }
        // B's operand strides are (k, n): its rows are the shared axis.
        let batches = (mm.batch.iter().product::<usize>()) as u32;
        if mm.m * mm.n > 0 && batches > 0 {
            if mm.k == 0 {
                let zero = self.fill(shape, 0.0);
                return zero;
            }
            self.dispatch(
                "matmul",
                [Some(&out), Some(a), Some(b), None],
                &p,
                [
                    (mm.n as u32).div_ceil(64),
                    (mm.m as u32).div_ceil(64),
                    batches,
                ],
            );
        }
        out
    }

    /// A convolution of `x` (NCHW) with `kernel` (OIHW) and an optional bias.
    pub fn conv(&mut self, x: &Tensor, kernel: &Tensor, bias: Option<&Tensor>, c: &Conv) -> Tensor {
        let out = self.empty(&[c.n, c.o, c.oh, c.ow]);
        let mut p = c.params();
        p[18] = bias.is_some() as i32;
        let g = c.group.max(1);
        let og = c.o / g;
        if og < 16 {
            self.flat(
                "conv_direct",
                [Some(&out), Some(x), Some(kernel), bias],
                &p,
                out.len(),
            );
        } else {
            let cols = (c.n * c.oh * c.ow) as u32;
            self.dispatch(
                "conv",
                [Some(&out), Some(x), Some(kernel), bias],
                &p,
                [cols.div_ceil(64), (og as u32).div_ceil(64), g as u32],
            );
        }
        out
    }

    /// A transposed convolution of `x` (NCHW); `kernel` is read with
    /// `strides` per (input channel, output channel in its group, ky, kx).
    pub fn deconv(
        &mut self,
        x: &Tensor,
        kernel: &Tensor,
        strides: [usize; 4],
        bias: Option<&Tensor>,
        c: &Conv,
    ) -> Tensor {
        let out = self.empty(&[c.n, c.o, c.oh, c.ow]);
        let mut p = c.params();
        p[18] = bias.is_some() as i32;
        p.resize(24, 0);
        for i in 0..4 {
            p[20 + i] = strides[i] as i32;
        }
        self.flat(
            "deconv",
            [Some(&out), Some(x), Some(kernel), bias],
            &p,
            out.len(),
        );
        out
    }

    /// `a` padded by `before` and `after` on each axis.
    pub fn pad(&mut self, a: &Tensor, before: &[usize], after: &[usize], mode: Pad) -> Tensor {
        let shape: Vec<usize> = (0..a.shape.len())
            .map(|i| a.shape[i] + before[i] + after[i])
            .collect();
        let out = self.empty(&shape);
        let n = out.len();
        let k = RANK - shape.len();
        let mut p = vec![0i32; 22];
        p[0] = n as i32;
        let (code, value) = match mode {
            Pad::Constant(v) => (0, v),
            Pad::Reflect => (1, 0.0),
            Pad::Edge => (2, 0.0),
        };
        p[1] = code;
        p[2] = value.to_bits() as i32;
        for i in 0..RANK {
            p[4 + i] = 1;
            p[10 + i] = 1;
        }
        for i in 0..shape.len() {
            p[4 + k + i] = shape[i] as i32;
            p[10 + k + i] = a.shape[i] as i32;
            p[16 + k + i] = before[i] as i32;
        }
        self.flat("pad", [Some(&out), Some(a), None, None], &p, n);
        out
    }

    fn along(
        &mut self,
        entry: &str,
        a: &Tensor,
        axis: usize,
        op: i32,
        extra: f32,
        shape: &[usize],
    ) -> Tensor {
        let outer: usize = a.shape[..axis].iter().product();
        let len = a.shape[axis];
        let inner: usize = a.shape[axis + 1..].iter().product();
        let out = self.empty(shape);
        let p = [
            (outer * inner) as i32,
            op,
            outer as i32,
            len as i32,
            inner as i32,
            extra.to_bits() as i32,
        ];
        self.flat(entry, [Some(&out), Some(a), None, None], &p, outer * inner);
        out
    }

    /// `a` reduced along `axis` (kept, with length 1).
    pub fn reduce(&mut self, op: Reduce, a: &Tensor, axis: usize) -> Tensor {
        let mut shape = a.shape.clone();
        shape[axis] = 1;
        self.along("reduce", a, axis, op as i32, 0.0, &shape)
    }

    /// Softmax along `axis`, or its log.
    pub fn softmax(&mut self, a: &Tensor, axis: usize, log: bool) -> Tensor {
        let shape = a.shape.clone();
        self.along("softmax", a, axis, log as i32, 0.0, &shape)
    }

    /// `x / √(mean(x²) + eps)` along `axis`.
    pub fn rms_norm(&mut self, a: &Tensor, axis: usize, eps: f32) -> Tensor {
        let shape = a.shape.clone();
        self.along("rms_norm", a, axis, 0, eps, &shape)
    }

    /// Max pooling of an NCHW input (`c.o` = `c.c`).
    pub fn max_pool(&mut self, x: &Tensor, c: &Conv) -> Tensor {
        let out = self.empty(&[c.n, c.c, c.oh, c.ow]);
        let mut p = c.params();
        p[19] = out.len() as i32;
        self.flat("max_pool", [Some(&out), Some(x), None, None], &p, out.len());
        out
    }

    /// `a` gathered along `axis` at `indices` (negative ones count from
    /// the end), the indices' shape in place of the axis.
    pub fn gather(&mut self, a: &Tensor, axis: usize, indices: &[i64], shape: &[usize]) -> Tensor {
        let outer: usize = a.shape[..axis].iter().product();
        let len = a.shape[axis];
        let inner: usize = a.shape[axis + 1..].iter().product();
        let idx: Vec<f32> = indices
            .iter()
            .map(|&i| f32::from_bits(i as i32 as u32))
            .collect();
        let idx = self.upload(&idx, &[indices.len()]);
        let out = self.empty(shape);
        let n = outer * indices.len() * inner;
        let p = [
            n as i32,
            0,
            outer as i32,
            indices.len() as i32,
            inner as i32,
            len as i32,
        ];
        self.flat("gather", [Some(&out), Some(a), Some(&idx), None], &p, n);
        self.free(idx);
        out
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Row-major strides of `shape`.
pub fn contiguous(shape: &[usize]) -> Vec<isize> {
    let mut s = vec![1isize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1] * shape[i + 1] as isize;
    }
    s
}

/// Strides reading `from` (contiguous) broadcast to `to` (same rank): 0
/// along axes it has once.
pub fn broadcast(from: &[usize], to: &[usize]) -> Vec<isize> {
    let s = contiguous(from);
    (0..to.len())
        .map(|i| if from[i] == 1 && to[i] != 1 { 0 } else { s[i] })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
                                        sum += x[((n * c_in + ch) * 9 + iy as usize) * 11
                                            + ix as usize]
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
}
