// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb search`: photos found by what is in them, or like another one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Rank the photos named (folders: the photos in them) by `query`'s words,
/// or with `--like PHOTO` by likeness to it, with the chosen search model;
/// photos not indexed yet are indexed first and kept in the catalogue.
pub(crate) fn search(
    args: &[&str],
    options: &BTreeMap<&str, &str>,
    json: bool,
) -> Result<String, String> {
    let like = options.get("--like").map(PathBuf::from);
    let (query, places) = match (&like, args) {
        (Some(_), places) => (None, places),
        (None, [q, places @ ..]) => (Some(*q), places),
        (None, []) => {
            return Err("search needs words (or --like PHOTO) and photos or folders".into());
        }
    };
    if places.is_empty() {
        return Err("search needs photos or folders to look in".into());
    }
    let photos = crate::organise::photos(places)?;
    let dir = crate::models::models_dir()?;
    let spec = amb_ai::chosen(&dir, "search")?;
    if !amb_ai::list(&dir)
        .iter()
        .any(|m| m.spec.id == spec.id && m.installed)
    {
        return Err(
            "no search model installed: `amb models install clip-b32` (156 MB) or `amb models install siglip2-base` (690 MB, better)".into()
        );
    }
    let model = amb_ai::search::Embedder::load(&dir, &spec)?;
    let cat_path = match options.get("--catalog") {
        Some(p) => PathBuf::from(p),
        None => {
            amb_library::catalog::default_path().ok_or("no config folder: give --catalog FILE")?
        }
    };
    let cat = amb_library::catalog::Catalog::open(&cat_path)?;
    let embed = |px: &amb_library::search::Pixels| model.image(&px.0, px.1, px.2);
    let mut all = photos.clone();
    if let Some(l) = &like {
        let l = std::path::absolute(l).map_err(|e| e.to_string())?;
        if !all.contains(&l) {
            all.push(l);
        }
    }
    let new = amb_library::search::index(&cat, &spec.id, &all, &embed, &|done, total| {
        eprint!("\rindexing {done}/{total} with {}", spec.name);
    });
    if new > 0 {
        eprintln!();
    }
    let target = match (&like, query) {
        (Some(l), _) => {
            let l = std::path::absolute(l).map_err(|e| e.to_string())?;
            cat.vectors(&spec.id, std::slice::from_ref(&l))
                .pop()
                .flatten()
                .ok_or_else(|| format!("{}: couldn't read it", l.display()))?
        }
        (None, Some(q)) => model.text(q)?,
        (None, None) => unreachable!(),
    };
    // The photo searched for like isn't one of its own matches.
    let looked = match &like {
        Some(l) => std::path::absolute(l).ok(),
        None => None,
    };
    let candidates: Vec<PathBuf> = photos
        .iter()
        .filter(|p| Some(*p) != looked.as_ref())
        .cloned()
        .collect();
    let found = amb_library::search::rank(&cat, &spec.id, &candidates, &target);
    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "model": spec.id,
            "indexed": new,
            "found": found.iter().map(|(p, s)| serde_json::json!({"path": p, "score": s})).collect::<Vec<_>>(),
        }))
        .map_err(|e| e.to_string());
    }
    let mut out = format!(
        "{} photo(s) found of {} ({}):\n",
        found.len(),
        photos.len(),
        spec.name
    );
    for (p, s) in &found {
        out += &format!("  {s:.3}  {}\n", display(p));
    }
    Ok(out.trim_end().to_string())
}

fn display(p: &Path) -> String {
    p.display().to_string()
}
