use super::*;

/// A module that returns `json` whatever the input, written in the
/// WebAssembly text format; `extra` adds instructions to `amb_run`.
fn module(json: &str, extra: &str, imports: &str) -> Vec<u8> {
    let escaped: String = json.bytes().map(|b| format!("\\{b:02x}")).collect();
    wat::parse_str(format!(
        r#"(module
            {imports}
            (memory (export "memory") 2)
            (data (i32.const 1024) "{escaped}")
            (func (export "amb_alloc") (param i32) (result i32) i32.const 8192)
            (func (export "amb_run") (param i32 i32) (result i64)
              {extra}
              i64.const 1024
              i64.const 32
              i64.shl
              i64.const {len}
              i64.or))"#,
        len = json.len()
    ))
    .unwrap()
}

fn manifest(permissions: &str) -> String {
    format!(
        r#"{{ "format": "ambrotype-plugin/1", "id": "test", "name": "Test", "version": "1.0.0",
            "permissions": {permissions},
            "params": [
              {{ "name": "strength", "label": "Strength", "type": "number", "min": 0, "max": 100, "default": 50 }},
              {{ "name": "style", "label": "Style", "type": "choice", "options": ["A", "B"], "default": "A" }},
              {{ "name": "keep", "label": "Keep", "type": "bool", "default": true }} ] }}"#
    )
}

#[test]
fn runs_a_plugin_and_returns_its_edits() {
    let wasm = module(
        r#"{"commands":[{"cmd":"set_param","name":"Exposure2012","value":0.5},{"cmd":"adjust_param","name":"Contrast2012","delta":-10}],"message":"Done"}"#,
        "",
        "",
    );
    let p = Plugin::from_parts(&manifest("[]"), &wasm).unwrap();
    let out = p.run(&Context::default(), &Map::new()).unwrap();
    assert_eq!(out.commands.len(), 2);
    assert_eq!(out.commands[0].label(), "set_param Exposure2012");
    assert_eq!(out.message.as_deref(), Some("Done"));
    assert!(out.fuel_used > 0);
}

#[test]
fn the_log_import_works() {
    let wasm = module(
        r#"{"commands":[]}"#,
        "i32.const 1024 i32.const 2 call $log",
        r#"(import "amb" "log" (func $log (param i32 i32)))"#,
    );
    let p = Plugin::from_parts(&manifest("[]"), &wasm).unwrap();
    assert_eq!(
        p.run(&Context::default(), &Map::new()).unwrap().log,
        ["{\""]
    );
}

