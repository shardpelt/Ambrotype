// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb negative` end to end: a roll of synthetic negatives gets one
//! conversion, written to each sidecar with Auto off; stocks list their grain;
//! a harder paper grade prints more contrast.

use std::process::Command;

fn amb(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "amb {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_roll_gets_one_conversion() {
    let dir = std::env::temp_dir().join(format!("amb-cli-roll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["a", "b", "c"] {
        amb(&dir, &["testimage", "negative", &format!("{n}.dng")]);
    }
    let plan: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &["negative", "sync", "a.dng", "b.dng", "c.dng", "--json"],
    ))
    .unwrap();
    let frames = plan["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    assert!(
        frames.iter().all(|f| f["scene"] == 0 && f["odd"] == false),
        "{plan}"
    );
    assert_eq!(frames[0]["settings"], frames[2]["settings"]);
    assert!(plan["written"].as_array().unwrap().is_empty());

    let text = amb(
        &dir,
        &[
            "negative",
            "sync",
            "a.dng",
            "b.dng",
            "c.dng",
            "--baseline",
            "b.dng",
            "--apply",
        ],
    );
    assert!(text.contains("wrote 3 sidecar(s)"), "{text}");
    for n in ["a", "b", "c"] {
        let xmp = std::fs::read_to_string(dir.join(format!("{n}.xmp"))).unwrap();
        assert!(xmp.contains("amb:NegativeAuto=\"False\""), "{xmp}");
        assert!(xmp.contains("amb:NegativeDensityHighGreen="), "{xmp}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stocks_show_their_grain_and_glow() {
    let dir = std::env::temp_dir();
    let text = amb(&dir, &["negative", "stocks"]);
    assert!(text.contains("halation 30"), "{text}");
    let json: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["negative", "stocks", "--json"])).unwrap();
    let cinema = json
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "cinema-500t")
        .unwrap();
    assert_eq!(cinema["halation"], 30.0);
    assert_eq!(cinema["grain_amount"], 28.0);
}

/// The spread (standard deviation) of a JPEG's grey values.
fn spread(path: &std::path::Path) -> f64 {
    let img = image::open(path).unwrap().to_luma8();
    let n = img.pixels().len() as f64;
    let mean = img.pixels().map(|p| f64::from(p[0])).sum::<f64>() / n;
    (img.pixels()
        .map(|p| (f64::from(p[0]) - mean).powi(2))
        .sum::<f64>()
        / n)
        .sqrt()
}

