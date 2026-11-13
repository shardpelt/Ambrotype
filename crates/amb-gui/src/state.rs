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
    /// The saved settings (settings.json), behind one lock: read them with [`AppState::prefs`], change them with [`AppState::change_prefs`].
    pub(crate) prefs: Mutex<Config>,
    /// A photo given on the command line, handed to the UI once it asks.
    pub(crate) initial_path: Mutex<Option<String>>,
    /// Where `Config` is saved (the app's config folder), once known.
    pub(crate) config_path: Mutex<Option<PathBuf>>,
    /// Soft proofing (for this run): the space, and the gamut warning.
    pub(crate) proof: Mutex<Option<(amb_color::space::OutputSpace, bool)>>,
    /// Watches the folders on screen for photos added, removed or renamed.
    pub(crate) folder_watch: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the open photo's sidecar for changes made outside the editor.
    pub(crate) sidecar_watch: Mutex<Option<notify::RecommendedWatcher>>,
    /// The assistant's key for this run, when the keychain can't keep it.
    pub(crate) assistant_key: Mutex<Option<String>>,
    /// The menu bar's items by action id, to grey them out.
    pub(crate) menu_items:
        Mutex<std::collections::HashMap<String, tauri::menu::MenuItem<tauri::Wry>>>,
}

impl AppState {
    /// Something read from the saved settings.
    pub(crate) fn prefs<T>(&self, read: impl FnOnce(&Config) -> T) -> Res<T> {
        let prefs = self.prefs.lock()?;
        Ok(read(&prefs))
    }

    /// Changes the saved settings, then writes them to the settings file.
    /// Inside `change`, only read and change fields: calling anything that
    /// takes the state (`save_config`, `prefs`, `change_prefs`, a command)
    /// would wait for this same lock forever.
    pub(crate) fn change_prefs<T>(&self, change: impl FnOnce(&mut Config) -> T) -> Res<T> {
        let out = {
            let mut prefs = self.prefs.lock()?;
            change(&mut prefs)
        };
        save_config(self);
        Ok(out)
    }
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
    let mut guard = slot.lock()?;
    if let Some(s) = guard.as_mut().filter(|s| s.has_unsaved_edits()) {
        s.save_sidecar()?;
    }
    Ok(())
}
