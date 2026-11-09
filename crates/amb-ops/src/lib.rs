// SPDX-License-Identifier: GPL-3.0-or-later
//! Operations that render photos, shared by the `amb` tool and the editor: raws to JPEGs, edit replays, merges, objects, bursts, dust and negatives. The library itself (folders, catalogue, sidecars) is `amb-library`.

pub mod burst;
pub mod convert;
pub mod dust;
pub mod merge;
pub mod negative;
pub mod object;
pub mod replay;
pub mod snap;
