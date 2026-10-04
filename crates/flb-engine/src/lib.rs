//! Flightbroom processing engine, v0.
//!
//! A first end-to-end path: decode a raw ([`raw`]), develop it in linear
//! ProPhoto with white balance, Exposure and point curves ([`pipeline`]), and
//! write sRGB output ([`output`]). The full tiled, GPU-backed graph described in
//! docs/SCOPE.md §9 replaces this incrementally; this crate fixes the colour
//! maths and gives the calibration harness something to compare against.

pub mod curve;
pub mod output;
pub mod pipeline;
pub mod raw;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("raw decode failed: {0}")]
    Decode(String),
    #[error("unsupported input: {0}")]
    Unsupported(String),
    #[error("output failed: {0}")]
    Output(String),
}
