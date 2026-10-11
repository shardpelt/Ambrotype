// SPDX-License-Identifier: GPL-3.0-or-later
//! The develop pipeline on the GPU (wgpu compute shaders), for the editor's
//! preview. This is the only crate that touches wgpu.
//!
//! Each stage is a WGSL kernel with the same maths as its CPU stage in
//! `amb-engine`, and a test that runs both on the same pixels and compares
//! them. A render runs every stage on the GPU or none: when an edit uses a
//! stage that has no kernel yet, [`Gpu::render`] says so and the caller
//! renders on the CPU (moving pixels back and forth between stages would
//! cost more than it saves).

mod develop;
mod device;

pub use device::Gpu;
