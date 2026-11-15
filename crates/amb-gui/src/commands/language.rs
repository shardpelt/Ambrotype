// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's languages: which there are, and a language's texts.

use crate::lang;

/// The built-in languages a person can pick.
#[tauri::command]
pub(crate) fn language_list() -> Vec<lang::Language> {
    lang::languages()
}

/// A language's texts: English text to translation (`zz` is the test language).
#[tauri::command]
pub(crate) fn language_texts(code: String) -> Result<lang::Texts, String> {
    lang::texts(&code)
}
