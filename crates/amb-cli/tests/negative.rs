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
fn analyze_says_the_range_and_clipping_of_each_frame() {
    let dir = std::env::temp_dir().join(format!("amb-cli-range-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    amb(&dir, &["testimage", "negative", "neg.dng"]);
    let text = amb(&dir, &["negative", "analyze", "neg.dng"]);
    assert!(
        text.contains("range ") && text.contains("clipped "),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&amb(&dir, &["negative", "analyze", "neg.dng", "--json"])).unwrap();
    assert_eq!(json["frames"][0]["range"].as_array().unwrap().len(), 3);
    assert!(json["frames"][0]["clipped"].as_f64().unwrap() >= 0.0);
    std::fs::remove_dir_all(&dir).unwrap();
}
