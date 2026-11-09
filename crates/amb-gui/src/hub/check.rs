// SPDX-License-Identifier: GPL-3.0-or-later
//! `--check-hub`: install every item of a hub checkout, as the hub's CI does.

use super::*;

/// `ambrotype --check-hub HUB_FOLDER`: install every item of a hub checkout
/// into a scratch folder, as the app installs it (checksums, formats, each
/// kind's own checks, a plugin's module), and print each one's result; true
/// when all install. The hub's CI runs it on every submission.
pub fn check_hub(hub: &Path) -> bool {
    // Not canonicalize: on Windows it gives \\?\C:\..., which isn't a file:// address.
    let hub = match std::path::absolute(hub) {
        Ok(h) => h,
        Err(e) => {
            println!("{}: {e}", hub.display());
            return false;
        }
    };
    // Forward slashes: item paths are found next to the index by its last '/'.
    let index = format!(
        "file://{}",
        hub.join("index.json")
            .display()
            .to_string()
            .replace('\\', "/")
    );
    let items = match read_index(&index) {
        Ok(items) => items,
        Err(e) => {
            println!("index.json: error: {e}");
            return false;
        }
    };
    let scratch = std::env::temp_dir().join(format!("amb-check-hub-{}", std::process::id()));
    let mut ok = true;
    for item in &items {
        let dir = scratch.join(&item.id);
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            match item.kind.as_str() {
                "plugin" => install_plugin(&dir, &index, &item.id).map(drop),
                "interface" => install_interface(&dir, &index, &item.id).map(drop),
                "preset" => install_preset(&dir, &index, &item.id).map(drop),
                "collage" => install_collage(&mut Vec::new(), &index, &item.id).map(drop),
                "border" => install_border(&mut Vec::new(), &index, &item.id).map(drop),
                "sizes" => install_sizes(&mut Vec::new(), &index, &item.id).map(drop),
                _ => install(&dir, &index, &item.id).map(drop),
            }
        })();
        match result {
            Ok(()) => println!("{}: ok ({})", item.id, item.path),
            Err(e) => {
                ok = false;
                println!("{}: error: {e} ({})", item.id, item.path);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    println!(
        "{} items, {}",
        items.len(),
        if ok {
            "all install"
        } else {
            "some don't install"
        }
    );
    ok
}
