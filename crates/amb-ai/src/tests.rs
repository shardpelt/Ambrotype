use super::model::{fingerprint, try_model};
use super::spec::catalogue;
use super::store::{choose, chosen, file_of, known_task, list, remove};
use super::*;
use std::path::PathBuf;

fn checker(w: usize, h: usize, scale: f32) -> Vec<f32> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            let v = if (x / 40 + y / 60) % 2 == 0 { 0.2 } else { 0.7 };
            [v * scale, v * 0.9 * scale, v * 0.8 * scale].map(|c| c.min(1.0))
        })
        .collect()
}

#[test]
fn fingerprints_ignore_exposure_but_not_the_picture() {
    let img = checker(320, 320, 1.0);
    assert_eq!(
        fingerprint(&img, 320, 320),
        fingerprint(&checker(320, 320, 1.3), 320, 320)
    );
    let flipped: Vec<f32> = img.iter().map(|v| 0.9 - v).collect();
    assert_ne!(fingerprint(&img, 320, 320), fingerprint(&flipped, 320, 320));
}

#[test]
fn the_list_has_one_default_per_task_and_free_licences() {
    let all = catalogue();
    for (task, _) in TASKS {
        assert_eq!(
            all.iter().filter(|s| s.task == task && s.default).count(),
            1,
            "{task}"
        );
    }
    let mut ids: Vec<_> = all.iter().map(|s| &s.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "ids are unique");
    for s in &all {
        known_task(&s.task).unwrap();
        assert!(
            // GPL-3.0 fits a GPL-3.0 app (the denoiser, also CC BY 4.0).
            ["Apache-2.0", "MIT", "BSD-3-Clause", "GPL-3.0 or CC BY 4.0"]
                .contains(&s.licence.as_str()),
            "{}",
            s.id
        );
        assert!(
            s.bytes > 0
                && s.sha256.len() == 64
                && (!s.urls.is_empty() || s.archive.as_ref().is_some_and(|a| !a.urls.is_empty())),
            "{}",
            s.id
        );
        assert!(s.source.starts_with("https://"), "{}", s.id);
        assert!(s.seconds > 0.0, "{}", s.id);
        if s.task == "search" {
            // Search models: their text half and tokenizer too.
            assert!(s.search.is_some() && s.files.len() == 2, "{}", s.id);
            for f in &s.files {
                assert!(
                    f.bytes > 0 && f.sha256.len() == 64 && !f.urls.is_empty(),
                    "{}",
                    s.id
                );
            }
        } else if s.task == "objects" {
            // Select Object models: their decoder too.
            assert!(s.files.iter().any(|f| f.name == "decoder.onnx"), "{}", s.id);
        } else if s.task == "denoise" {
            // The Bayer network, and the linear one, from one archive.
            let a = s.archive.as_ref().expect("an archive");
            assert!(a.bytes > 0 && a.sha256.len() == 64 && !a.member.is_empty());
            assert!(
                s.files
                    .iter()
                    .any(|f| f.name == "linear.onnx" && !f.member.is_empty())
            );
        } else if s.task != "remove" && s.task != "faces" {
            // Masks: how much edge detail they keep.
            assert!((1..=5).contains(&s.detail), "{}", s.id);
        }
    }
}

#[test]
fn choosing_needs_an_installed_model_and_removing_goes_back_to_the_default() {
    let dir = std::env::temp_dir().join(format!("amb-ai-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(chosen(&dir, "subject").unwrap().id, "u2netp");
    assert!(choose(&dir, "u2net").is_err());
    assert!(chosen(&dir, "nothing").is_err());
    // A file of the right size stands in for the download.
    std::fs::create_dir_all(&dir).unwrap();
    let big = catalogue().into_iter().find(|s| s.id == "u2net").unwrap();
    std::fs::File::create(file_of(&dir, "u2net"))
        .unwrap()
        .set_len(big.bytes)
        .unwrap();
    choose(&dir, "u2net").unwrap();
    assert_eq!(chosen(&dir, "subject").unwrap().id, "u2net");
    let l = list(&dir);
    let row = l.iter().find(|m| m.spec.id == "u2net").unwrap();
    assert!(row.installed && row.chosen);
    assert!(!l.iter().find(|m| m.spec.id == "u2netp").unwrap().chosen);
    remove(&dir, "u2net").unwrap();
    assert_eq!(chosen(&dir, "subject").unwrap().id, "u2netp");
    let _ = std::fs::remove_dir_all(&dir);
}

/// With the models downloaded (`AMB_MODELS=dir`): each finds a bright
/// disc on a dark ground as the subject.
#[test]
#[ignore]
fn installed_models_find_a_bright_disc() {
    let dir = PathBuf::from(std::env::var("AMB_MODELS").unwrap());
    let (w, h) = (640, 480);
    let img: Vec<f32> = (0..w * h)
        .flat_map(|i| {
            let (x, y) = ((i % w) as f32 - 320.0, (i / w) as f32 - 240.0);
            if x * x + y * y < 120.0 * 120.0 {
                [0.9, 0.5, 0.2]
            } else {
                [0.1, 0.12, 0.15]
            }
        })
        .collect();
    for m in list(&dir)
        .into_iter()
        .filter(|m| m.installed && m.spec.task == "subject")
    {
        let (map, took) = try_model(&dir, &m.spec.id, &img, w, h).unwrap();
        let at = |u: f32, v: f32| {
            map.data[(v * map.height as f32) as usize * map.width + (u * map.width as f32) as usize]
        };
        eprintln!(
            "{}: {took:?} centre {} corner {}",
            m.spec.id,
            at(0.5, 0.5),
            at(0.03, 0.03)
        );
        assert!(at(0.5, 0.5) > 0.7, "{}", m.spec.id);
        assert!(at(0.03, 0.03) < 0.3, "{}", m.spec.id);
    }
}
