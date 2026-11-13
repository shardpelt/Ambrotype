// SPDX-License-Identifier: GPL-3.0-or-later
//! The error of an editor command: one type, so `?` converts what a library returns.

/// What an editor command returns on failure. The page receives it as the plain text.
pub(crate) struct CmdError(String);

// Not `Display` or `Error` itself: that is what lets every other error convert.
impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError(e.to_string())
    }
}

impl serde::Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

// For the functions that still return `Result<_, String>`.
impl From<CmdError> for String {
    fn from(e: CmdError) -> String {
        e.0
    }
}

impl CmdError {
    pub(crate) fn text(&self) -> &str {
        &self.0
    }
}

pub(crate) type Res<T> = Result<T, CmdError>;
