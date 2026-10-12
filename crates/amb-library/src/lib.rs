// SPDX-License-Identifier: GPL-3.0-or-later
//! Library operations that work on folders of photos and their XMP
//! sidecars, shared by the `amb` tool and the editor. Nothing here keeps a
//! catalogue: the files are the library.

pub mod collections;
pub mod import;
pub mod lensdb;
pub mod look;
pub mod lrcat;
pub mod merge;
pub mod negative;
pub mod smart;
