// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu bar. The UI sends its menus (`ui/menu.js`: the keyboard actions
//! by name, with their keys), so changed shortcuts show in them; choosing an
//! item sends its action id back as a `menu` event (`main`). On macOS it is
//! the system's menu bar; Windows and Linux show it in the window when the
//! user asks for it (Settings → General) or with `AMB_MENU_BAR=1`.

use std::collections::HashMap;

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Wry};

use crate::*;

/// One entry of a menu, as the UI describes it.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Entry {
    /// One of the app's actions: `id` comes back in the `menu` event. `keys`
    /// ("CmdOrCtrl+S") is shown, and works while the menu bar is there.
    Item {
        id: String,
        text: String,
        #[serde(default)]
        keys: Option<String>,
        #[serde(default = "yes")]
        enabled: bool,
    },
    Separator,
    /// `role` "window" or "help": macOS adds its window list or search there.
    Submenu {
        text: String,
        #[serde(default)]
        role: Option<String>,
        items: Vec<Entry>,
    },
    /// One the system provides and runs itself: "copy", "quit" and so on.
    Native {
        name: String,
    },
}

fn yes() -> bool {
    true
}

/// Whether the menu bar is shown: always on macOS (the system's bar at
/// the top of the screen), elsewhere when `wanted`.
fn shown(wanted: bool) -> bool {
    cfg!(target_os = "macos") || wanted || std::env::var("AMB_MENU_BAR").is_ok_and(|v| v == "1")
}

/// Puts up the menu bar the UI describes, or takes it away; false where
/// none is shown.
#[tauri::command]
pub(crate) fn set_menu(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    menus: Vec<Entry>,
    wanted: bool,
) -> Res<bool> {
    if !shown(wanted) {
        app.remove_menu().map_err(|e| e.to_string())?;
        if let Ok(mut m) = state.menu_items.lock() {
            m.clear();
        }
        return Ok(false);
    }
    let mut items = HashMap::new();
    let menu = Menu::new(&app).map_err(|e| e.to_string())?;
    for entry in &menus {
        menu.append(&build(&app, entry, &mut items)?)
            .map_err(|e| e.to_string())?;
    }
    app.set_menu(menu).map_err(|e| e.to_string())?;
    if let Ok(mut m) = state.menu_items.lock() {
        *m = items;
    }
    Ok(true)
}

/// The menu bar under a custom interface: the app's actions are the built-in
/// interface's, so it has only a way back to that (`main` handles
/// "builtin"), and the system's own items. Windows and Linux get it only
/// when a menu bar was up already.
pub(crate) fn interface_menu(app: &AppHandle) -> Res<()> {
    if !cfg!(target_os = "macos") && app.menu().is_none() {
        return Ok(());
    }
    let back = Entry::Item {
        id: "builtin".into(),
        text: "Back to the Built-in Interface".into(),
        keys: None,
        enabled: true,
    };
    let native = |n: &str| Entry::Native { name: n.into() };
    let mut menus = Vec::new();
    if cfg!(target_os = "macos") {
        menus.push(Entry::Submenu {
            text: "Ambrotype".into(),
            role: None,
            items: vec![
                native("about"),
                Entry::Separator,
                native("hide"),
                native("quit"),
            ],
        });
        menus.push(Entry::Submenu {
            text: "Edit".into(),
            role: None,
            items: ["cut", "copy", "paste"].map(native).into(),
        });
    }
    menus.push(Entry::Submenu {
        text: "Interface".into(),
        role: None,
        items: vec![back],
    });
    let mut items = HashMap::new();
    let menu = Menu::new(app).map_err(|e| e.to_string())?;
    for entry in &menus {
        menu.append(&build(app, entry, &mut items)?)
            .map_err(|e| e.to_string())?;
    }
    app.set_menu(menu).map_err(|e| e.to_string())?;
    if let Ok(mut m) = app.state::<AppState>().menu_items.lock() {
        *m = items;
    }
    Ok(())
}

