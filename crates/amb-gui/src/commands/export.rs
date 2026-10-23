// SPDX-License-Identifier: GPL-3.0-or-later
//! Export, export presets, borders and captions, and Edit in another app.

use crate::*;

/// The fonts for text on exports: id and name.
#[tauri::command]
pub(crate) fn fonts() -> Vec<(&'static str, &'static str)> {
    amb_export::fonts::FONTS
        .iter()
        .map(|(id, name, _)| (*id, *name))
        .collect()
}

/// A font file, so the editor's previews use the same fonts as exports.
#[tauri::command]
pub(crate) fn font_data(id: String) -> Response {
    Response::new(amb_export::fonts::bytes(&id).to_vec())
}

/// A border caption as the open photo's export would set it, and the tokens.
#[tauri::command]
pub(crate) fn caption_preview(
    template: String,
    state: tauri::State<'_, AppState>,
) -> Res<(String, Vec<(String, String)>)> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let text = guard
        .as_ref()
        .map(|s| s.caption_text(&template))
        .unwrap_or_default();
    let tokens = amb_export::border::CAPTION_TOKENS
        .iter()
        .map(|(t, d)| (t.to_string(), d.to_string()))
        .collect();
    Ok((text, tokens))
}

#[derive(Deserialize)]
pub(crate) struct ExportOptions {
    pub(crate) format: String,
    pub(crate) quality: u8,
    pub(crate) bits: Option<u32>,
    pub(crate) color_space: String,
    #[serde(default)]
    pub(crate) long_edge: Option<u32>,
    /// Resize to a long or short edge, a width or height (px), megapixels
    /// or a percent; wins over `long_edge`.
    #[serde(default)]
    pub(crate) resize: Option<ResizeChoice>,
    /// Exact output size (a size preset); wins over the others.
    pub(crate) exact: Option<(u32, u32)>,
    pub(crate) sharpen: Option<String>,
    pub(crate) metadata: String,
    pub(crate) max_kb: Option<u64>,
    /// A frame around the photo.
    #[serde(default)]
    pub(crate) border: Option<amb_export::border::Border>,
    /// Put the photo's rating and keywords in the file's metadata.
    #[serde(default = "yes")]
    pub(crate) include_tags: bool,
    /// File names for a batch, as Import's templates (`{name}`, `{seq}`, ...).
    #[serde(default)]
    pub(crate) name_template: Option<String>,
    /// Text or a logo on the photo.
    #[serde(default)]
    pub(crate) watermark: Option<amb_export::watermark::Watermark>,
    /// JPEG chroma subsampling ("444", "422", "420"); none for automatic.
    #[serde(default)]
    pub(crate) subsampling: Option<String>,
    #[serde(default)]
    pub(crate) progressive: bool,
    /// AVIF encoder speed, 1 (slow, small) to 10.
    #[serde(default)]
    pub(crate) avif_speed: Option<u8>,
}

pub(crate) fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResizeChoice {
    /// long, short, width, height, mp or percent.
    pub(crate) mode: String,
    pub(crate) value: f64,
}

impl ResizeChoice {
    fn resize(&self) -> Res<amb_export::Resize> {
        use amb_export::Resize;
        let v = self.value;
        if !(v.is_finite() && v > 0.0) {
            return Err(format!("resize: {v} isn't a size"));
        }
        let px = v.round().clamp(1.0, 65_535.0) as u32;
        Ok(match self.mode.as_str() {
            "long" => Resize::LongEdge(px),
            "short" => Resize::ShortEdge(px),
            "width" => Resize::Width(px),
            "height" => Resize::Height(px),
            "mp" => Resize::Megapixels(v),
            "percent" => Resize::Percent(v),
            other => return Err(format!("unknown resize {other:?}")),
        })
    }
}

/// Export choices saved under a name (the dialog's fields, as it shows them).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExportPreset {
    pub(crate) name: String,
    pub(crate) fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize)]
pub(crate) struct Exported {
    pub(crate) path: String,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) bytes: u64,
    pub(crate) quality: u8,
}

