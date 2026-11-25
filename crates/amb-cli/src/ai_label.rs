// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb ai-label`: what in a photo's edit a platform may label as AI
//! (`amb_ops::ai_label`), for each photo given.

use crate::*;
use amb_ops::ai_label::{self, Verdict};

/// `amb ai-label PHOTO… [--json]`.
pub(crate) fn run(photos: &[&str], json: bool) -> Result<(), String> {
    let labels: Vec<_> = photos
        .iter()
        .map(|p| (*p, ai_label::check_photo(Path::new(p))))
        .collect();
    if json {
        let list: Vec<_> = labels
            .iter()
            .map(|(p, l)| serde_json::json!({ "photo": p, "label": l }))
            .collect();
        return print_json(&serde_json::json!(list));
    }
    for (photo, l) in &labels {
        let line = match l.verdict {
            Verdict::Clear => "no AI marks".to_string(),
            Verdict::AiInEdit => format!("AI in the edit: {}", l.edit_parts.join(", ")),
        };
        println!("{photo}: {line}");
        if let Some(w) = ai_label::left_out_warning(&l.left_out) {
            println!("  {w}");
        }
    }
    println!("Platforms may also run their own checks; this shows what is in the file.");
    Ok(())
}
