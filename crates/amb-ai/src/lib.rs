// SPDX-License-Identifier: GPL-3.0-or-later
//! Local AI models, for the masks that need one (Select Subject and
//! Background, Select People; [`amb_engine::subject`]).
//!
//! - **Which models:** `data/models.json` lists the ones we can install:
//!   only models whose weights are published under a licence that allows use
//!   in free software (Apache-2.0, MIT, …), each with its licence, authors
//!   and source. None ships with the app. Each task (`subject`, `people`) has
//!   a default; the user (or their AI assistant, through `amb models` or MCP)
//!   can install others and choose one per task.
//! - **Your own models:** any ONNX model that takes a square RGB photo and
//!   gives a map of the same side can be added with [`add_custom`], with its
//!   input size, normalisation and output activation.
//! - **Where:** the editor's config folder, `models/` (or `AMB_MODELS`):
//!   `<id>.onnx` for each installed model, `choices.json` for the chosen ones,
//!   and `custom/<id>.json` for the user's own models' descriptions.
//! - **Downloads** come from where the authors (or the converter) publish
//!   the model, and are checked against the sha256 in the list.
//! - **Running** is local: on the GPU with our own kernels when there is
//!   one (`net`), else with `tract` (pure Rust) on all cores. The result
//!   for a photo is kept, so moving sliders doesn't run it again; only a new
//!   photo or crop does.

use std::path::PathBuf;

mod archive;
pub mod denoise;
pub mod faces;
pub mod hooks;
mod net;
pub use net::runs_on;

mod model;
mod spec;
mod store;
#[cfg(test)]
mod tests;

pub use model::{finder, try_model};
pub use spec::{
    Activation, CustomModel, Input, Layout, ModelFile, ModelSpec, ModelState, Output, Pixels,
    catalogue,
};
pub use store::{add_custom, choose, chosen, ensure_model, install, list, remove};
pub(crate) use store::{file_of, is_installed};
pub mod objects;
pub mod remove;
pub mod search;

/// The tasks a model can do, with what they're for.
pub const TASKS: [(&str, &str); 7] = [
    ("subject", "Select Subject and Select Background"),
    ("people", "Select People (everyone, as one mask)"),
    (
        "search",
        "Smart search: find photos by what is in them, or like another one",
    ),
    (
        "remove",
        "AI Remove: fill a Remove spot with what belongs there",
    ),
    (
        "objects",
        "Select an object by a click or a box (Remove object)",
    ),
    (
        "faces",
        "Find faces and eyes: Best in each burst picks the shot with eyes open",
    ),
    (
        "denoise",
        "AI Denoise: a raw without its noise, as a new DNG",
    ),
];

/// Models up to this size install by themselves the first time a mask needs
/// them; bigger ones are installed on request.
const AUTO_INSTALL_BYTES: u64 = 20 << 20;

/// The editor's models folder (next to its settings).
pub fn default_dir() -> Option<PathBuf> {
    let env = |v: &str| {
        std::env::var_os(v)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    if let Some(d) = env("AMB_MODELS") {
        return Some(d);
    }
    let config = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(config.join("org.ambrotype.editor").join("models"))
}
