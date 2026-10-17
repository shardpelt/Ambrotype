// SPDX-License-Identifier: GPL-3.0-or-later
//! The Tauri commands the web UI calls, one file per area. `main` registers
//! them all in `invoke_handler`.

pub(crate) mod appearance;
pub(crate) mod assistant;
pub(crate) mod catalog;
pub(crate) mod collage;
pub(crate) mod export;
pub(crate) mod hub;
pub(crate) mod library;
pub(crate) mod models;
pub(crate) mod photo;
pub(crate) mod plugins;
pub(crate) mod presets;
pub(crate) mod preview;
pub(crate) mod settings;