#[test]
fn refuses_other_imports_and_commands() {
    let wasm = module(
        "{}",
        "",
        r#"(import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))"#,
    );
    let err = Plugin::from_parts(&manifest("[]"), &wasm).unwrap_err();
    assert!(err.contains("fd_write"), "{err}");

    for (json, word) in [
        (
            r#"{"commands":[{"cmd":"apply_preset","path":"/etc/passwd"}]}"#,
            "apply_preset",
        ),
        (r#"{"commands":[{"cmd":"undo"}]}"#, "undo"),
        (r#"{"error":"no photo"}"#, "no photo"),
        ("not json", "isn't JSON"),
    ] {
        let p = Plugin::from_parts(&manifest("[]"), &module(json, "", "")).unwrap();
        let err = p.run(&Context::default(), &Map::new()).unwrap_err();
        assert!(err.contains(word), "{err}");
    }
}

#[test]
fn a_runaway_plugin_is_stopped() {
    let wasm = module("{}", "(loop $l br $l)", "");
    let p = Plugin::from_parts(&manifest("[]"), &wasm).unwrap();
    let err = p.run(&Context::default(), &Map::new()).unwrap_err();
    assert!(err.contains("too long"), "{err}");
}

#[test]
fn memory_is_capped() {
    // Grow by 2048 pages (128 MiB): over the cap, so memory.grow fails (-1).
    let wasm = module(
        r#"{"commands":[]}"#,
        "(if (i32.ne (memory.grow (i32.const 2048)) (i32.const -1)) (then unreachable))",
        "",
    );
    let p = Plugin::from_parts(&manifest("[]"), &wasm).unwrap();
    p.run(&Context::default(), &Map::new()).unwrap();
}

#[test]
fn input_holds_only_what_is_permitted() {
    let ctx = Context {
        settings: serde_json::json!({ "Exposure2012": 1.0 })
            .as_object()
            .cloned()
            .unwrap(),
        photo: Some(serde_json::json!({ "camera": "Nikon D750" })),
        stats: Some(Stats::from_rgb8(&[0, 0, 0], 1, 1)),
        seed: 7,
    };
    let wasm = module("{}", "", "");
    let none = Plugin::from_parts(&manifest("[]"), &wasm).unwrap();
    let input = none.input(&ctx, &Map::new()).unwrap();
    assert!(input.get("photo").is_none() && input.get("stats").is_none());
    assert_eq!(input["settings"]["Exposure2012"], 1.0);
    assert_eq!(input["params"]["strength"], 50.0);
    let all = Plugin::from_parts(&manifest(r#"["photo-info", "photo-stats"]"#), &wasm).unwrap();
    let input = all.input(&ctx, &Map::new()).unwrap();
    assert_eq!(input["photo"]["camera"], "Nikon D750");
    assert_eq!(input["stats"]["luma"][0], 1);
    // A different permission list is a different fingerprint.
    assert_ne!(none.fingerprint(), all.fingerprint());
}

#[test]
fn parameters_are_checked_and_clamped() {
    let m = Manifest::from_json(&manifest("[]")).unwrap();
    let given = serde_json::json!({ "strength": "250", "style": "B", "keep": false });
    let p = m.resolve_params(given.as_object().unwrap()).unwrap();
    assert_eq!(
        (
            p["strength"].as_f64(),
            p["style"].as_str(),
            p["keep"].as_bool()
        ),
        (Some(100.0), Some("B"), Some(false))
    );
    for bad in [
        r#"{"style": "C"}"#,
        r#"{"nope": 1}"#,
        r#"{"strength": "x"}"#,
    ] {
        let v: Value = serde_json::from_str(bad).unwrap();
        assert!(m.resolve_params(v.as_object().unwrap()).is_err(), "{bad}");
    }
    for bad in [
        r#"{"format":"ambrotype-plugin/1","id":"Bad Id","name":"x","version":"1"}"#,
        r#"{"format":"ambrotype-plugin/1","id":"x","name":"x","version":"1","wasm":"../x.wasm"}"#,
        r#"{"format":"ambrotype-plugin/1","id":"x","name":"x","version":"1","permissions":["network"]}"#,
        r#"{"format":"ambrotype-plugin/1","id":"x","name":"x","version":"1","params":[{"name":"a","label":"A","type":"number","min":5,"max":1,"default":2}]}"#,
    ] {
        assert!(Manifest::from_json(bad).is_err(), "{bad}");
    }
}

/// The example plugins in the repo, as built (`amb plugin build plugins/<id>`).
#[test]
fn the_example_plugins_run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let random = Plugin::load(&root.join("random-look")).unwrap();
    let ctx = Context {
        seed: 42,
        ..Context::default()
    };
    let params = serde_json::json!({ "style": "Film" });
    let out = random.run(&ctx, params.as_object().unwrap()).unwrap();
    assert!(out.commands.len() >= 7, "{:?}", out.commands);
    assert!(out.message.unwrap().starts_with("Film look"));

    let auto = Plugin::load(&root.join("auto-tone")).unwrap();
    assert_eq!(auto.manifest.permissions, [Permission::PhotoStats]);
    // A dark, flat photo.
    let rgb: Vec<u8> = (20..60u8).flat_map(|v| [v, v, v]).collect();
    let ctx = Context {
        stats: Some(Stats::from_rgb8(&rgb, 40, 1)),
        ..Context::default()
    };
    let out = auto.run(&ctx, &Map::new()).unwrap();
    let first = out.commands[0].label();
    assert!(
        first.starts_with("adjust_param Exposure2012 +"),
        "{first}: {:?}",
        out.message
    );
    // Without statistics it explains itself.
    let err = auto.run(&Context::default(), &Map::new()).unwrap_err();
    assert!(err.contains("photo-stats"), "{err}");
}

#[test]
fn sidecar_text_becomes_numbers() {
    use amb_presets::Value as V;
    let mut s = std::collections::BTreeMap::new();
    s.insert("Exposure2012".to_string(), V::Text("+0.35".into()));
    s.insert("AutoLateralCA".to_string(), V::Text("True".into()));
    s.insert("WhiteBalance".to_string(), V::Text("As Shot".into()));
    let j = Context::settings_json(&s);
    assert_eq!(
        (
            j["Exposure2012"].as_f64(),
            j["AutoLateralCA"].as_bool(),
            j["WhiteBalance"].as_str()
        ),
        (Some(0.35), Some(true), Some("As Shot"))
    );
}
