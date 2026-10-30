// SPDX-License-Identifier: GPL-3.0-or-later
//! Smart search over photo files: each photo's vector from a search model
//! (`amb_ai::search`), kept in the catalogue until the file changes, and the
//! photos ranked by how close they are to a search's words or to another
//! photo. The model itself is passed in as `embed`, so this crate doesn't
//! need it.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::catalog::Catalog;

/// Long side the photos are developed at before the model squares them.
const EDGE: usize = 448;

/// A photo's pixels for the model: sRGB 0–1 interleaved, width, height.
pub type Pixels = (Vec<f32>, usize, usize);

/// A photo developed small with its own edit, as sRGB pixels.
pub fn pixels(photo: &Path) -> Result<Pixels, String> {
    let img = crate::contact::small(photo, EDGE)?;
    Ok((amb_engine::output::to_srgb(&img), img.width, img.height))
}

/// Make sure every photo has a vector from model `model`: those without
/// one (or whose file changed) are developed and passed to `embed`, a few at
/// a time, and kept in the catalogue. `progress` hears how many are done
/// out of how many were missing. Photos that can't be read are skipped.
pub fn index(
    cat: &Catalog,
    model: &str,
    photos: &[PathBuf],
    embed: &(dyn Fn(&Pixels) -> Result<Vec<f32>, String> + Sync),
    progress: &dyn Fn(usize, usize),
) -> usize {
    let have = cat.vectors(model, photos);
    let missing: Vec<&PathBuf> = photos
        .iter()
        .zip(&have)
        .filter(|(_, v)| v.is_none())
        .map(|(p, _)| p)
        .collect();
    let mut done = 0;
    for chunk in missing.chunks(8) {
        // Developing is the slow, parallel part; the model uses every core itself.
        let developed: Vec<_> = chunk.par_iter().map(|p| pixels(p)).collect();
        let vectors: Vec<(PathBuf, Vec<f32>)> = chunk
            .iter()
            .zip(developed)
            .filter_map(|(p, px)| Some(((*p).clone(), embed(&px.ok()?).ok()?)))
            .collect();
        cat.store_vectors(model, &vectors);
        done += chunk.len();
        progress(done, missing.len());
    }
    missing.len()
}

/// The photos (of those indexed) ranked by closeness to `target`, best
/// first, with their scores; only those that stand out from the rest.
pub fn rank(cat: &Catalog, model: &str, photos: &[PathBuf], target: &[f32]) -> Vec<(PathBuf, f32)> {
    let mut scored: Vec<(PathBuf, f32)> = photos
        .iter()
        .zip(cat.vectors(model, photos))
        .filter_map(|(p, v)| {
            let v = v?;
            Some((p.clone(), v.iter().zip(target).map(|(a, b)| a * b).sum()))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    keep_standouts(scored)
}

/// The best matches: those closer than halfway from the median score to the
/// best one (the scale differs between models), at least one, at most 200.
fn keep_standouts(scored: Vec<(PathBuf, f32)>) -> Vec<(PathBuf, f32)> {
    if scored.len() < 3 {
        return scored;
    }
    let best = scored[0].1;
    let median = scored[scored.len() / 2].1;
    let cut = median + (best - median) * 0.5;
    scored
        .into_iter()
        .enumerate()
        .take_while(|(i, (_, s))| *i == 0 || *s >= cut)
        .map(|(_, x)| x)
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standouts_are_kept_and_the_rest_dropped() {
        let p = |i: usize, s: f32| (PathBuf::from(format!("{i}.jpg")), s);
        let scored = vec![
            p(0, 0.30),
            p(1, 0.29),
            p(2, 0.22),
            p(3, 0.21),
            p(4, 0.20),
            p(5, 0.20),
        ];
        let kept: Vec<_> = keep_standouts(scored).into_iter().map(|(p, _)| p).collect();
        assert_eq!(kept, [PathBuf::from("0.jpg"), PathBuf::from("1.jpg")]);
    }

    #[test]
    fn vectors_are_kept_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("amb-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("a.jpg");
        std::fs::write(&photo, b"not a photo").unwrap();
        let cat = Catalog::in_memory();
        cat.store_vectors("m", &[(photo.clone(), vec![0.6, 0.8])]);
        assert_eq!(
            cat.vectors("m", std::slice::from_ref(&photo)),
            vec![Some(vec![0.6, 0.8])]
        );
        assert_eq!(
            cat.vectors("other", std::slice::from_ref(&photo)),
            vec![None]
        );
        let ranked = rank(&cat, "m", std::slice::from_ref(&photo), &[0.6, 0.8]);
        assert!((ranked[0].1 - 1.0).abs() < 1e-6);
        std::fs::write(&photo, b"a different file now").unwrap();
        assert_eq!(cat.vectors("m", std::slice::from_ref(&photo)), vec![None]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
