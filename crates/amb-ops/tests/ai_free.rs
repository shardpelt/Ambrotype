// SPDX-License-Identifier: GPL-3.0-or-later
//! The promise of AI-free mode, tested end to end: with it on, no model hook
//! is called by any path (render, masks, AI Remove, denoise, object
//! selection, faces), and the command path refuses every AI command. With it
//! off, the same edit calls the hooks, which proves this test can see them.
//!
//! The switch is process-wide and the hooks are too, so this file holds one
//! test that runs both halves in turn.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use amb_engine::{pipeline, raw, retouch, subject};
use amb_presets::ai_free;
use amb_protocol::{DevelopState, parse_commands};
use amb_testkit::{dng, patterns};

static CALLS: AtomicUsize = AtomicUsize::new(0);

fn called() -> usize {
    CALLS.swap(0, Ordering::SeqCst)
}

/// Every engine hook, counting its calls (`ai_free::GATED_HOOKS` lists them).
fn install_counting_hooks() {
    let map = || subject::Map {
        width: 8,
        height: 8,
        data: (0..64).map(|i| if i % 8 < 4 { 1.0 } else { 0.0 }).collect(),
    };
    subject::set_finder(Arc::new(move |_, _, _, _| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(map())
    }));
    subject::set_selector(Arc::new(move |_, _, _, _| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(map())
    }));
    subject::set_face_finder(Arc::new(|_, _, _| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }));
    retouch::set_inpainter(Arc::new(|rgb, _| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(rgb.to_vec())
    }));
    amb_engine::raw_denoise::set_raw_denoiser(Arc::new(|input, _| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        match input {
            amb_engine::raw_denoise::DenoiseInput::Linear { rgb, .. } => Ok(rgb.to_vec()),
            amb_engine::raw_denoise::DenoiseInput::Bayer { .. } => Err("not used".into()),
        }
    }));
}

/// Every command that needs a model, as the wire format has it.
const AI_COMMANDS: &[&str] = &[
    r#"{"cmd":"add_mask","kind":"subject"}"#,
    r#"{"cmd":"add_mask","kind":"background"}"#,
    r#"{"cmd":"add_mask","kind":"people"}"#,
    r#"{"cmd":"add_spot","at":[0.5,0.5],"kind":"remove"}"#,
    r#"{"cmd":"add_spot","at":[0.5,0.5],"kind":"content-aware"}"#,
    r#"{"cmd":"set_param","name":"EnhanceDenoiseVersion","value":"1"}"#,
];

/// Commands that need none, and must keep working.
const PLAIN_COMMANDS: &[&str] = &[
    r#"{"cmd":"add_mask","kind":"sky","adjust":{"Exposure":0.5}}"#,
    r#"{"cmd":"add_mask","kind":"linear"}"#,
    r#"{"cmd":"add_spot","at":[0.5,0.5],"kind":"heal"}"#,
    r#"{"cmd":"set_param","name":"Exposure2012","value":0.5}"#,
    r#"{"cmd":"set_param","name":"EnhanceDenoiseAlreadyApplied","value":"False"}"#,
];

fn run(state: &mut DevelopState, json: &str) -> Result<(), amb_protocol::Error> {
    for c in parse_commands(json).unwrap() {
        state.execute(&c)?;
    }
    Ok(())
}

