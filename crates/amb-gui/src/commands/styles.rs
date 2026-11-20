// SPDX-License-Identifier: GPL-3.0-or-later
//! Styles: saved looks that fit themselves to each photo
//! (`amb_library::styles`, `amb_ops::style`).

use crate::*;
use amb_engine::matching::{Focus, MatchOptions};
use amb_library::styles;

/// A saved style as the dialog lists it.
#[derive(Serialize)]
pub(crate) struct StyleSummary {
    pub(crate) name: String,
    pub(crate) focus: Focus,
    pub(crate) references: Vec<String>,
}

impl From<&styles::Style> for StyleSummary {
    fn from(s: &styles::Style) -> Self {
        StyleSummary {
            name: s.name.clone(),
            focus: s.focus,
            references: s.references.clone(),
        }
    }
}

fn dir() -> Res<PathBuf> {
    Ok(styles::editor_dir().ok_or("no config folder for the styles")?)
}

fn find(name: &str) -> Res<styles::Style> {
    Ok(styles::find(&dir()?, name)?.1)
}

#[tauri::command]
pub(crate) fn styles_list() -> Res<Vec<StyleSummary>> {
    let all = styles::list(&dir()?);
    Ok(all.iter().map(|(_, s)| StyleSummary::from(s)).collect())
}

/// Measure `references` and keep their look as a style.
#[tauri::command]
pub(crate) async fn style_save(
    name: String,
    references: Vec<String>,
    focus: String,
) -> Res<StyleSummary> {
    let dir = dir()?;
    tauri::async_runtime::spawn_blocking(move || {
        let focus = Focus::parse(&focus).ok_or_else(|| format!("unknown focus {focus:?}"))?;
        let paths: Vec<&Path> = references.iter().map(Path::new).collect();
        let style = styles::from_photos(&name, &paths, focus)?;
        styles::save(&dir, &style)?;
        Ok(StyleSummary::from(&style))
    })
    .await?
}

#[tauri::command]
pub(crate) fn style_delete(name: String) -> Res<()> {
    Ok(styles::delete(&dir()?, &name)?)
}

/// The match for the open photo (its own lens, camera profile and Upright).
fn solve_open(
    s: &mut Session,
    style: &styles::Style,
    lens: &LensSources,
) -> amb_engine::matching::MatchResult {
    let opts = MatchOptions {
        parts: style.focus.parts(),
        weights: style.focus.weights(),
        ..Default::default()
    };
    s.match_look(&style.look, opts, lens)
}

/// The commands that take the open photo to the style at `amount` (1.0 =
/// 100 %); the page runs them as one undo step.
#[tauri::command]
pub(crate) async fn style_apply(
    name: String,
    amount: f64,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<Command>> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let style = find(&name)?;
        let mut guard = slot.lock()?;
        let s = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let result = solve_open(s, &style, lens);
        Ok(styles::commands(s.state.settings(), &result, amount))
    })
    .await?
}

#[derive(Serialize)]
pub(crate) struct StyleBatch {
    #[serde(flatten)]
    pub(crate) report: amb_ops::style::Report,
    /// The open photo's new state, when it was one of the photos.
    pub(crate) state: Option<session::StateInfo>,
}

/// The style on many photos. The open photo gets it as one undo step; the
/// others in their sidecars. Progress goes to the page as "style-progress".
#[tauri::command]
pub(crate) async fn styles_apply_batch(
    name: String,
    photos: Vec<String>,
    amount: f64,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<StyleBatch> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let style = find(&name)?;
        save_open_edits(&slot)?;
        let progress = crate::progress::throttled(&app, "style-progress");
        let total = photos.len();
        let mut batch = StyleBatch {
            report: Default::default(),
            state: None,
        };
        let open = {
            let guard = slot.lock()?;
            guard
                .as_ref()
                .and_then(|s| photos.iter().find(|p| Path::new(p) == s.path).cloned())
        };
        if let Some(open) = &open {
            let mut guard = slot.lock()?;
            if let Some(s) = guard.as_mut().filter(|s| s.path == Path::new(open)) {
                let lens = lens.get().unwrap_or_else(|| empty_lens());
                let result = solve_open(s, &style, lens);
                let cmds = styles::commands(s.state.settings(), &result, amount);
                if cmds.is_empty() {
                    batch.report.unchanged.push(PathBuf::from(open));
                } else {
                    s.execute(&cmds, Some(&format!("Style: {}", style.name)))?;
                    s.save_sidecar()?;
                    batch.state = Some(s.state_info());
                    batch.report.changed.push(PathBuf::from(open));
                }
            }
            progress(1, total);
        }
        let rest: Vec<PathBuf> = photos
            .iter()
            .filter(|p| open.as_ref() != Some(*p))
            .map(PathBuf::from)
            .collect();
        let done = total - rest.len();
        let r = amb_ops::style::apply(
            &amb_ops::style::Options {
                photos: &rest,
                style: &style,
                amount,
            },
            &|d, t| progress(done + d, done + t),
        );
        batch.report.changed.extend(r.changed);
        batch.report.unchanged.extend(r.unchanged);
        batch.report.errors.extend(r.errors);
        Ok(batch)
    })
    .await?
}