#[tauri::command]
pub(crate) async fn pick_export_path(
    app: tauri::AppHandle,
    suggested: String,
    format: String,
    dir: Option<String>,
) -> Option<String> {
    let ext = amb_export::Format::from_name(&format)?.extension();
    // A UI test saves without asking.
    if let Some(t) = crate::uitest::current() {
        return Some(
            t.out_dir
                .join(format!("{suggested}.{ext}"))
                .display()
                .to_string(),
        );
    }
    let mut dialog = app.dialog().file();
    // Next to the photo: an app opened from the Finder starts in `/`.
    if let Some(d) = dir.filter(|d| Path::new(d).is_dir()) {
        dialog = dialog.set_directory(d);
    }
    dialog
        .set_title("Export")
        .set_file_name(format!("{suggested}.{ext}"))
        .add_filter(format.to_uppercase(), &[ext])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

#[tauri::command]
pub(crate) async fn export(
    path: String,
    options: ExportOptions,
    state: tauri::State<'_, AppState>,
) -> Res<Exported> {
    let slot = state.session.clone();
    let lens = state.lens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = export_settings(&options)?;
        if options.include_tags {
            if let Some(info) = slot
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|s| s.info.clone()))
            {
                s.tags = amb_export::exif::Tags {
                    place: info.place(),
                    rating: info.rating,
                    keywords: info.keywords,
                    label: info.label,
                    title: info.title,
                    caption: info.caption,
                    creator: info.creator,
                    copyright: info.copyright,
                };
            }
        }
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("no photo open")?;
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        let r = session.export(Path::new(&path), &s, lens)?;
        Ok(Exported {
            path,
            width: r.width,
            height: r.height,
            bytes: r.bytes,
            quality: r.quality,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The export dialog's choices as export settings (without the photo's tags).
pub(crate) fn export_settings(options: &ExportOptions) -> Res<amb_export::ExportSettings> {
    use amb_export::{BitDepth, ExportSettings, Format, MetadataPolicy, Resize};
    {
        let format = Format::from_name(&options.format)
            .ok_or(format!("unknown format {}", options.format))?;
        let mut s = ExportSettings::new(format);
        s.quality = options.quality.clamp(1, 100);
        s.bit_depth = options.bits.and_then(BitDepth::from_bits);
        s.color_space = amb_color::space::OutputSpace::from_id(&options.color_space)
            .ok_or(format!("unknown colour space {}", options.color_space))?;
        if let Some((w, h)) = options.exact.filter(|(w, h)| *w > 0 && *h > 0) {
            s.resize = Resize::Exact(w, h);
        } else if let Some(r) = &options.resize {
            s.resize = r.resize()?;
        } else if let Some(edge) = options.long_edge.filter(|e| *e > 0) {
            s.resize = Resize::LongEdge(edge);
        }
        s.sharpen = options
            .sharpen
            .as_deref()
            .and_then(amb_export::parse_sharpen);
        s.metadata =
            MetadataPolicy::from_id(&options.metadata).unwrap_or(MetadataPolicy::AllExceptLocation);
        if let Some(b) = &options.border {
            b.validate()?;
            s.border = Some(b.clone());
        }
        if let Some(w) = &options.watermark {
            w.validate()?;
            s.watermark = Some(w.clone());
        }
        s.subsampling = match options.subsampling.as_deref() {
            None | Some("") => None,
            Some("444") => Some(amb_export::Subsampling::S444),
            Some("422") => Some(amb_export::Subsampling::S422),
            Some("420") => Some(amb_export::Subsampling::S420),
            Some(other) => return Err(format!("unknown subsampling {other:?}")),
        };
        s.progressive = options.progressive;
        if let Some(speed) = options.avif_speed {
            s.avif_speed = speed.clamp(1, 10);
        }
        s.max_bytes = options
            .max_kb
            .filter(|k| *k > 0 && format == Format::Jpeg)
            .map(|k| k * 1000);
        Ok(s)
    }
}

/// The user's export presets, by name.
#[tauri::command]
pub(crate) fn export_presets(state: tauri::State<'_, AppState>) -> Res<Vec<ExportPreset>> {
    Ok(state
        .export_presets
        .lock()
        .map_err(|e| e.to_string())?
        .clone())
}

/// Save (or replace) an export preset.
#[tauri::command]
pub(crate) fn save_export_preset(
    name: String,
    fields: serde_json::Map<String, serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the preset a name".into());
    }
    {
        let mut list = state.export_presets.lock().map_err(|e| e.to_string())?;
        list.retain(|p| p.name != name);
        list.push(ExportPreset { name, fields });
        list.sort_by_key(|p| p.name.to_lowercase());
    }
    save_config(&state);
    Ok(())
}

#[tauri::command]
pub(crate) fn delete_export_preset(name: String, state: tauri::State<'_, AppState>) -> Res<()> {
    state
        .export_presets
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|p| p.name != name);
    save_config(&state);
    Ok(())
}

/// Built-in border presets followed by the user's.
#[tauri::command]
pub(crate) fn border_presets(state: tauri::State<'_, AppState>) -> Res<Vec<borders::BorderPreset>> {
    let custom = state.border_presets.lock().map_err(|e| e.to_string())?;
    Ok(borders::all(&custom))
}

/// Add or update a border preset of the user's; returns the list and the saved id.
#[tauri::command]
pub(crate) fn save_border_preset(
    preset: borders::BorderPreset,
    state: tauri::State<'_, AppState>,
) -> Res<(Vec<borders::BorderPreset>, String)> {
    let id = {
        let mut custom = state.border_presets.lock().map_err(|e| e.to_string())?;
        borders::upsert(&mut custom, &preset)?
    };
    save_config(state.inner());
    Ok((border_presets(state)?, id))
}

