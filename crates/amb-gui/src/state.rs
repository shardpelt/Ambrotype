// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's state, shared by the commands.

use crate::*;

#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) session: Arc<Mutex<Option<Session>>>,
    pub(crate) lens: Arc<LensCell>,
    /// The GPU for previews, once set up (`None` inside: there is none).
    pub(crate) gpu: Arc<OnceLock<Option<&'static amb_gpu::Gpu>>>,
    pub(crate) preset_dirs: Arc<Mutex<Vec<PathBuf>>>,
    /// The user's own size presets (the built-ins are added when listing).
    pub(crate) size_presets: Mutex<Vec<SizePreset>>,
    /// A photo given on the command line, handed to the UI once it asks.
    pub(crate) initial_path: Mutex<Option<String>>,
    /// Where `Config` is saved (the app's config folder), once known.
    pub(crate) config_path: Mutex<Option<PathBuf>>,
    /// The active layout and theme ids, as chosen in the Looks dialog.
    pub(crate) look: Mutex<(Option<String>, Option<String>)>,
    pub(crate) keybindings: Mutex<keys::Bindings>,
    pub(crate) hub_url: Mutex<Option<String>>,
    pub(crate) border_presets: Mutex<Vec<borders::BorderPreset>>,
    pub(crate) recent_keywords: Mutex<Vec<String>>,
    pub(crate) collage_templates: Mutex<Vec<collages::TemplateEntry>>,
    pub(crate) plugin_approvals: Mutex<plugins::Approvals>,
    pub(crate) plugin_recipes: Mutex<PluginRecipes>,
    pub(crate) import_choice: Mutex<Option<ImportChoice>>,
    pub(crate) export_presets: Mutex<Vec<ExportPreset>>,
    /// The presets starred as favourites, by path.
    pub(crate) preset_favourites: Mutex<Vec<String>>,
    pub(crate) mask_effects: Mutex<Vec<MaskEffect>>,
    pub(crate) edit_in: Mutex<EditIn>,
    /// Metadata templates (library → Organise).
    pub(crate) metadata_templates: Mutex<Vec<commands::files::MetadataTemplate>>,
    /// The monitor profile choice (Settings).
    pub(crate) monitor: Mutex<monitor::Choice>,
    /// Soft proofing (for this run): the space, and the gamut warning.
    pub(crate) proof: Mutex<Option<(amb_color::space::OutputSpace, bool)>>,
    /// Watches the folders on screen for photos added, removed or renamed.
    pub(crate) folder_watch: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the open photo's sidecar for changes made outside the editor.
    pub(crate) sidecar_watch: Mutex<Option<notify::RecommendedWatcher>>,
    pub(crate) assistant: Mutex<assistant::Provider>,
    /// The assistant's key for this run, when the keychain can't keep it.
    pub(crate) assistant_key: Mutex<Option<String>>,
    /// The custom interface chosen in Settings (a folder), if any.
    pub(crate) interface: Mutex<Option<PathBuf>>,
    /// The menu bar's items by action id, to grey them out.
    pub(crate) menu_items:
        Mutex<std::collections::HashMap<String, tauri::menu::MenuItem<tauri::Wry>>>,
}

/// The lens database and profiles, loaded in the background at start and
/// loaded again after `amb lens fetch` (Download lens database). A
/// replaced one stays in memory until the app closes, so a render that
/// started with it can finish; that happens once per download.
#[derive(Default)]
pub(crate) struct LensCell(Mutex<Option<&'static LensSources>>);

impl LensCell {
    pub(crate) fn get(&self) -> Option<&'static LensSources> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn set(&self, sources: LensSources) -> Result<(), ()> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::leak(Box::new(sources)));
        Ok(())
    }
}

pub(crate) fn empty_lens() -> &'static LensSources {
    static EMPTY: OnceLock<LensSources> = OnceLock::new();
    EMPTY.get_or_init(LensSources::default)
}

/// Collages read each photo's edits from its sidecar: write the open photo's
/// unsaved edits there first.
pub(crate) fn save_open_edits(slot: &Mutex<Option<Session>>) -> Res<()> {
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    if let Some(s) = guard.as_mut().filter(|s| s.has_unsaved_edits()) {
        s.save_sidecar()?;
    }
    Ok(())
}
