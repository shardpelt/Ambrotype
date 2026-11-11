// SPDX-License-Identifier: GPL-3.0-or-later
//! Library operations that work on folders of photos and their XMP
//! sidecars, shared by the `amb` tool and the editor. The files are the
//! library; the catalogue only remembers what was read from them.

pub mod catalog;
pub mod collections;
pub mod contact;
pub mod duplicates;
pub mod files;
pub mod folder;
pub mod gpx;
pub mod import;
pub mod insights;
pub mod lensdb;
pub mod lightroom;
pub mod look;
pub mod lrcat;
pub mod roots;
pub mod search;
pub mod smart;
