// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb man`: the manual page (`man amb`), made from the help text so the
//! two never disagree. `docs/man/amb.1` is a copy, checked by a test;
//! regenerate it with `cargo run -p amb-cli -- man > docs/man/amb.1`.

/// Escape text for roff: backslashes, and a line can't start with . or '.
fn esc(s: &str) -> String {
    let s = s
        .replace('\\', "\\e")
        .replace('-', "\\-")
        .replace('"', "\\(dq");
    if s.starts_with('.') || s.starts_with('\'') {
        format!("\\&{s}")
    } else {
        s
    }
}

/// A help line's command (or option) and its words, split at the first run
/// of two spaces after the start.
fn split_entry(line: &str) -> (&str, &str) {
    let t = line.trim_start();
    match t.find("  ") {
        Some(i) => (t[..i].trim_end(), t[i..].trim()),
        None => (t, ""),
    }
}

/// The page, in roff (`man 7 man`), from `usage`.
pub(crate) fn page(usage: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        ".TH AMB 1 \"\" \"Ambrotype {}\" \"User Commands\"\n",
        env!("CARGO_PKG_VERSION")
    ));
    out.push_str(".SH NAME\namb \\- Ambrotype's command\\-line tool: develop raws, edit, organise and export photos\n");
    out.push_str(".SH SYNOPSIS\n.B amb\n.I command\n.RI [ arguments ]\n.RB [ \\-\\-json ]\n.RB [ \\-\\-dry\\-run ]\n");
    out.push_str(".SH DESCRIPTION\nAmbrotype is a free, open\\-source photo editor that reads and writes Lightroom's XMP sidecars and presets. \\fBamb\\fR does from a terminal (or a script, or an AI assistant) what the editor does: every edit goes through the same undoable commands and lands in the photo's sidecar.\n");
    // Entries: (command, words), flushed as .TP paragraphs.
    let mut entry: Option<(String, String)> = None;
    let flush = |out: &mut String, e: &mut Option<(String, String)>| {
        if let Some((cmd, words)) = e.take() {
            out.push_str(&format!(".TP\n.B {}\n{}\n", esc(&cmd), esc(words.trim())));
        }
    };
    let mut in_usage = false;
    let mut para = String::new();
    for line in usage.lines() {
        let indent = line.len() - line.trim_start().len();
        let text = line.trim();
        if !in_usage {
            in_usage = text == "USAGE:";
            if in_usage {
                out.push_str(".SH COMMANDS\n");
            }
            continue;
        }
        if text.is_empty() {
            flush(&mut out, &mut entry);
            if !para.is_empty() {
                out.push_str(&format!(".PP\n{}\n", esc(para.trim())));
                para.clear();
            }
            continue;
        }
        if indent == 0 {
            flush(&mut out, &mut entry);
            if let Some(head) = text.strip_suffix(':').filter(|h| {
                h.chars()
                    .all(|c| !c.is_lowercase() || c == '(' || c == ')' || c == ' ')
                    || h.contains("OPTIONS")
            }) {
                out.push_str(&format!(".SH {}\n", esc(head)));
            } else {
                para.push_str(text);
                para.push(' ');
            }
            continue;
        }
        if indent == 4 {
            flush(&mut out, &mut entry);
            let (cmd, words) = split_entry(line);
            entry = Some((cmd.to_string(), words.to_string()));
            continue;
        }
        let Some((cmd, words)) = entry.as_mut() else {
            continue;
        };
        // Under the command: more of its arguments, or more of its words.
        let more_args =
            indent < 40 || (words.is_empty() && (text.starts_with('[') || text.starts_with('<')));
        if more_args {
            cmd.push(' ');
            cmd.push_str(text);
        } else {
            words.push(' ');
            words.push_str(text);
        }
    }
    flush(&mut out, &mut entry);
    if !para.is_empty() {
        out.push_str(&format!(".PP\n{}\n", esc(para.trim())));
    }
    out.push_str(concat!(
        ".SH ENVIRONMENT\n",
        ".TP\n.B AMB_GPU\n\\fBoff\\fR develops on the CPU even when there is a GPU.\n",
        ".TP\n.B AMB_AI_GPU\n\\fBoff\\fR runs the AI models on the CPU.\n",
        ".TP\n.B AMB_MODELS\nThe folder of AI models (default: the editor's).\n",
        ".TP\n.B AMB_DCP_DIRS\nMore folders to look for camera profiles (.dcp) in.\n",
        ".SH FILES\n",
        ".TP\n.I PHOTO.xmp\nThe sidecar next to each photo: its edit, rating, label, flag, keywords and place, as Lightroom keeps them.\n",
        ".TP\n.I catalog.sqlite\nThe editor's catalogue, in its config folder: an index of what was read from the photos (deleting it loses nothing).\n",
        ".SH SEE ALSO\n",
        "The guide for scripts and AI assistants: docs/ai/README.md in the source, and \\fBamb ref\\fR. ",
        "Home: https://github.com/shardpelt/Ambrotype\n",
        ".SH BUGS\nReport them with the editor's Feedback button, or at https://github.com/shardpelt/Ambrotype/issues\n",
        ".SH COPYRIGHT\nCopyright (C) 2026 The Ambrotype contributors. Free software under the GNU GPL, version 3 or later, with ABSOLUTELY NO WARRANTY.\n",
    ));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_man_page_in_docs_is_current() {
        let want = super::page(crate::USAGE);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/man/amb.1");
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "docs/man/amb.1 is out of date: cargo run -p amb-cli -- man > docs/man/amb.1"
        );
    }

    #[test]
    fn every_command_is_an_entry() {
        let page = super::page(crate::USAGE);
        for cmd in ["amb render", "amb insights", "amb place", "amb mcp"] {
            assert!(page.contains(&format!(".B {}", cmd)), "{cmd}");
        }
        assert!(page.contains(".SH EXPORT OPTIONS"), "the options section");
    }
}
