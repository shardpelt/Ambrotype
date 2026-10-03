use std::path::Path;

use flb_presets::registry::Registry;
use flb_presets::{Preset, SourceFormat, Value, curve_points, report};

fn fixture(name: &str) -> Preset {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    Preset::from_path(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn lrtemplate_fixture() {
    let p = fixture("warm-matte.lrtemplate");
    assert_eq!(p.format, SourceFormat::Lrtemplate);
    assert_eq!(p.name(), Some("Warm Matte"));
    assert_eq!(p.process_version(), Some("11.0"));
    assert_eq!(p.get("Highlights2012").unwrap().as_f64(), Some(-40.0));
    let curve = curve_points(p.get("ToneCurvePV2012").unwrap()).unwrap();
    assert_eq!(
        curve,
        vec![(0.0, 18.0), (64.0, 70.0), (192.0, 200.0), (255.0, 245.0)]
    );
    let grad = &p.get("GradientBasedCorrections").unwrap().as_seq().unwrap()[0];
    let mask = &grad.as_struct().unwrap()["CorrectionMasks"]
        .as_seq()
        .unwrap()[0];
    assert_eq!(
        mask.as_struct().unwrap()["What"].as_str(),
        Some("Mask/Gradient")
    );
}

#[test]
fn element_form_xmp_fixture() {
    let p = fixture("element-form.xmp");
    assert_eq!(p.format, SourceFormat::XmpPreset);
    assert!(p.supports_amount());
    assert_eq!(p.name(), Some("Cool Shadows"));
    assert_eq!(p.group(), Some("Flightbroom Tests"));
    assert_eq!(p.get("Exposure2012").unwrap().as_f64(), Some(-0.2));
    match p.get("Name").unwrap() {
        Value::Alt(items) => assert_eq!(items.len(), 2),
        other => panic!("expected Alt, got {other:?}"),
    }
    let r = report::inspect(&p, Registry::builtin());
    let warnings = r.warnings.join("\n");
    assert!(warnings.contains("Adobe Standard"), "{warnings}");
    assert!(warnings.contains("AI-dependent masks"), "{warnings}");
    assert!(r.unknown.is_empty(), "unknown tags: {:?}", r.unknown);
}

#[test]
fn sidecar_fixture() {
    let p = fixture("sidecar.xmp");
    assert_eq!(p.format, SourceFormat::XmpSidecar);
    assert_eq!(p.get("Temperature").unwrap().as_f64(), Some(5400.0));
    assert_eq!(p.get("Tint").unwrap().as_f64(), Some(8.0));
    // Non-Camera-Raw properties (xmp:Rating) are not part of the settings.
    assert!(p.get("Rating").is_none());
    let r = report::inspect(&p, Registry::builtin());
    assert!(r.counts["image-specific"] >= 5);
}
