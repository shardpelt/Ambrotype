// SPDX-License-Identifier: GPL-3.0-or-later
//! Ambrotype command protocol.
//!
//! The single, typed API for changing develop settings: the GUI, the `amb`
//! CLI, scripts and AI clients all go through it, with the same validation,
//! diffs and undo (docs/design/ai-control.md).
//!
//! - [`DevelopState`]: a sparse Camera Raw settings map (`crs:` names to
//!   [`amb_presets::Value`]) plus undo/redo history; loads and saves XMP
//!   sidecars and JSON.
//! - [`Command`]: undoable edits (`set_param`, `adjust_param`, `reset_param`,
//!   `reset_group`, `apply_preset`, the [`masks`] commands `add_mask`,
//!   `set_mask`, `paint_mask`, `remove_mask`, the [`spots`] commands
//!   `add_spot`, `set_spot`, `remove_spot`, and `undo`, `redo`), validated
//!   against the tag registry. Each returns an [`Outcome`] with a diff.
//! - [`Query`]: read-only requests (`get_param`, `list_params`, `describe`,
//!   `list_groups`, `list_masks`, `list_spots`).
//! - [`schema`]: JSON Schemas for both; [`reference`]: the generated settings
//!   reference; [`catalog`]: tag descriptions and name suggestions.
//! - [`compact`]: the one-line edit syntax (`exp=+0.5 con+=10`); [`card`]:
//!   the short references for AI clients (`amb ref`).

pub mod card;
pub mod catalog;
pub mod command;
pub mod compact;
pub mod convert;
pub mod edit;
pub mod masks;
pub mod reference;
pub mod schema;
pub mod sidecar;
pub mod spots;
pub mod state;

pub use command::{Command, Query, parse_commands, parse_queries};
pub use convert::Clamp;
pub use state::{Change, DevelopState, HistoryEntry, Outcome, Snapshot};

fn did_you_mean(suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(" (did you mean {}?)", suggestions.join(", "))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "unknown setting {name:?}{}; all settings are listed by `amb settings reference` (docs/ai/settings-reference.md)",
        did_you_mean(suggestions)
    )]
    UnknownTag {
        name: String,
        suggestions: Vec<String>,
    },
    #[error(
        "unknown group {name:?}{}; groups: {}",
        did_you_mean(suggestions),
        known.join(", ")
    )]
    UnknownGroup {
        name: String,
        suggestions: Vec<String>,
        known: Vec<String>,
    },
    #[error("{name} expects {expected}, got {got}")]
    WrongKind {
        name: String,
        expected: String,
        got: String,
    },
    #[error("{name}: {reason}")]
    InvalidValue { name: String, reason: String },
    #[error(
        "{name} has no value in this state and no fixed default to adjust from (it depends on the image); use set_param with an absolute value{}",
        if name == "Temperature" { " (`amb render` prints the as-shot temperature)" } else { "" }
    )]
    NoBase { name: String },
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("{0}")]
    Load(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Json(String),
    /// A compact edit line that doesn't parse ([`compact`]).
    #[error("{0}")]
    Syntax(String),
    #[error("command {index} ({label}): {source}")]
    InCommand {
        index: usize,
        label: String,
        source: Box<Error>,
    },
}
