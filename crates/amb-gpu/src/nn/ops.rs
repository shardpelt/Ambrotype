// SPDX-License-Identifier: GPL-3.0-or-later
//! The operations a network is made of: tensors, views and each kind of layer.

use std::sync::Arc;

/// The most axes a tensor may have here.
pub const RANK: usize = 6;

/// A contiguous f32 tensor on the GPU (row-major: the last axis is
/// contiguous). Clones share the buffer, as a reshape does.
#[derive(Clone)]
pub struct Tensor {
    pub(super) buf: Arc<wgpu::Buffer>,
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
    pub(super) fn code(self) -> (i32, f32) {
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
    pub(super) fn params(&self) -> Vec<i32> {
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
