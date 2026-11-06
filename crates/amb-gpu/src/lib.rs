// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline on the GPU (wgpu compute shaders), for the editor's
//! preview and full-size exports ([`full`]). This is the only crate that touches wgpu.
//!
//! Each stage is a WGSL kernel with the same maths as its CPU stage in
//! `amb-engine`, and a test that runs both on the same pixels and compares
//! them. [`Gpu::render`] runs an `amb_engine::plan::Plan`; an edit the plan
//! can't express (filling the edges a lens correction leaves) is rendered
//! on the CPU by the caller.
//!
//! [`nn`] runs AI models' layers, for `amb-ai`.

mod denoise;
mod develop;
mod device;
pub mod full;
mod local;
pub mod memory;
pub mod nn;

pub use device::Gpu;
