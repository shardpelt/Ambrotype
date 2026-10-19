// SPDX-License-Identifier: GPL-3.0-or-later
//! Library operations that work on folders of photos and their XMP
//! sidecars, shared by the `amb` tool and the editor. The files are the
//! library; the catalogue only remembers what was read from them.

pub mod catalog;
pub mod collections;
pub mod folder;
pub mod import;
pub mod lensdb;
pub mod look;
pub mod lrcat;
pub mod merge;
pub mod negative;
pub mod roots;
pub mod smart;