/// Greys out (or back) the items whose action can't run now: no photo open,
/// or another tool's keys.
#[tauri::command]
pub(crate) fn menu_enable(
    state: tauri::State<'_, AppState>,
    enabled: HashMap<String, bool>,
) -> Res<()> {
    let items = state.menu_items.lock().map_err(|e| e.to_string())?;
    for (id, on) in enabled {
        if let Some(item) = items.get(&id) {
            item.set_enabled(on).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn build(
    app: &AppHandle,
    entry: &Entry,
    items: &mut HashMap<String, MenuItem<Wry>>,
) -> Res<MenuItemKind<Wry>> {
    let err = |e: tauri::Error| e.to_string();
    Ok(match entry {
        Entry::Item {
            id,
            text,
            keys,
            enabled,
        } => {
            let item = MenuItem::with_id(app, id.as_str(), text, *enabled, keys.as_deref())
                .map_err(err)?;
            items.insert(id.clone(), item.clone());
            MenuItemKind::MenuItem(item)
        }
        Entry::Separator => {
            MenuItemKind::Predefined(PredefinedMenuItem::separator(app).map_err(err)?)
        }
        Entry::Submenu {
            text,
            role,
            items: children,
        } => {
            let sub = match role.as_deref() {
                Some("window") => Submenu::with_id(app, WINDOW_SUBMENU_ID, text, true),
                Some("help") => Submenu::with_id(app, HELP_SUBMENU_ID, text, true),
                _ => Submenu::new(app, text, true),
            }
            .map_err(err)?;
            for child in children {
                sub.append(&build(app, child, items)?).map_err(err)?;
            }
            MenuItemKind::Submenu(sub)
        }
        Entry::Native { name } => MenuItemKind::Predefined(native(app, name)?),
    })
}

/// The system's items the UI can ask for (`native`).
const NATIVE: &[&str] = &[
    "about",
    "services",
    "hide",
    "hide_others",
    "show_all",
    "quit",
    "cut",
    "copy",
    "paste",
    "minimize",
    "maximize",
    "fullscreen",
    "close_window",
    "bring_all_to_front",
];

fn native(app: &AppHandle, name: &str) -> Res<PredefinedMenuItem<Wry>> {
    if !NATIVE.contains(&name) {
        return Err(format!("no system menu item {name:?}"));
    }
    let item = match name {
        "about" => PredefinedMenuItem::about(app, None, Some(about())),
        "services" => PredefinedMenuItem::services(app, None),
        "hide" => PredefinedMenuItem::hide(app, None),
        "hide_others" => PredefinedMenuItem::hide_others(app, None),
        "show_all" => PredefinedMenuItem::show_all(app, None),
        "quit" => PredefinedMenuItem::quit(app, None),
        "cut" => PredefinedMenuItem::cut(app, None),
        "copy" => PredefinedMenuItem::copy(app, None),
        "paste" => PredefinedMenuItem::paste(app, None),
        "minimize" => PredefinedMenuItem::minimize(app, None),
        "maximize" => PredefinedMenuItem::maximize(app, None),
        "fullscreen" => PredefinedMenuItem::fullscreen(app, None),
        "close_window" => PredefinedMenuItem::close_window(app, None),
        "bring_all_to_front" => PredefinedMenuItem::bring_all_to_front(app, None),
        _ => return Err(format!("no system menu item {name:?}")),
    };
    item.map_err(|e| e.to_string())
}

fn about() -> AboutMetadata<'static> {
    AboutMetadata {
        name: Some("Ambrotype".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("© 2026 The Ambrotype contributors".into()),
        license: Some("GPL-3.0-or-later".into()),
        website: Some(commands::settings::SOURCE_URL.into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    /// Every item in `MENU_BAR` (ui/menu.js) is a keyboard action, one of
    /// the menu's own extras, or a system item `native` knows.
    #[test]
    fn menu_bar_names_only_known_actions() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let menu = std::fs::read_to_string(ui.join("menu.js")).unwrap();
        let keys = std::fs::read_to_string(ui.join("keys.js")).unwrap();
        let bar = menu
            .split("const MENU_BAR = [")
            .nth(1)
            .and_then(|r| r.split("\n];").next())
            .unwrap();
        let mut checked = 0;
        for name in bar.split('"').skip(1).step_by(2) {
            let name = name.trim_start_matches('~');
            // Menu titles and roles.
            if name == "|"
                || name.starts_with(char::is_uppercase)
                || ["mac", "window", "help"].contains(&name)
            {
                continue;
            }
            checked += 1;
            if let Some(n) = name.strip_prefix('#') {
                assert!(super::NATIVE.contains(&n), "unknown system item {name}");
                continue;
            }
            let generated = name
                .strip_prefix("rate-")
                .is_some_and(|d| d.parse::<u8>().is_ok_and(|d| d <= 5))
                || ["red", "yellow", "green", "blue"]
                    .iter()
                    .any(|c| name == format!("label-{c}"))
                || ["library", "develop", "color", "retouch", "film", "collage"]
                    .iter()
                    .any(|t| name == format!("task-{t}"));
            assert!(
                generated
                    || keys.contains(&format!("id: \"{name}\""))
                    || menu.contains(&format!("{name}: () =>")),
                "MENU_BAR names {name}, which keys.js doesn't have"
            );
        }
        assert!(checked > 40, "{checked}");
    }
}
