// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline on the GPU (wgpu compute shaders), for the editor's
//! preview. This is the only crate that touches wgpu.
//!
//! Each stage is a WGSL kernel with the same maths as its CPU stage in
//! `amb-engine`, and a test that runs both on the same pixels and compares
//! them. [`Gpu::render`] runs an `amb_engine::plan::Plan`; an edit the plan
//! can't express (defringe, camera profiles) is rendered on
//! the CPU by the caller.

mod denoise;
mod develop;
mod device;
mod local;

pub use device::Gpu;
