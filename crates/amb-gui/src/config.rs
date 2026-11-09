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
pub(crate) fn apply_config(state: &AppState, mut config: Config) -> Option<PathBuf> {
    if let Ok(mut dirs) = state.preset_dirs.lock() {
        for d in &config.preset_dirs {
            if d.is_dir() && !dirs.contains(d) {
                dirs.push(d.clone());
            }
        }
    }
    config.keybindings = keys::validate(&config.keybindings).unwrap_or_default();
    config.interface = config.interface.filter(|d| interface::is_interface(d));
    let interface = config.interface.clone();
    *state.prefs.lock().expect("fresh mutex") = config;
    interface
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
    pub(crate) preset_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) size_presets: Vec<SizePreset>,
    /// The active layout and theme (look ids).
    #[serde(default)]
    pub(crate) layout: Option<String>,
    #[serde(default)]
    pub(crate) theme: Option<String>,
    /// Keyboard shortcuts the user changed (action id -> keys).
    #[serde(default)]
    pub(crate) keybindings: keys::Bindings,
    /// Another hub than the default (an index.json address).
    #[serde(default)]
    pub(crate) hub_url: Option<String>,
    /// The user's border presets (the built-ins are added when listing).
    #[serde(default)]
    pub(crate) border_presets: Vec<borders::BorderPreset>,
    /// Keywords used lately, newest first (for suggestions).
    #[serde(default)]
    pub(crate) recent_keywords: Vec<String>,
    /// The user's collage templates (the built-ins are added when listing).
    #[serde(default)]
    pub(crate) collage_templates: Vec<collages::TemplateEntry>,
    /// Plugins whose permissions the user allowed: id -> the fingerprint allowed.
    #[serde(default)]
    pub(crate) plugin_approvals: plugins::Approvals,
    /// Plugin recipes the user saved (plugin id -> recipes).
    #[serde(default)]
    pub(crate) plugin_recipes: PluginRecipes,
    /// The import dialog's last destination and choices.
    #[serde(default)]
    pub(crate) import: Option<ImportChoice>,
    /// The assistant's provider and model (its key is in the keychain).
    #[serde(default)]
    pub(crate) assistant: assistant::Provider,
    #[serde(default)]
    pub(crate) export_presets: Vec<ExportPreset>,
    #[serde(default)]
    pub(crate) preset_favourites: Vec<String>,
    /// The user's own mask effects (the built-ins are added when listing).
    #[serde(default)]
    pub(crate) mask_effects: Vec<MaskEffect>,
    /// Edit in another app: which app and what file.
    #[serde(default)]
    pub(crate) edit_in: EditIn,
    #[serde(default)]
    pub(crate) metadata_templates: Vec<commands::files::MetadataTemplate>,
    /// Which monitor profile the preview is shown in.
    #[serde(default)]
    pub(crate) monitor: monitor::Choice,
    /// A custom interface folder shown instead of the built-in one.
    #[serde(default)]
    pub(crate) interface: Option<PathBuf>,
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
    let defaults = preset_files::default_preset_dirs();
    let dirs: Vec<PathBuf> = dirs.into_iter().filter(|d| !defaults.contains(d)).collect();
    let text = {
        let Ok(mut prefs) = state.prefs.lock() else {
            return;
        };
        prefs.preset_dirs = dirs;
        serde_json::to_string_pretty(&*prefs)
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = text {
        let _ = std::fs::write(path, text);
    }
}
