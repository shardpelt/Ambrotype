use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use flb_presets::Value;
use flb_protocol::edit::{self, EditRequest};
use flb_protocol::{Command, DevelopState, Error, Query};
use serde_json::json;

fn fixture(name: &str) -> String {
    format!(
        "{}/../flb-presets/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "flb-protocol-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn set(name: &str, value: serde_json::Value) -> Command {
    Command::SetParam {
        name: name.into(),
        value,
    }
}

fn num(state: &DevelopState, name: &str) -> Option<f64> {
    state.get(name).and_then(Value::as_f64)
}

#[test]
fn set_adjust_reset_undo_redo_with_diffs() {
    let mut s = DevelopState::new();

    let o = s.execute(&set("Exposure2012", json!(0.5))).unwrap();
    assert_eq!(o.diff.len(), 1);
    assert_eq!(o.diff[0].name, "Exposure2012");
    assert_eq!(o.diff[0].before, None);
    assert_eq!(o.diff[0].after, Some(Value::Number(0.5)));

    // Adjust from the current value, and from the registry default when unset.
    let o = s
        .execute(&Command::AdjustParam {
            name: "Exposure2012".into(),
            delta: 0.25,
        })
        .unwrap();
    assert_eq!(o.diff[0].before, Some(Value::Number(0.5)));
    assert_eq!(num(&s, "Exposure2012"), Some(0.75));
    s.execute(&Command::AdjustParam {
        name: "ColorGradeBlending".into(),
        delta: 10.0,
    })
    .unwrap();
    assert_eq!(num(&s, "ColorGradeBlending"), Some(60.0)); // default 50

    // Setting the same value again is a no-op and is not recorded.
    let depth = s.undo_stack().len();
    let o = s.execute(&set("Exposure2012", json!("+0.75"))).unwrap();
    assert!(o.diff.is_empty());
    assert_eq!(o.notes, vec!["no change".to_string()]);
    assert_eq!(s.undo_stack().len(), depth);

    let o = s
        .execute(&Command::ResetParam {
            name: "Exposure2012".into(),
        })
        .unwrap();
    assert_eq!(o.diff[0].after, None);
    assert!(s.get("Exposure2012").is_none());

    // Undo the reset, then the blending adjustment.
    let o = s.execute(&Command::Undo).unwrap();
    assert_eq!(o.command, "undo reset_param Exposure2012");
    assert_eq!(o.diff[0].after, Some(Value::Number(0.75)));
    assert_eq!(num(&s, "Exposure2012"), Some(0.75));
    s.execute(&Command::Undo).unwrap();
    assert!(s.get("ColorGradeBlending").is_none());
    assert_eq!(s.redo_stack().len(), 2);

    // Redo re-applies; a new command clears the redo stack.
    let o = s.execute(&Command::Redo).unwrap();
    assert_eq!(o.diff[0].after, Some(Value::Number(60.0)));
    s.execute(&set("Vibrance", json!(10))).unwrap();
    assert!(!s.can_redo());
    assert!(matches!(
        s.execute(&Command::Redo),
        Err(Error::NothingToRedo)
    ));

    // Undo everything back to empty.
    while s.can_undo() {
        s.execute(&Command::Undo).unwrap();
    }
    assert!(s.settings().is_empty());
    assert!(matches!(
        s.execute(&Command::Undo),
        Err(Error::NothingToUndo)
    ));
}

#[test]
fn reset_group_removes_only_that_group() {
    let mut s = DevelopState::new();
    s.execute_all(&[
        set("HueAdjustmentRed", json!(10)),
        set("SaturationAdjustmentBlue", json!(-20)),
        set("Exposure2012", json!(1)),
    ])
    .unwrap();
    let o = s
        .execute(&Command::ResetGroup {
            group: "hsl".into(),
        })
        .unwrap();
    assert_eq!(o.diff.len(), 2);
    assert_eq!(s.settings().len(), 1);
    let err = s
        .execute(&Command::ResetGroup {
            group: "colour_grading".into(),
        })
        .unwrap_err();
    assert!(
        err.to_string().contains("did you mean color_grading"),
        "{err}"
    );
}

#[test]
fn clamping_is_reported() {
    let mut s = DevelopState::new();
    let o = s.execute(&set("Exposure2012", json!(7.5))).unwrap();
    assert_eq!(num(&s, "Exposure2012"), Some(5.0));
    assert_eq!(o.clamped.len(), 1);
    assert_eq!(o.clamped[0].requested, 7.5);
    assert_eq!(o.clamped[0].applied, 5.0);
    assert!(o.clamped[0].to_string().contains("clamped to 5"));

    let o = s
        .execute(&Command::AdjustParam {
            name: "Saturation".into(),
            delta: -250.0,
        })
        .unwrap();
    assert_eq!(num(&s, "Saturation"), Some(-100.0));
    assert_eq!(o.clamped[0].applied, -100.0);
}

#[test]
fn unknown_tags_and_wrong_kinds_are_rejected_helpfully() {
    let mut s = DevelopState::new();
    let err = s.execute(&set("Exposure2021", json!(1))).unwrap_err();
    match &err {
        Error::UnknownTag { suggestions, .. } => assert_eq!(suggestions[0], "Exposure2012"),
        other => panic!("{other:?}"),
    }
    assert!(
        err.to_string().contains("did you mean Exposure2012"),
        "{err}"
    );
    let err = s.execute(&set("Temp", json!(5000))).unwrap_err();
    assert!(err.to_string().contains("Temperature"), "{err}");

    let err = s
        .execute(&set("Exposure2012", json!("bright")))
        .unwrap_err();
    assert!(matches!(err, Error::WrongKind { .. }), "{err:?}");
    let err = s.execute(&set("ConvertToGrayscale", json!(3))).unwrap_err();
    assert!(err.to_string().contains("boolean"), "{err}");
    let err = s
        .execute(&Command::AdjustParam {
            name: "WhiteBalance".into(),
            delta: 1.0,
        })
        .unwrap_err();
    assert!(err.to_string().contains("numeric"), "{err}");
    // Temperature has no default: adjusting an As Shot photo needs an absolute value.
    let err = s
        .execute(&Command::AdjustParam {
            name: "Temperature".into(),
            delta: 500.0,
        })
        .unwrap_err();
    assert!(err.to_string().contains("as-shot"), "{err}");
    assert!(s.settings().is_empty());
}

#[test]
fn temperature_switches_white_balance_to_custom() {
    let mut s = DevelopState::new();
    let o = s.execute(&set("Temperature", json!(6500))).unwrap();
    let names: Vec<&str> = o.diff.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Temperature", "WhiteBalance"]);
    assert_eq!(s.get("WhiteBalance"), Some(&Value::Text("Custom".into())));
    // One undo step reverts both.
    s.execute(&Command::Undo).unwrap();
    assert!(s.settings().is_empty());
}

#[test]
fn legacy_tags_get_a_note() {
    let mut s = DevelopState::new();
    let o = s.execute(&set("Contrast", json!(20))).unwrap();
    assert!(o.notes.iter().any(|n| n.contains("Contrast2012")), "{o:?}");
}

#[test]
fn transactions_roll_back_on_error() {
    let mut s = DevelopState::new();
    s.execute(&set("Vibrance", json!(5))).unwrap();
    let before = s.clone();
    let err = s
        .execute_all(&[set("Exposure2012", json!(1)), set("Nope", json!(1))])
        .unwrap_err();
    assert!(
        err.to_string().starts_with("command 1 (set_param Nope)"),
        "{err}"
    );
    assert_eq!(s, before);
}

#[test]
fn apply_preset_with_amount_and_group_filters() {
    let preset = fixture("punchy-teal-orange.xmp");
    let mut s = DevelopState::new();
    s.execute(&set("Exposure2012", json!(1.0))).unwrap();

    // Full strength: preset values land, metadata is not copied.
    let mut full = s.clone();
    let o = full
        .execute(&Command::ApplyPreset {
            path: preset.clone(),
            amount: 100.0,
            include_groups: None,
            exclude_groups: None,
        })
        .unwrap();
    assert_eq!(num(&full, "Contrast2012"), Some(35.0));
    assert_eq!(num(&full, "Exposure2012"), Some(0.2));
    assert_eq!(num(&full, "ColorGradeShadowHue"), Some(195.0));
    assert!(full.get("Name").is_none() && full.get("PresetType").is_none());
    assert_eq!(
        full.get("ProcessVersion"),
        Some(&Value::Text("15.4".into()))
    );
    assert!(o.diff.iter().any(|c| c.name == "Vibrance"));

    // Half strength interpolates from the current value (or default).
    let mut half = s.clone();
    half.execute(&Command::ApplyPreset {
        path: preset.clone(),
        amount: 50.0,
        include_groups: None,
        exclude_groups: None,
    })
    .unwrap();
    assert!((num(&half, "Exposure2012").unwrap() - 0.6).abs() < 1e-9); // 1.0 -> 0.2
    assert_eq!(num(&half, "Contrast2012"), Some(17.5)); // 0 -> 35
    assert_eq!(num(&half, "ColorGradeBlending"), Some(55.0)); // 50 -> 60

    // Only the tone group.
    let mut tone = s.clone();
    tone.execute(&Command::ApplyPreset {
        path: preset.clone(),
        amount: 100.0,
        include_groups: Some(vec!["tone".into()]),
        exclude_groups: None,
    })
    .unwrap();
    assert_eq!(num(&tone, "Highlights2012"), Some(-60.0));
    assert!(tone.get("Vibrance").is_none());
    assert!(tone.get("HueAdjustmentGreen").is_none());
    assert!(tone.get("ProcessVersion").is_none());

    // Everything except HSL and color grading; one undo reverts the whole preset.
    let mut ex = s.clone();
    ex.execute(&Command::ApplyPreset {
        path: preset.clone(),
        amount: 100.0,
        include_groups: None,
        exclude_groups: Some(vec!["hsl".into(), "color_grading".into()]),
    })
    .unwrap();
    assert!(ex.get("HueAdjustmentGreen").is_none());
    assert!(ex.get("ColorGradeShadowHue").is_none());
    assert_eq!(num(&ex, "Vibrance"), Some(25.0));
    ex.execute(&Command::Undo).unwrap();
    assert_eq!(ex.settings(), s.settings());

    // Bad amount and bad group are rejected before anything changes.
    assert!(
        s.clone()
            .execute(&Command::ApplyPreset {
                path: preset.clone(),
                amount: 250.0,
                include_groups: None,
                exclude_groups: None,
            })
            .is_err()
    );
    let err = s
        .clone()
        .execute(&Command::ApplyPreset {
            path: preset,
            amount: 100.0,
            include_groups: Some(vec!["tones".into()]),
            exclude_groups: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("did you mean tone"), "{err}");
}

#[test]
fn xmp_round_trip() {
    let text = std::fs::read_to_string(fixture("sidecar.xmp")).unwrap();
    let mut s = DevelopState::from_xmp_str(&text).unwrap();
    // Loaded values are typed.
    assert_eq!(s.get("Tint"), Some(&Value::Number(8.0)));
    assert_eq!(s.get("HasCrop"), Some(&Value::Bool(true)));
    s.execute_all(&[
        set("Exposure2012", json!(1.25)),
        set("ToneCurvePV2012", json!([[0, 18], [128, 132], [255, 250]])),
        set("Name", json!([["x-default", "My edit"]])),
    ])
    .unwrap();
    let xmp = s.to_xmp();
    assert!(xmp.contains("crs:Exposure2012=\"+1.25\""), "{xmp}");
    let back = DevelopState::from_xmp_str(&xmp).unwrap();
    assert_eq!(back.settings(), s.settings());
}

#[test]
fn json_round_trip_keeps_history() {
    let mut s = DevelopState::new();
    s.execute_all(&[
        set("Exposure2012", json!(0.3)),
        set("ToneCurvePV2012Red", json!([[0, 0], [255, 240]])),
        set("Name", json!([["x-default", "Look"]])),
        set("Vibrance", json!(15)),
    ])
    .unwrap();
    s.execute(&Command::Undo).unwrap();
    let text = serde_json::to_string(&s.to_json()).unwrap();
    let mut back = DevelopState::from_json_str(&text).unwrap();
    assert_eq!(back, s);
    back.execute(&Command::Redo).unwrap();
    assert_eq!(num(&back, "Vibrance"), Some(15.0));
}

#[test]
fn save_and_load_files() {
    let dir = temp_dir("files");
    let mut s = DevelopState::new();
    s.execute(&set("Dehaze", json!(12))).unwrap();
    for name in ["state.json", "state.xmp"] {
        let path = dir.join(name);
        s.save(&path).unwrap();
        let back = DevelopState::load(&path).unwrap();
        assert_eq!(back.settings(), s.settings(), "{name}");
    }
    let err = s.save(&dir.join("photo.dng")).unwrap_err();
    assert!(err.to_string().contains("refusing"), "{err}");
}

#[test]
fn queries() {
    let mut s = DevelopState::new();
    s.execute(&set("Vibrance", json!(20))).unwrap();
    let v = s
        .query(&Query::GetParam {
            name: "Vibrance".into(),
        })
        .unwrap();
    assert_eq!(v["value"], json!(20.0));
    assert_eq!(v["effective"], json!(20.0));
    assert_eq!(v["min"], json!(-100.0));
    let v = s
        .query(&Query::GetParam {
            name: "ColorGradeBlending".into(),
        })
        .unwrap();
    assert_eq!(v["value"], json!(null));
    assert_eq!(v["effective"], json!(50));

    let d = s
        .query(&Query::Describe {
            name: "Exposure2012".into(),
        })
        .unwrap();
    assert_eq!(d["units"], json!("stops"));
    assert!(d["description"].as_str().unwrap().contains("aperture"));

    let set_only = s
        .query(&Query::ListParams {
            group: None,
            only_set: None,
        })
        .unwrap();
    assert_eq!(set_only["params"].as_array().unwrap().len(), 1);
    let hsl = s
        .query(&Query::ListParams {
            group: Some("hsl".into()),
            only_set: None,
        })
        .unwrap();
    assert_eq!(hsl["params"].as_array().unwrap().len(), 24);

    let groups = s.query(&Query::ListGroups).unwrap();
    let presence = groups["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["name"] == "presence")
        .unwrap();
    assert_eq!(presence["set"], json!(1));

    assert!(
        s.query(&Query::Describe {
            name: "Vibrence".into()
        })
        .unwrap_err()
        .to_string()
        .contains("Vibrance")
    );
}

#[test]
fn edit_workflow_writes_sidecar_and_keeps_foreign_metadata() {
    let dir = temp_dir("edit");
    let sidecar = dir.join("IMG_0001.xmp");
    std::fs::copy(fixture("sidecar.xmp"), &sidecar).unwrap();
    let cmds = r#"[
        {"cmd": "set_param", "name": "Exposure2012", "value": 0.9},
        {"cmd": "adjust_param", "name": "Contrast2012", "delta": 15},
        {"cmd": "set_param", "name": "Shadows2012", "value": 140}
    ]"#;

    // Dry run: report only.
    let report = edit::run(&EditRequest {
        sidecar: &sidecar,
        commands: cmds,
        out: None,
        dry_run: true,
    })
    .unwrap();
    assert!(report.written.is_none());
    assert!(report.to_text().contains("dry run: 3 change(s)"));
    let untouched = std::fs::read_to_string(&sidecar).unwrap();
    assert!(untouched.contains("crs:Exposure2012=\"+0.65\""));

    // Real run.
    let report = edit::run(&EditRequest {
        sidecar: &sidecar,
        commands: cmds,
        out: None,
        dry_run: false,
    })
    .unwrap();
    let text = report.to_text();
    assert!(text.contains("Exposure2012: +0.65 -> +0.9"), "{text}");
    assert!(text.contains("Contrast2012: (unset) -> +15"), "{text}");
    assert!(text.contains("clamped: Shadows2012 = 140"), "{text}");
    assert_eq!(report.written.as_deref(), Some(sidecar.as_path()));
    let json = report.to_json();
    assert_eq!(json["results"][0]["diff"][0]["after"], json!(0.9));

    let written = std::fs::read_to_string(&sidecar).unwrap();
    assert!(written.contains("xmp:Rating=\"4\""), "{written}");
    let state = DevelopState::load(&sidecar).unwrap();
    assert_eq!(num(&state, "Exposure2012"), Some(0.9));
    assert_eq!(num(&state, "Shadows2012"), Some(100.0));
    assert_eq!(num(&state, "CropAngle"), Some(1.2));

    // A failing batch writes nothing.
    let before = std::fs::read_to_string(&sidecar).unwrap();
    let err = edit::run(&EditRequest {
        sidecar: &sidecar,
        commands: r#"[{"cmd":"set_param","name":"Vibrance","value":5},{"cmd":"undo"},{"cmd":"undo"}]"#,
        out: None,
        dry_run: false,
    })
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("command 2 (undo): nothing to undo"),
        "{err}"
    );
    assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), before);

    // --out writes elsewhere, and a missing sidecar starts empty.
    let out = dir.join("copy.json");
    let fresh = dir.join("new.xmp");
    edit::run(&EditRequest {
        sidecar: &fresh,
        commands: r#"{"cmd":"set_param","name":"Texture","value":20}"#,
        out: Some(&out),
        dry_run: false,
    })
    .unwrap();
    assert!(!fresh.exists());
    assert_eq!(
        num(&DevelopState::load(&out).unwrap(), "Texture"),
        Some(20.0)
    );

    // Queries against the written sidecar.
    let answers = edit::run_queries(
        &sidecar,
        r#"[{"query":"get_param","name":"Exposure2012"},{"query":"list_params"}]"#,
    )
    .unwrap();
    assert_eq!(answers[0]["value"], json!(0.9));
    assert!(answers[1]["params"].as_array().unwrap().len() > 5);
}
