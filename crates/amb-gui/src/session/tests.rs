// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the open photo.

use super::render::to_srgb8;
use super::*;
use crate::preset_files::*;

#[test]
fn photo_sidecars_are_not_presets() {
    let dir = std::env::temp_dir().join(format!(
        "amb-gui-sidecars-not-presets-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.join("Look")).unwrap();
    let packet = |name: &str| {
        amb_presets::xmp_writer::write_packet(
            &[
                (
                    "Name".to_string(),
                    Value::Alt(vec![("x-default".into(), name.into())]),
                ),
                ("Exposure2012".to_string(), Value::Number(0.5)),
            ]
            .into_iter()
            .collect(),
            amb_presets::is_signed_tag,
        )
    };
    std::fs::write(dir.join("Look/Warm.xmp"), packet("Warm")).unwrap();
    std::fs::write(dir.join("Look/IMG_1.NEF"), b"x").unwrap();
    std::fs::write(dir.join("Look/IMG_1.xmp"), packet("")).unwrap();
    std::fs::write(dir.join("Look/IMG_1.copy-2.xmp"), packet("")).unwrap();
    // A JPEG's sidecar keeps the photo's whole name.
    std::fs::write(dir.join("Look/DSCF9692.JPG"), b"x").unwrap();
    std::fs::write(dir.join("Look/DSCF9692.JPG.xmp"), packet("")).unwrap();
    let found: Vec<String> = scan_presets(std::slice::from_ref(&dir), None)
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(found, ["Warm"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_copies_are_named_like_lightrooms() {
    let dir = std::env::temp_dir().join(format!("amb-gui-editin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let raw = dir.join("IMG_0001.NEF");
    std::fs::write(&raw, b"x").unwrap();
    assert_eq!(edit_copy_path(&raw), dir.join("IMG_0001-Edit.tif"));
    std::fs::write(dir.join("IMG_0001-Edit.tif"), b"x").unwrap();
    assert_eq!(edit_copy_path(&raw), dir.join("IMG_0001-Edit-2.tif"));
    // A virtual copy's goes next to its photo, by the photo's name.
    let copy = dir.join("IMG_0001.copy-2.xmp");
    std::fs::write(&copy, "<x/>").unwrap();
    assert_eq!(edit_copy_path(&copy), dir.join("IMG_0001-Edit-2.tif"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A synthetic DNG in its own file: tests run in parallel, and two of them
/// writing the same path would read each other's half-written file.
fn synthetic(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static TIDY: std::sync::Once = std::sync::Once::new();
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // One folder for every run; files from runs over an hour ago go, so
    // it doesn't grow with each `cargo test`.
    let dir = std::env::temp_dir().join("amb-gui-tests");
    std::fs::create_dir_all(&dir).unwrap();
    TIDY.call_once(|| {
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if e.metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t < hour_ago)
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    });
    let path = dir.join(format!("{name}-{}-{n}.dng", std::process::id()));
    amb_testkit::dng::write(
        &path,
        &amb_testkit::patterns::generate(name).unwrap(),
        &Default::default(),
    )
    .unwrap();
    // A process with the same id may have left a sidecar here.
    let _ = std::fs::remove_file(sidecar_path(&path));
    path
}

#[test]
fn open_edit_preview_undo_save_export() {
    let path = synthetic("colorchecker");
    let lens = LensSources::default();
    let mut s = Session::open(&path).unwrap();
    let info = s.info();
    assert_eq!((info.width, info.height), (1368, 920));
    assert!(!info.sidecar_loaded);

    let base = s.preview(512, &BTreeMap::new(), false, false, &lens);
    // Integer area averaging: the long edge is at most 512 (here 1368 / 3).
    assert_eq!(base.width, 456);
    assert_eq!(
        base.histogram[1].iter().sum::<u32>() as usize,
        base.width * base.height
    );
    assert_eq!(&base.jpeg[..2], &[0xFF, 0xD8]);

    // A drag override brightens without touching history.
    let over = BTreeMap::from([("Exposure2012".to_string(), Value::Number(1.0))]);
    let bright = s.preview(512, &over, false, false, &lens);
    let mean = |p: &Preview| {
        p.histogram[1]
            .iter()
            .enumerate()
            .map(|(i, n)| i as f64 * f64::from(*n))
            .sum::<f64>()
            / (p.width * p.height) as f64
    };
    assert!(mean(&bright) > mean(&base) + 10.0);
    assert!(!s.state_info().can_undo);

    // A crop renders a crop-sized preview; the photo info stays uncropped.
    let crop = BTreeMap::from([
        ("HasCrop".to_string(), Value::Bool(true)),
        ("CropLeft".to_string(), Value::Number(0.25)),
        ("CropRight".to_string(), Value::Number(0.75)),
        ("CropAngle".to_string(), Value::Number(2.0)),
    ]);
    let cropped = s.preview(512, &crop, false, false, &lens);
    // About half the photo's width, rendered from a larger source so the
    // crop still fills about `max_edge` (integer downscale: 1368 / 2).
    assert!((300..=512).contains(&cropped.width), "{}", cropped.width);
    assert!(cropped.height > cropped.width, "{}", cropped.height);
    assert_eq!((s.info().width, s.info().height), (1368, 920));

    // Committing a command is undoable.
    s.execute(
        &[Command::SetParam {
            name: "Exposure2012".into(),
            value: serde_json::json!(1.0),
        }],
        None,
    )
    .unwrap();
    let info = s.state_info();
    assert!(info.can_undo && info.settings["Exposure2012"] == serde_json::json!(1.0));
    s.execute(&[Command::Undo], None).unwrap();
    assert!(s.state.settings().is_empty());
    s.execute(&[Command::Redo], None).unwrap();

    // "Before" ignores the edits.
    let before = s.preview(512, &BTreeMap::new(), true, false, &lens);
    assert!((mean(&before) - mean(&base)).abs() < 0.5);

    let saved = s.save_sidecar().unwrap();
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.contains("crs:Exposure2012=\"+1\""), "{text}");
    let reopened = Session::open(&path).unwrap();
    assert!(reopened.info().sidecar_loaded);
    assert_eq!(reopened.state.settings().len(), 1);

    let out = path.with_extension("jpg");
    let mut settings = amb_export::ExportSettings::new(amb_export::Format::Jpeg);
    settings.resize = amb_export::Resize::LongEdge(800);
    let report = s.export(&out, &settings, &lens).unwrap();
    assert_eq!(report.width, 800);
}

#[test]
fn region_pick_white_and_clipping() {
    let path = synthetic("colorchecker");
    let lens = LensSources::default();
    let mut s = Session::open(&path).unwrap();
    let r = s.region(0.5, 0.5, 300, 200, false, false, &lens);
    assert_eq!((r.width, r.height), (300, 200));
    assert_eq!((r.full_width, r.full_height), (1368, 920));
    assert_eq!((r.x, r.y), (684 - 150, 460 - 100));
    // A corner request is clamped inside the image.
    let c = s.region(0.0, 1.0, 300, 200, false, true, &lens);
    assert_eq!((c.x, c.y), (0, 920 - 200));
    // Picking the light grey patch (row 4, second patch) gives D50: ~5000 K, and a tint
    // near +10 because D50 sits above the Planckian locus (Lightroom shows the same).
    let (patch, gap) = (200.0, 24.0);
    let u = (gap + (patch + gap) + patch / 2.0) / 1368.0;
    let v = (gap + 3.0 * (patch + gap) + patch / 2.0) / 920.0;
    let [("Temperature", t), ("Tint", tint)] = s.pick_white(u, v).expect("grey patch") else {
        panic!("a raw picks Kelvin");
    };
    assert!(
        (4800.0..5300.0).contains(&t) && (5.0..15.0).contains(&tint),
        "{t} {tint}"
    );
    let ctx = s.feedback_context(true);
    assert!(ctx.contains("Camera:") && ctx.contains("Settings:"));
}

#[test]
fn region_tiles_match_the_full_render_in_any_orientation() {
    let path = synthetic("colorchecker");
    let lens = LensSources::default();
    for orientation in [1u16, 6, 3] {
        let mut s = Session::open(&path).unwrap();
        s.full.metadata.orientation = orientation;
        let set = |name: &str, v: f64| Command::SetParam {
            name: name.into(),
            value: v.into(),
        };
        let edit = [
            set("Clarity2012", 40.0),
            set("Shadows2012", 50.0),
            set("ColorNoiseReduction", 0.0),
        ];
        s.execute(&edit, None).unwrap();
        let settings = s.effective_settings(&BTreeMap::new(), false);
        let develop = s.develop(&settings, &lens);
        let full = to_srgb8(&amb_export::geometry::orient(
            &pipeline::render(&s.full, &develop),
            orientation,
        ));
        // A first crop, then a pan that needs new tiles.
        for (cx, cy) in [(0.3, 0.4), (0.6, 0.55)] {
            let r = s.region(cx, cy, 300, 200, false, false, &lens);
            let cache = s.region_cache.as_ref().unwrap();
            let worst = (r.y..r.y + r.height)
                .flat_map(|y| (r.x * 3..(r.x + r.width) * 3).map(move |i| y * r.full_width * 3 + i))
                .map(|i| cache.srgb[i].abs_diff(full[i]))
                .max()
                .unwrap();
            assert!(worst <= 1, "orientation {orientation}: {worst}");
        }
    }
}

#[test]
fn crop_tool_round_trips_through_one_undo_step() {
    let path = synthetic("colorchecker");
    let lens = LensSources::default();
    let mut s = Session::open(&path).unwrap();
    let none = s.crop_state();
    assert!(!none.enabled);
    assert_eq!(
        none.rect,
        CropRect {
            cx: 684.0,
            cy: 460.0,
            width: 1368.0,
            height: 920.0,
            angle: 0.0
        }
    );
    let rect = CropRect {
        cx: 700.0,
        cy: 450.0,
        width: 800.0,
        height: 500.0,
        angle: 4.5,
    };
    let cmds = s.crop_commands(&rect);
    assert_eq!(cmds.len(), 6);
    s.execute(&cmds, Some("crop")).unwrap();
    let info = s.state_info();
    assert_eq!(info.history, vec!["crop".to_string()]);
    let back = s.crop_state();
    assert!(back.enabled);
    for (a, b) in [
        (back.rect.cx, rect.cx),
        (back.rect.cy, rect.cy),
        (back.rect.width, rect.width),
        (back.rect.height, rect.height),
        (back.rect.angle, rect.angle),
    ] {
        assert!((a - b).abs() < 0.01, "{back:?} vs {rect:?}");
    }
    // The preview renders the crop's size; with `HasCrop` overridden off
    // (the crop tool's view) it shows the whole photo.
    let p = s.preview(1368, &BTreeMap::new(), false, false, &lens);
    assert_eq!((p.width, p.height), (800, 500));
    let off = BTreeMap::from([("HasCrop".to_string(), Value::Bool(false))]);
    let whole = s.preview(1368, &off, false, false, &lens);
    assert_eq!((whole.width, whole.height), (1368, 920));
    // The whole photo with no angle (the tool's Reset) turns the crop off.
    let reset = s.crop_commands(&none.rect);
    s.execute(&reset, Some("reset crop")).unwrap();
    assert!(!s.crop_state().enabled);
    assert!(!s.state.settings().contains_key("CropLeft"));
    s.execute(&[Command::Undo], None).unwrap();
    assert!(s.crop_state().enabled);
}

#[test]
fn rating_and_keywords_are_written_at_once_without_unsaved_edits() {
    use amb_protocol::sidecar::PhotoInfo;
    let path = synthetic("ramp");
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.info, PhotoInfo::default());
    // An unsaved edit...
    s.execute(
        &[Command::SetParam {
            name: "Exposure2012".into(),
            value: serde_json::json!(1.0),
        }],
        None,
    )
    .unwrap();
    // ...is not written by rating the photo.
    let info = PhotoInfo {
        rating: Some(3),
        keywords: vec!["test".into()],
        label: Some("Red".into()),
        flag: Some(amb_protocol::sidecar::Flag::Pick),
        ..Default::default()
    };
    assert_eq!(s.set_info(&info).unwrap(), info);
    let on_disk = std::fs::read_to_string(&s.sidecar).unwrap();
    assert!(!on_disk.contains("Exposure2012"), "{on_disk}");
    // Saving the edit keeps the rating; reopening reads both.
    assert!(s.has_unsaved_edits());
    s.save_sidecar().unwrap();
    assert!(!s.has_unsaved_edits());
    let again = Session::open(&path).unwrap();
    assert_eq!(again.info, info);
    assert_eq!(again.state.settings().len(), 1);
}

#[test]
fn a_turn_written_by_another_app_turns_the_open_photo() {
    use amb_protocol::sidecar::{self, PhotoInfo};
    let path = synthetic("ramp");
    let mut s = Session::open(&path).unwrap();
    let before = s.full.metadata.orientation;
    // `amb tag --rotate right` while the photo is open.
    let on_disk = std::fs::read_to_string(&s.sidecar).ok();
    let info = PhotoInfo {
        orientation: Some(6),
        ..Default::default()
    };
    let text = sidecar::with_info(on_disk.as_deref(), &info).unwrap();
    sidecar::write_atomic(&s.sidecar, &text).unwrap();
    s.reload_sidecar().unwrap();
    assert_ne!(before, 6);
    assert_eq!(s.full.metadata.orientation, 6);
}

#[test]
fn presets_are_found_with_names_and_groups() {
    let dir = std::env::temp_dir().join(format!("amb-gui-presets-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Warm")).unwrap();
    std::fs::write(
        dir.join("Warm/sunny.xmp"),
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Temperature="6500" crs:Exposure2012="+0.3"/></rdf:RDF></x:xmpmeta>"#,
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not a preset").unwrap();
    let found = scan_presets(std::slice::from_ref(&dir), None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "sunny");
    assert_eq!(found[0].group, "Warm");
}

#[test]
fn saves_a_preset_without_the_crop() {
    let dir = std::env::temp_dir().join(format!("amb-save-preset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut settings = BTreeMap::new();
    settings.insert("Exposure2012".to_string(), Value::Text("+0.50".into()));
    settings.insert("GrainAmount".to_string(), Value::Text("25".into()));
    settings.insert("CropTop".to_string(), Value::Text("0.1".into()));
    settings.insert("HasCrop".to_string(), Value::Text("True".into()));
    let path = save_preset(&dir, "Faded street", "", &settings, true).unwrap();
    assert!(
        path.ends_with("Yours/Faded street.xmp"),
        "{}",
        path.display()
    );
    let preset = amb_presets::Preset::from_path(&path).unwrap();
    assert_eq!(preset.name(), Some("Faded street"));
    assert!(preset.settings.contains_key("GrainAmount"));
    assert!(!preset.settings.contains_key("CropTop") && !preset.settings.contains_key("HasCrop"));
    // It shows up in the presets list, in its group.
    let found = scan_presets(std::slice::from_ref(&dir), None);
    assert_eq!(
        (found[0].name.as_str(), found[0].group.as_str()),
        ("Faded street", "Yours")
    );
    assert!(save_preset(&dir, " ", "", &settings, true).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
