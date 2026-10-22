// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted UI tests: `ambrotype --ui-test SCRIPT.js [PHOTO]`.
//!
//! The editor starts as usual, then the page runs the script with the helpers
//! in `ui/uitest.js` (click, type, set a slider, wait for something) and
//! reports back here; the process exits 0 when every step passed. Save
//! dialogs don't open: exports go to a folder of their own, printed at the
//! start. A test that hangs is stopped after `LIMIT`. No driver or browser
//! automation library is involved, so it runs wherever the editor runs.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;

/// A hung test fails rather than holding CI.
const LIMIT: Duration = Duration::from_secs(300);

#[derive(Serialize, Clone)]
pub(crate) struct UiTest {
    /// The test script's text.
    pub(crate) script: String,
    /// Where exports go instead of a save dialog.
    pub(crate) out_dir: PathBuf,
}

static TEST: OnceLock<UiTest> = OnceLock::new();

/// The running test, if this is a test run.
pub(crate) fn current() -> Option<&'static UiTest> {
    TEST.get()
}

/// Start a test run from `--ui-test SCRIPT`, or say why not.
pub(crate) fn start(script: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(script).map_err(|e| format!("{script}: {e}"))?;
    let out_dir = std::env::temp_dir().join(format!("ambrotype-ui-test-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    eprintln!("ui test {script}: exports go to {}", out_dir.display());
    let _ = TEST.set(UiTest {
        script: text,
        out_dir,
    });
    std::thread::spawn(|| {
        std::thread::sleep(LIMIT);
        eprintln!("ui test: FAILED: no result after {} s", LIMIT.as_secs());
        std::process::exit(2);
    });
    Ok(())
}

/// The page asks whether this is a test run. If so, the script is put into
/// the page from here, as `window.__ambUiTest` (an async function of `t`):
/// the page's content security policy rightly forbids it to `eval` text.
#[tauri::command]
pub(crate) fn ui_test(webview: tauri::Webview) -> Result<Option<UiTest>, String> {
    let Some(test) = current() else {
        return Ok(None);
    };
    webview
        .eval(format!(
            "window.__ambUiTest = async function (t) {{\n{}\n}};",
            test.script
        ))
        .map_err(|e| e.to_string())?;
    Ok(Some(test.clone()))
}

/// The page reports the result: each step's line, then the exit.
#[tauri::command]
pub(crate) fn ui_test_done(passed: bool, report: Vec<String>) {
    if current().is_none() {
        return;
    }
    for line in &report {
        eprintln!("{line}");
    }
    eprintln!("ui test: {}", if passed { "passed" } else { "FAILED" });
    std::process::exit(if passed { 0 } else { 1 });
}