#[tauri::command]
pub(crate) fn delete_border_preset(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Res<Vec<borders::BorderPreset>> {
    {
        let mut custom = state.border_presets.lock().map_err(|e| e.to_string())?;
        borders::remove(&mut custom, &id)?;
    }
    save_config(state.inner());
    border_presets(state)
}

/// Save a border preset as a file to share (`ambrotype-border/1`).
#[tauri::command]
pub(crate) async fn share_border_preset(
    app: tauri::AppHandle,
    border: amb_export::border::Border,
) -> Res<Option<String>> {
    border.validate()?;
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Share this border")
        .set_file_name(format!("{}.json", border.name))
        .add_filter("Borders", &["json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = serde_json::to_string_pretty(&border).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Read a shared border file; the dialog then saves it as one of the user's.
#[tauri::command]
pub(crate) async fn import_border_preset(
    app: tauri::AppHandle,
) -> Res<Option<amb_export::border::Border>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Open a border")
        .add_filter("Borders", &["json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    amb_export::border::Border::from_json(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// A PNG logo for the export watermark.
#[tauri::command]
pub(crate) async fn pick_watermark_image(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose a logo")
        .add_filter("PNG", &["png", "PNG"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// The Edit in settings.
#[tauri::command]
pub(crate) fn edit_in_settings(state: tauri::State<'_, AppState>) -> Res<EditIn> {
    Ok(state.edit_in.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub(crate) fn set_edit_in(settings: EditIn, state: tauri::State<'_, AppState>) -> Res<EditIn> {
    amb_color::space::OutputSpace::from_id(&settings.color_space)
        .ok_or(format!("unknown colour space {}", settings.color_space))?;
    if !matches!(settings.bits, 8 | 16) {
        return Err("8 or 16 bits".into());
    }
    let settings = EditIn {
        app: settings
            .app
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty()),
        ..settings
    };
    *state.edit_in.lock().map_err(|e| e.to_string())? = settings.clone();
    save_config(state.inner());
    Ok(settings)
}

/// Choose the app to edit in.
#[tauri::command]
pub(crate) async fn pick_edit_app(app: tauri::AppHandle) -> Option<String> {
    let mut dialog = app.dialog().file().set_title("Edit in: choose the app");
    if cfg!(target_os = "macos") {
        dialog = dialog.set_directory("/Applications");
    }
    dialog
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string())
}

/// After an export, as Lightroom's Post-Processing: show the files in their
/// folder, or open them in the app for Edit in (the system's default when
/// none is set).
#[tauri::command]
pub(crate) fn after_export(
    paths: Vec<String>,
    action: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<()> {
    if paths.is_empty() {
        return Ok(());
    }
    match action.as_str() {
        "" => Ok(()),
        "reveal" => app
            .opener()
            .reveal_items_in_dir(&paths)
            .map_err(|e| e.to_string()),
        "open" => {
            let with = state.edit_in.lock().ok().and_then(|e| e.app.clone());
            // A batch could start hundreds of windows: the first few.
            for p in paths.iter().take(12) {
                app.opener()
                    .open_path(p, with.clone())
                    .map_err(|e| format!("{p}: {e}"))?;
            }
            Ok(())
        }
        other => Err(format!("unknown after-export action {other:?}")),
    }
}

/// Edit the open photo in another app: write it, as edited, to a TIFF next
/// to it (`IMG_0001-Edit.tif`, then `-Edit-2`, …) and open that there.
/// The TIFF then shows in the folder like any photo.
#[tauri::command]
pub(crate) async fn edit_in(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Res<String> {
    let settings = state.edit_in.lock().map_err(|e| e.to_string())?.clone();
    let slot = state.session.clone();
    let lens = state.lens.clone();
    let path = tauri::async_runtime::spawn_blocking(move || -> Res<PathBuf> {
        use amb_export::{BitDepth, ExportSettings, Format, MetadataPolicy};
        let mut s = ExportSettings::new(Format::Tiff);
        s.bit_depth = BitDepth::from_bits(settings.bits);
        s.color_space = amb_color::space::OutputSpace::from_id(&settings.color_space)
            .unwrap_or(amb_color::space::OutputSpace::ProPhoto);
        s.metadata = MetadataPolicy::All;
        let mut guard = slot.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("no photo open")?;
        let i = &session.info;
        s.tags = amb_export::exif::Tags {
            rating: i.rating,
            keywords: i.keywords.clone(),
            label: i.label.clone(),
            title: i.title.clone(),
            caption: i.caption.clone(),
            creator: i.creator.clone(),
            copyright: i.copyright.clone(),
            place: i.place(),
        };
        let out = session::edit_copy_path(&session.path);
        let lens = lens.get().unwrap_or_else(|| empty_lens());
        session.export(&out, &s, lens)?;
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())??;
    let with = state.edit_in.lock().ok().and_then(|e| e.app.clone());
    app.opener()
        .open_path(path.display().to_string(), with)
        .map_err(|e| format!("wrote {}, but couldn't open it: {e}", path.display()))?;
    Ok(path.display().to_string())
}