#[test]
fn a_harder_paper_grade_prints_more_contrast() {
    let dir = std::env::temp_dir().join(format!("amb-cli-paper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "negative", "neg.dng"]);
    let mut spreads = Vec::new();
    for grade in [1, 4] {
        let preset = format!("grade{grade}.xmp");
        std::fs::write(
            dir.join(&preset),
            format!(
                r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:amb="http://ns.ambrotype.org/develop/1.0/"
 crs:PresetType="Normal" amb:NegativeConvert="True" amb:PaperCurve="True" amb:PaperGrade="{grade}"/>
</rdf:RDF></x:xmpmeta>"#
            ),
        )
        .unwrap();
        let out = format!("grade{grade}.jpg");
        amb(&dir, &["render", "neg.dng", &out, "--preset", &preset]);
        spreads.push(spread(&dir.join(&out)));
    }
    assert!(spreads[1] > spreads[0] * 1.1, "{spreads:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn probe_reads_range_clipping_and_zone() {
    let dir = std::env::temp_dir().join(format!("amb-cli-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "negative", "neg.dng"]);
    std::fs::write(
        dir.join("on.xmp"),
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:amb="http://ns.ambrotype.org/develop/1.0/" crs:PresetType="Normal" amb:NegativeConvert="True"/></rdf:RDF></x:xmpmeta>"#,
    )
    .unwrap();
    let args = |extra: &[&'static str]| {
        let mut a = vec!["negative", "probe", "neg.dng", "--preset", "on.xmp"];
        a.extend_from_slice(extra);
        a
    };
    let text = amb(&dir, &args(&["--at", "0.5,0.5"]));
    assert!(
        text.contains("range ") && text.contains("clipped at scan"),
        "{text}"
    );
    assert!(
        text.contains("zone ") && text.contains("density "),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&amb(&dir, &args(&["--at", "0.5,0.5", "--json"]))).unwrap();
    assert_eq!(json["stats"]["range"].as_array().unwrap().len(), 3);
    assert!(json["stats"]["clipped"].as_f64().unwrap() >= 0.0);
    assert!(json["probe"]["zone"].as_u64().unwrap() <= 10);
    assert_eq!(json["probe"]["density"].as_array().unwrap().len(), 3);
    // Without the conversion on there is nothing to read.
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(["negative", "probe", "neg.dng", "--at", "0.5,0.5"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    // Analyze says the same range and clipping per frame.
    let analyze = amb(&dir, &["negative", "analyze", "neg.dng"]);
    assert!(
        analyze.contains("range ") && analyze.contains("clipped "),
        "{analyze}"
    );
    let j: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["negative", "analyze", "neg.dng", "--json"])).unwrap();
    assert_eq!(j["frames"][0]["range"].as_array().unwrap().len(), 3);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn strip_and_ring_write_a_sheet_and_name_each_cell() {
    let dir = std::env::temp_dir().join(format!("amb-cli-aids-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "negative", "neg.dng"]);
    std::fs::write(
        dir.join("neg.xmp"),
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:amb="http://ns.ambrotype.org/develop/1.0/" crs:PresetType="Normal" amb:NegativeConvert="True"/></rdf:RDF></x:xmpmeta>"#,
    )
    .unwrap();
    let strip: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "negative",
            "strip",
            "neg.dng",
            "--out",
            "strip.png",
            "--size",
            "200",
            "--steps",
            "5",
            "--json",
        ],
    ))
    .unwrap();
    assert_eq!(strip["cells"].as_array().unwrap().len(), 5);
    assert_eq!(strip["cells"][2]["label"], "As now");
    assert!(dir.join("strip.png").metadata().unwrap().len() > 100);
    let ring: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "negative", "ring", "neg.dng", "--out", "ring.png", "--size", "100", "--json",
        ],
    ))
    .unwrap();
    assert_eq!(ring["cells"].as_array().unwrap().len(), 9);
    assert!(dir.join("ring.png").metadata().unwrap().len() > 100);
    let text = amb(
        &dir,
        &[
            "negative", "ring", "neg.dng", "--out", "ring.png", "--size", "100",
        ],
    );
    assert!(
        text.contains("More red") && text.contains("amb:NegativeCyanRed="),
        "{text}"
    );
    // Pick Better: a step down and a step up in the negative's exposure.
    let pick: serde_json::Value = serde_json::from_str(&amb(
        &dir,
        &[
            "negative", "pick", "neg.dng", "--out", "pick.png", "--size", "100", "--step", "0.5",
            "--json",
        ],
    ))
    .unwrap();
    assert_eq!(pick["cells"].as_array().unwrap().len(), 2);
    assert_eq!(pick["cells"][0]["changes"]["amb:NegativeExposure"], -0.5);
    assert_eq!(pick["cells"][1]["changes"]["amb:NegativeExposure"], 0.5);
    assert!(dir.join("pick.png").metadata().unwrap().len() > 100);
    // Without the conversion on there is nothing to go round.
    std::fs::remove_file(dir.join("neg.xmp")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(["negative", "ring", "neg.dng", "--out", "x.png"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("switch on Film Negative"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn place_finds_the_exposure_for_a_zone_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("amb-cli-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "ramp", "ramp.dng"]);
    let args = |zone: &'static str| {
        [
            "negative", "place", "ramp.dng", "--at", "0.5,0.9", "--zone", zone, "--size", "256",
        ]
    };
    let mut json_args = args("VII").to_vec();
    json_args.push("--json");
    let p: serde_json::Value = serde_json::from_str(&amb(&dir, &json_args)).unwrap();
    assert_eq!(p["setting"], "Exposure2012");
    assert!((p["zone_after"].as_f64().unwrap() - 7.0).abs() < 0.2, "{p}");
    assert_eq!(p["changes"]["Exposure2012"], p["to"]);
    // Roman and number name the same zone; the text says from, to and zones.
    let text = amb(&dir, &args("7"));
    assert!(
        text.contains("Exposure2012") && text.contains("-> VII"),
        "{text}"
    );
    assert!(!dir.join("ramp.xmp").exists());
    let out = Command::new(env!("CARGO_BIN_EXE_amb"))
        .args(args("12"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("0 to 10"));
    std::fs::remove_dir_all(&dir).unwrap();
}
