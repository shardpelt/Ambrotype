// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings kept between runs, and how they are read and saved.

use crate::*;

/// The saved settings at `path`, or the defaults.
pub(crate) fn read_config(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Puts the saved settings into the app's state, and gives back the
/// custom interface's folder, which `main` opens.
pub(crate) fn apply_config(state: &AppState, config: Config) -> Option<PathBuf> {
    if let Ok(mut dirs) = state.preset_dirs.lock() {
        for d in config.preset_dirs {
            if d.is_dir() && !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    }
    if let Ok(mut p) = state.size_presets.lock() {
        *p = config.size_presets;
    }
    if let Ok(mut l) = state.look.lock() {
        *l = (config.layout, config.theme);
    }
    if let Ok(mut k) = state.keybindings.lock() {
        *k = keys::validate(&config.keybindings).unwrap_or_default();
    }
    if let Ok(mut h) = state.hub_url.lock() {
        *h = config.hub_url;
    }
    if let Ok(mut b) = state.border_presets.lock() {
        *b = config.border_presets;
    }
    if let Ok(mut k) = state.recent_keywords.lock() {
        *k = config.recent_keywords;
    }
    if let Ok(mut t) = state.collage_templates.lock() {
        *t = config.collage_templates;
    }
    if let Ok(mut a) = state.plugin_approvals.lock() {
        *a = config.plugin_approvals;
    }
    if let Ok(mut r) = state.plugin_recipes.lock() {
        *r = config.plugin_recipes;
    }
    if let Ok(mut c) = state.import_choice.lock() {
        *c = config.import;
    }
    if let Ok(mut a) = state.assistant.lock() {
        *a = config.assistant;
    }
    if let Ok(mut p) = state.export_presets.lock() {
        *p = config.export_presets;
    }
    if let Ok(mut p) = state.preset_favourites.lock() {
        *p = config.preset_favourites;
    }
    if let Ok(mut p) = state.mask_effects.lock() {
        *p = config.mask_effects;
    }
    if let Ok(mut p) = state.edit_in.lock() {
        *p = config.edit_in;
    }
    if let Ok(mut m) = state.monitor.lock() {
        *m = config.monitor;
    }
    if let Ok(mut m) = state.metadata_templates.lock() {
        *m = config.metadata_templates;
    }
    config.interface
}

/// The import dialog's last destination and choices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImportChoice {
    pub(crate) dest: String,
    pub(crate) options: amb_library::import::ImportOptions,
}

/// Settings kept between runs.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    preset_dirs: Vec<PathBuf>,
    #[serde(default)]
    size_presets: Vec<SizePreset>,
    /// The active layout and theme (look ids).
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    /// Keyboard shortcuts the user changed (action id -> keys).
    #[serde(default)]
    keybindings: keys::Bindings,
    /// Another hub than the default (an index.json address).
    #[serde(default)]
    hub_url: Option<String>,
    /// The user's border presets (the built-ins are added when listing).
    #[serde(default)]
    border_presets: Vec<borders::BorderPreset>,
    /// Keywords used lately, newest first (for suggestions).
    #[serde(default)]
    recent_keywords: Vec<String>,
    /// The user's collage templates (the built-ins are added when listing).
    #[serde(default)]
    collage_templates: Vec<collages::TemplateEntry>,
    /// Plugins whose permissions the user allowed: id -> the fingerprint allowed.
    #[serde(default)]
    plugin_approvals: plugins::Approvals,
    /// Plugin recipes the user saved (plugin id -> recipes).
    #[serde(default)]
    plugin_recipes: PluginRecipes,
    /// The import dialog's last destination and choices.
    #[serde(default)]
    import: Option<ImportChoice>,
    /// The assistant's provider and model (its key is in the keychain).
    #[serde(default)]
    assistant: assistant::Provider,
    #[serde(default)]
    export_presets: Vec<ExportPreset>,
    #[serde(default)]
    preset_favourites: Vec<String>,
    /// The user's own mask effects (the built-ins are added when listing).
    #[serde(default)]
    mask_effects: Vec<MaskEffect>,
    /// Edit in another app: which app and what file.
    #[serde(default)]
    edit_in: EditIn,
    #[serde(default)]
    metadata_templates: Vec<commands::files::MetadataTemplate>,
    /// Which monitor profile the preview is shown in.
    #[serde(default)]
    monitor: monitor::Choice,
    /// A custom interface folder shown instead of the built-in one.
    #[serde(default)]
    interface: Option<PathBuf>,
}

/// Lightroom's "Edit in": the photo, as edited, goes to another app (an
/// image editor) as a TIFF next to it, `IMG_0001-Edit.tif`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditIn {
    /// The app (a program, or a `.app` on macOS); none: the system's
    /// default for TIFFs.
    #[serde(default)]
    pub(crate) app: Option<String>,
    #[serde(default = "prophoto")]
    pub(crate) color_space: String,
    #[serde(default = "sixteen")]
    pub(crate) bits: u32,
}

fn prophoto() -> String {
    "prophoto".into()
}

fn sixteen() -> u32 {
    16
}

impl Default for EditIn {
    fn default() -> EditIn {
        EditIn {
            app: None,
            color_space: prophoto(),
            bits: sixteen(),
        }
    }
}

/// A mask's sliders saved by name (the Masks panel's Effect menu), in the
/// panel's units: Exposure in stops, the rest -100..100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MaskEffect {
    pub(crate) name: String,
    pub(crate) values: BTreeMap<String, f64>,
}

pub(crate) const BUILTIN_MASK_EFFECTS: &str = include_str!("../data/mask-effects.json");

pub(crate) fn save_config(state: &AppState) {
    let Ok(path) = state.config_path.lock() else {
        return;
    };
    let Some(path) = path.as_ref() else {
        return;
    };
    let dirs = state
        .preset_dirs
        .lock()
        .map(|d| d.clone())
        .unwrap_or_default();
    // Lightroom's own folders are found automatically; store only added ones.
    let defaults = session::default_preset_dirs();
    let config = Config {
        preset_dirs: dirs.into_iter().filter(|d| !defaults.contains(d)).collect(),
        size_presets: state
            .size_presets
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        layout: state.look.lock().ok().and_then(|l| l.0.clone()),
        theme: state.look.lock().ok().and_then(|l| l.1.clone()),
        keybindings: state
            .keybindings
            .lock()
            .map(|k| k.clone())
            .unwrap_or_default(),
        hub_url: state.hub_url.lock().ok().and_then(|h| h.clone()),
        border_presets: state
            .border_presets
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default(),
        recent_keywords: state
            .recent_keywords
            .lock()
            .map(|k| k.clone())
            .unwrap_or_default(),
        collage_templates: state
            .collage_templates
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default(),
        plugin_approvals: state
            .plugin_approvals
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default(),
        plugin_recipes: state
            .plugin_recipes
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default(),
        import: state.import_choice.lock().ok().and_then(|c| c.clone()),
        assistant: state
            .assistant
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default(),
        export_presets: state
            .export_presets
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        mask_effects: state
            .mask_effects
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        preset_favourites: state
            .preset_favourites
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default(),
        edit_in: state.edit_in.lock().map(|e| e.clone()).unwrap_or_default(),
        monitor: state.monitor.lock().map(|m| m.clone()).unwrap_or_default(),
        metadata_templates: state
            .metadata_templates
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default(),
        interface: state.interface.lock().ok().and_then(|i| i.clone()),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&config) {
        let _ = std::fs::write(path, text);
    }
}
