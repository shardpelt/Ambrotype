// SPDX-License-Identifier: GPL-3.0-or-later
//! The one-line progress `amb` shows on stderr while long work runs.

/// A progress callback that rewrites one line on stderr: `text(done, total)` is the line. Silent with `--json` (`quiet`). A newline at the end.
pub(crate) fn line(
    quiet: bool,
    text: impl Fn(usize, usize) -> String + Sync,
) -> impl Fn(usize, usize) + Sync {
    move |done, total| {
        if quiet {
            return;
        }
        eprint!("\r{}", text(done, total));
        if done == total {
            eprintln!();
        }
    }
}
