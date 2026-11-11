// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb docs`: the guides for AI agents, built in (`amb_protocol::guides`),
//! so an installed `amb` has them without the source's docs folder.

use amb_protocol::guides::{self, GUIDES};
use std::path::Path;

use crate::organise::Options;

pub(crate) fn docs(args: &[&str], options: &Options) -> Result<(), String> {
    if let Some(dir) = options.get("--to") {
        let dir = Path::new(dir);
        for g in GUIDES {
            let path = dir.join(g.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&path, g.text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        eprintln!(
            "Wrote {} guides to {} (start with {})",
            GUIDES.len(),
            dir.display(),
            dir.join(GUIDES[0].path).display()
        );
        return Ok(());
    }
    match args {
        [] => {
            println!("The guides (amb docs NAME prints one; amb docs --to DIR writes them all):\n");
            for g in GUIDES {
                println!("  {:<15}{}", g.name, g.about);
            }
            Ok(())
        }
        names => {
            for name in names {
                let g = guides::find(name).ok_or_else(|| {
                    let all: Vec<&str> = GUIDES.iter().map(|g| g.name).collect();
                    format!("no guide {name}: one of {}", all.join(", "))
                })?;
                print!("{}", g.text);
            }
            Ok(())
        }
    }
}