/// An edit that holds every AI part: subject, background and people masks
/// (each brightening by two stops) and an AI Remove spot in the middle.
fn ai_edit() -> std::collections::BTreeMap<String, amb_presets::Value> {
    let mut s = DevelopState::new();
    for kind in ["subject", "background", "people"] {
        run(
            &mut s,
            &format!(r#"{{"cmd":"add_mask","kind":"{kind}","adjust":{{"Exposure":2.0}}}}"#),
        )
        .unwrap();
    }
    run(
        &mut s,
        r#"{"cmd":"add_spot","at":[0.5,0.5],"radius":0.05,"kind":"remove"}"#,
    )
    .unwrap();
    s.settings().clone()
}

fn render(
    img: &raw::CameraImage,
    settings: &std::collections::BTreeMap<String, amb_presets::Value>,
) -> Vec<f32> {
    let (develop, _) = pipeline::DevelopSettings::from_settings(settings);
    pipeline::render(img, &develop).rgb
}

#[test]
fn no_model_runs_while_ai_free_mode_is_on() {
    let _lock = ai_free::test_lock();
    install_counting_hooks();
    let dir = std::env::temp_dir().join(format!("amb-aifree-ops-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let photo = dir.join("target.dng");
    dng::write(
        &photo,
        &patterns::generate("colorchecker").unwrap(),
        &Default::default(),
    )
    .unwrap();
    let img = raw::load(&photo).unwrap();
    let edit = ai_edit();
    let plain = render(&img, &Default::default());
    called();

    // ---- off: the same edit calls the hooks (the test can see them) ------
    let with_ai = render(&img, &edit);
    assert!(called() >= 2, "masks and the spot should reach their hooks");
    assert_ne!(with_ai, plain, "the AI parts change the picture when off");
    assert!(subject::faces(&[0.0; 12], 2, 2).is_ok());
    assert!(subject::select(&[0.0; 12], 2, 2, &[]).is_ok());
    assert!(subject::find(subject::MapKind::Subject, &[0.0; 12], 2, 2).is_ok());
    let out = dir.join("denoised.dng");
    amb_ops::denoise::denoise(&photo, Some(&out), 100.0, &|_, _| {}).unwrap();
    assert!(called() >= 4, "denoise reaches its hook");
    for c in AI_COMMANDS.iter().chain(PLAIN_COMMANDS) {
        run(&mut DevelopState::new(), c).unwrap_or_else(|e| panic!("{c}: {e}"));
    }

    // ---- on: nothing is called, the AI parts are left out ----------------
    ai_free::set(true);
    assert!(!subject::available());
    let left = render(&img, &edit);
    assert_eq!(called(), 0, "a hook was called while AI-free mode is on");
    assert_eq!(left, plain, "an edit's AI parts must render as nothing");
    let named = amb_engine::ai_free::left_out(&edit);
    assert!(
        named.iter().any(|n| n.contains("Subject"))
            && named.iter().any(|n| n.contains("People"))
            && named.iter().any(|n| n.contains("Remove")),
        "{named:?}"
    );
    assert!(subject::faces(&[0.0; 12], 2, 2).unwrap_err() == ai_free::MESSAGE);
    assert!(subject::select(&[0.0; 12], 2, 2, &[]).unwrap_err() == ai_free::MESSAGE);
    assert!(subject::find(subject::MapKind::People, &[0.0; 12], 2, 2).is_err());
    let again = dir.join("denoised-again.dng");
    let refused = amb_ops::denoise::denoise(&photo, Some(&again), 100.0, &|_, _| {}).unwrap_err();
    assert_eq!(refused, ai_free::MESSAGE);
    assert!(!again.exists());
    let look = amb_ops::burst::look(&image::RgbImage::new(64, 64));
    assert_eq!(look.faces, 0);
    assert_eq!(called(), 0, "a hook was called while AI-free mode is on");

    // The command path refuses each AI command, and only those.
    for c in AI_COMMANDS {
        let e = run(&mut DevelopState::new(), c).unwrap_err();
        assert!(e.to_string().contains(ai_free::MESSAGE), "{c}: {e}");
    }
    for c in PLAIN_COMMANDS {
        run(&mut DevelopState::new(), c).unwrap_or_else(|e| panic!("{c}: {e}"));
    }
    // An undo can't bring one back, and a duplicate can't copy one in.
    let mut s = DevelopState::from_settings(edit.clone());
    assert!(run(&mut s, r#"{"cmd":"duplicate_mask","mask":0}"#).is_err());
    ai_free::set(false);
    let mut s = DevelopState::new();
    run(&mut s, r#"{"cmd":"add_mask","kind":"subject"}"#).unwrap();
    run(&mut s, r#"{"cmd":"remove_mask","mask":0}"#).unwrap();
    ai_free::set(true);
    assert!(
        run(&mut s, r#"{"cmd":"undo"}"#).is_err(),
        "undo brought an AI mask back"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
