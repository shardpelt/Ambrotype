// SPDX-License-Identifier: GPL-3.0-or-later
//! Running AI models (neural networks) on the GPU: tensors of f32 and the
//! layers the models we offer are made of (`shaders/nn.wgsl`). Which layer
//! runs when is up to the caller (`amb-ai` walks a model's graph); this
//! module only knows tensors and kernels, not model files.
//!
//! A [`Session`] records work into one command list and sends it to the GPU
//! now and then, and when a result is read back. Buffers freed in it are
//! kept for reuse until it ends.

mod kernels;
mod ops;
mod session;
#[cfg(test)]
mod tests;

pub(crate) use kernels::Kernels;
pub use ops::{
    Binary, Conv, MatMul, Operand, Pad, RANK, Reduce, Tensor, Unary, View, broadcast, contiguous,
};
pub use session::Session;
