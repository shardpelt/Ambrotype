// SPDX-License-Identifier: GPL-3.0-or-later
//! Progress events for long editor commands, sent to the page at a pace it can draw.

/// A progress callback that sends `[done, total]` to the page as `event`, at most every 200 ms (and always at the end).
pub(crate) fn throttled(
    app: &tauri::AppHandle,
    event: &'static str,
) -> impl Fn(usize, usize) + Send + Sync + 'static {
    use tauri::Emitter;
    let app = app.clone();
    let last = std::sync::Mutex::new(std::time::Instant::now());
    move |done, total| {
        let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
        if done == total || last.elapsed().as_millis() > 200 {
            *last = std::time::Instant::now();
            let _ = app.emit(event, [done, total]);
        }
    }
}
