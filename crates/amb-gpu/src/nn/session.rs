// SPDX-License-Identifier: GPL-3.0-or-later
//! A session: buffers on the GPU and the operations queued on them.

use super::kernels::Kernels;
use super::ops::{
    Binary, Conv, MatMul, Pad, RANK, Reduce, Tensor, Unary, View, broadcast, contiguous,
};
use crate::Gpu;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

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
                usage: wgpu::BufferUsages::STORAGE,
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
