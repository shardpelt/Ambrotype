// SPDX-License-Identifier: GPL-3.0-or-later
//! Setups: a layout and theme kept together to come back to.

use super::*;

pub const SETUP_FORMAT: &str = "ambrotype-setup/1";

/// A whole appearance kept to come back to: copies of the layout and theme
/// as they were (not just their ids), so changing or deleting those later
/// doesn't change the setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub format: String,
    pub name: String,
    /// When it was kept (seconds since 1970).
    pub saved: u64,
    pub layout: SetupPart,
    pub theme: SetupPart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupPart {
    /// The look it was (`builtin:…` or `user:…`).
    pub id: String,
    pub data: Value,
}

fn setup_path(dir: &Path, name: &str) -> PathBuf {
    dir.join("setups").join(format!("{}.json", slug(name)))
}

/// The kept setups, newest first.
pub fn setups(dir: &Path) -> Vec<Setup> {
    let mut out: Vec<Setup> = std::fs::read_dir(dir.join("setups"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str::<Setup>(&t).ok())
        .filter(|s| s.format == SETUP_FORMAT)
        .collect();
    out.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Keep the layout `layout` and theme `theme` as they are now, as `name`
/// (replacing a setup of that name).
pub fn save_setup(dir: &Path, name: &str, layout: &str, theme: &str) -> Result<Setup, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the setup a name".into());
    }
    let part = |kind: Kind, id: &str| -> Result<SetupPart, String> {
        let look = all(Some(dir), kind)
            .into_iter()
            .find(|l| l.id == id && l.usable)
            .ok_or_else(|| format!("{id} isn't there to keep"))?;
        Ok(SetupPart {
            id: look.id,
            data: look.data,
        })
    };
    let setup = Setup {
        format: SETUP_FORMAT.into(),
        name: name.into(),
        saved: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        layout: part(Kind::Layout, layout)?,
        theme: part(Kind::Theme, theme)?,
    };
    let path = setup_path(dir, name);
    std::fs::create_dir_all(dir.join("setups")).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&setup).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(setup)
}

/// The layout and theme ids that bring `name` back: the same looks when
/// they haven't changed, otherwise the kept copies, saved again as looks of
/// the user's.
pub fn restore_setup(dir: &Path, name: &str) -> Result<(String, String), String> {
    let setup = setups(dir)
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("no setup {name:?}"))?;
    let bring_back = |kind: Kind, part: &SetupPart| -> Result<String, String> {
        let looks = all(Some(dir), kind);
        // The same look, unchanged.
        if looks.iter().any(|l| l.id == part.id && l.data == part.data) {
            return Ok(part.id.clone());
        }
        // Another look that is exactly this one (restored before).
        if let Some(l) = looks.iter().find(|l| l.data == part.data) {
            return Ok(l.id.clone());
        }
        save(dir, kind, None, part.data.clone())
    };
    Ok((
        bring_back(Kind::Layout, &setup.layout)?,
        bring_back(Kind::Theme, &setup.theme)?,
    ))
}

pub fn delete_setup(dir: &Path, name: &str) -> Result<(), String> {
    let path = setup_path(dir, name);
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}
