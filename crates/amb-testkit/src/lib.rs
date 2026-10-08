// SPDX-License-Identifier: GPL-3.0-or-later
//! Calibration tooling for the Lightroom measurement month
//! (docs/lightroom-trial-plan.md).
//!
//! A *sweep* varies one Camera Raw setting over a set of values while every
//! other setting stays at a neutral baseline. For each input raw and each
//! value, the kit holds a copy of the raw plus an XMP sidecar carrying the
//! settings. Lightroom applies the sidecar on import, and a batch export then
//! yields one reference render per (image, setting, value).
//!
//! Lightroom reads `.xmp` sidecars only for proprietary raw formats. DNG, TIFF
//! and JPEG files carry their settings inside the file, so those inputs are
//! rejected as sidecar inputs; synthetic DNG targets ([`patterns`], written by
//! [`dng`]) carry their settings embedded instead.

pub mod dng;
pub mod patterns;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use amb_presets::xmp_writer::{format_number, write_packet};
use amb_presets::{Value, is_signed_tag};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "{0}: Lightroom ignores sidecars for DNG/TIFF/JPEG files; use proprietary raws (CR2, CR3, NEF, ARW, RAF, ORF, RW2, ...)"
    )]
    UnsupportedInput(PathBuf),
    #[error("unknown synthetic pattern {0:?} (known: ramp, hue, edges, colorchecker, negative)")]
    UnknownPattern(String),
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
}

/// One setting swept over a list of values. `extra` settings are added to every
/// step (e.g. `WhiteBalance = Custom` for a temperature sweep).
#[derive(Debug, Clone)]
pub struct Sweep {
    pub group: &'static str,
    pub tag: String,
    pub values: Vec<f64>,
    pub extra: Vec<(String, Value)>,
}

const SIGNED_STEPS: [f64; 10] = [
    -100.0, -75.0, -50.0, -25.0, -10.0, 10.0, 25.0, 50.0, 75.0, 100.0,
];
const UNSIGNED_STEPS: [f64; 6] = [5.0, 10.0, 25.0, 50.0, 75.0, 100.0];

impl Sweep {
    fn new(group: &'static str, tag: impl Into<String>, values: &[f64]) -> Sweep {
        Sweep {
            group,
            tag: tag.into(),
            values: values.to_vec(),
            extra: vec![],
        }
    }

    fn with(mut self, key: impl Into<String>, value: Value) -> Sweep {
        self.extra.push((key.into(), value));
        self
    }
}

fn signed(group: &'static str, tags: &[&str]) -> Vec<Sweep> {
    tags.iter()
        .map(|tag| Sweep::new(group, *tag, &SIGNED_STEPS))
        .collect()
}

fn unsigned(group: &'static str, tags: &[&str]) -> Vec<Sweep> {
    tags.iter()
        .map(|tag| Sweep::new(group, *tag, &UNSIGNED_STEPS))
        .collect()
}

/// The default sweep plan, ordered by priority (see docs/research/05 §19).
pub fn default_plan() -> Vec<Sweep> {
    let exposure: Vec<f64> = (-8..=8)
        .filter(|i| *i != 0)
        .map(|i| f64::from(i) * 0.5)
        .collect();
    let mut plan = vec![Sweep::new("tone", "Exposure2012", &exposure)];
    plan.extend(signed(
        "tone",
        &[
            "Contrast2012",
            "Highlights2012",
            "Shadows2012",
            "Whites2012",
            "Blacks2012",
        ],
    ));
    plan.push(
        Sweep::new(
            "white_balance",
            "Temperature",
            &[
                2500.0, 3500.0, 4500.0, 5500.0, 6500.0, 8000.0, 10000.0, 15000.0,
            ],
        )
        .with("WhiteBalance", Value::Text("Custom".into()))
        .with("Tint", Value::Number(0.0)),
    );
    plan.push(
        Sweep::new(
            "white_balance",
            "Tint",
            &[-150.0, -100.0, -50.0, -25.0, 25.0, 50.0, 100.0, 150.0],
        )
        .with("WhiteBalance", Value::Text("Custom".into()))
        .with("Temperature", Value::Number(5500.0)),
    );
    plan.extend(signed(
        "presence",
        &["Vibrance", "Saturation", "Texture", "Clarity2012", "Dehaze"],
    ));
    plan.extend(signed(
        "tone_curve",
        &[
            "ParametricShadows",
            "ParametricDarks",
            "ParametricLights",
            "ParametricHighlights",
        ],
    ));
    for colour in [
        "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
    ] {
        for kind in ["Hue", "Saturation", "Luminance"] {
            plan.push(Sweep::new(
                "hsl",
                format!("{kind}Adjustment{colour}"),
                &SIGNED_STEPS,
            ));
        }
    }
    let hues: Vec<f64> = (0..12).map(|i| f64::from(i) * 30.0).collect();
    for region in ["Shadow", "Midtone", "Highlight", "Global"] {
        let (hue, sat, lum) = (
            format!("ColorGrade{region}Hue"),
            format!("ColorGrade{region}Sat"),
            format!("ColorGrade{region}Lum"),
        );
        plan.push(
            Sweep::new("color_grading", hue.clone(), &hues).with(sat.clone(), Value::Number(50.0)),
        );
        plan.push(Sweep::new("color_grading", sat, &UNSIGNED_STEPS).with(hue, Value::Number(30.0)));
        plan.push(Sweep::new("color_grading", lum, &SIGNED_STEPS));
    }
    plan.push(
        Sweep::new(
            "color_grading",
            "ColorGradeBlending",
            &[0.0, 25.0, 75.0, 100.0],
        )
        .with("ColorGradeShadowHue", Value::Number(220.0))
        .with("ColorGradeShadowSat", Value::Number(50.0))
        .with("ColorGradeHighlightHue", Value::Number(40.0))
        .with("ColorGradeHighlightSat", Value::Number(50.0)),
    );
    plan.extend(signed(
        "calibration",
        &[
            "ShadowTint",
            "RedHue",
            "RedSaturation",
            "GreenHue",
            "GreenSaturation",
            "BlueHue",
            "BlueSaturation",
        ],
    ));
    plan.push(Sweep::new(
        "detail",
        "Sharpness",
        &[25.0, 50.0, 100.0, 150.0],
    ));
    plan.extend(unsigned(
        "detail",
        &["LuminanceSmoothing", "ColorNoiseReduction"],
    ));
    plan.extend(signed("effects", &["PostCropVignetteAmount"]));
    plan.extend(unsigned("effects", &["GrainAmount"]));
    // Lens Corrections. The profile sweeps need a real raw whose lens Lightroom
    // has a profile for; the manual ones work on any input.
    plan.extend(signed(
        "lens",
        &["LensManualDistortionAmount", "VignetteAmount"],
    ));
    plan.push(
        Sweep::new("lens", "VignetteMidpoint", &[0.0, 25.0, 75.0, 100.0])
            .with("VignetteAmount", Value::Number(100.0)),
    );
    plan.push(
        Sweep::new(
            "lens",
            "LensManualDistortionAmount",
            &[-100.0, -50.0, 50.0, 100.0],
        )
        .with("CropConstrainToWarp", Value::Number(1.0)),
    );
    for tag in ["LensProfileDistortionScale", "LensProfileVignettingScale"] {
        plan.push(
            Sweep::new("lens", tag, &[0.0, 50.0, 100.0, 150.0, 200.0])
                .with("LensProfileEnable", Value::Number(1.0)),
        );
    }
    for tag in ["DefringePurpleAmount", "DefringeGreenAmount"] {
        plan.push(Sweep::new("lens", tag, &[2.0, 5.0, 10.0, 20.0]));
    }
    plan
}

/// Neutral baseline written into every sidecar, so that Lightroom's raw defaults
/// (sharpening 40, colour NR 25, lens profile) do not contaminate measurements.
/// The process version is deliberately left out: Lightroom then uses its current
/// one, which the operator records (trial plan, week 1).
pub fn baseline() -> BTreeMap<String, Value> {
    [
        ("HasSettings", Value::Bool(true)),
        ("WhiteBalance", Value::Text("As Shot".into())),
        ("Sharpness", Value::Number(0.0)),
        ("LuminanceSmoothing", Value::Number(0.0)),
        ("ColorNoiseReduction", Value::Number(0.0)),
        ("LensProfileEnable", Value::Number(0.0)),
        ("AutoLateralCA", Value::Number(0.0)),
        ("PostCropVignetteAmount", Value::Number(0.0)),
        ("GrainAmount", Value::Number(0.0)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

const SIDECAR_IGNORED: &[&str] = &["dng", "tif", "tiff", "jpg", "jpeg", "png", "heic", "psd"];

/// One file of the kit: output name, sweep group and tag, value, full settings.
type Step<'a> = (
    String,
    &'a str,
    &'a str,
    Option<f64>,
    BTreeMap<String, Value>,
);

pub struct KitSummary {
    pub files: usize,
    pub manifest: PathBuf,
}

/// Filesystem-safe label for a value, e.g. `-0.5` → `m0.5`, `25` → `p25`.
fn value_label(v: f64) -> String {
    let s = format_number(v.abs(), false);
    if v < 0.0 {
        format!("m{s}")
    } else {
        format!("p{s}")
    }
}

fn link_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    std::fs::hard_link(src, dst).or_else(|_| std::fs::copy(src, dst).map(|_| ()))
}

/// What a kit is built from.
#[derive(Debug, Clone)]
pub enum Input {
    /// A proprietary camera raw: each step is a hard link (or copy) plus an XMP sidecar.
    Raw(PathBuf),
    /// A synthetic target from [`patterns`]: each step is a DNG with the settings embedded.
    Synthetic(String),
}

fn steps<'a>(stem: &str, plan: &'a [Sweep]) -> Vec<Step<'a>> {
    let mut steps: Vec<Step<'a>> = vec![(
        format!("{stem}__baseline"),
        "baseline",
        "",
        None,
        baseline(),
    )];
    for sweep in plan {
        for &v in &sweep.values {
            let mut settings = baseline();
            for (k, extra) in &sweep.extra {
                settings.insert(k.clone(), extra.clone());
            }
            settings.insert(sweep.tag.clone(), Value::Number(v));
            let name = format!("{stem}__{}_{}", sweep.tag, value_label(v));
            steps.push((name, sweep.group, &sweep.tag, Some(v), settings));
        }
    }
    steps
}

fn manifest_extra(settings: &BTreeMap<String, Value>, tag: &str) -> String {
    let base = baseline();
    settings
        .iter()
        .filter(|(k, _)| k.as_str() != tag && !base.contains_key(k.as_str()))
        .map(|(k, v)| {
            let text = v.as_f64().map_or_else(
                || v.as_str().unwrap_or("").to_string(),
                |n| format_number(n, false),
            );
            format!("{k}={text}")
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Write a sweep kit into `out`, plus `manifest.csv` listing every file with its
/// settings. Every input also gets one baseline step (`__baseline`).
pub fn generate(inputs: &[Input], plan: &[Sweep], out: &Path) -> Result<KitSummary, Error> {
    std::fs::create_dir_all(out).map_err(|e| Error::Io(out.to_path_buf(), e))?;
    for input in inputs {
        match input {
            Input::Raw(image) => {
                let ext = image
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if SIDECAR_IGNORED.contains(&ext.as_str()) {
                    return Err(Error::UnsupportedInput(image.clone()));
                }
            }
            Input::Synthetic(name) => {
                if !patterns::NAMES.contains(&name.as_str()) {
                    return Err(Error::UnknownPattern(name.clone()));
                }
            }
        }
    }
    let mut manifest = String::from("file,source,group,tag,value,extra\n");
    let mut files = 0;
    for input in inputs {
        match input {
            Input::Raw(image) => {
                let stem = image
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("image");
                let ext = image.extension().and_then(|e| e.to_str()).unwrap_or("raw");
                for (name, group, tag, value, settings) in steps(stem, plan) {
                    let raw = out.join(format!("{name}.{ext}"));
                    link_or_copy(image, &raw).map_err(|e| Error::Io(raw.clone(), e))?;
                    let sidecar = out.join(format!("{name}.xmp"));
                    std::fs::write(&sidecar, write_packet(&settings, is_signed_tag))
                        .map_err(|e| Error::Io(sidecar.clone(), e))?;
                    let value = value.map_or_else(String::new, |v| format_number(v, false));
                    let extra = manifest_extra(&settings, tag);
                    let _ = writeln!(
                        manifest,
                        "{name}.{ext},{},{group},{tag},{value},{extra}",
                        image.display()
                    );
                    files += 1;
                }
            }
            Input::Synthetic(pattern) => {
                let image = patterns::generate(pattern).expect("validated above");
                for (name, group, tag, value, settings) in steps(pattern, plan) {
                    let path = out.join(format!("{name}.dng"));
                    let xmp = write_packet(&settings, is_signed_tag);
                    let opts = dng::DngOptions {
                        xmp: Some(&xmp),
                        ..Default::default()
                    };
                    dng::write(&path, &image, &opts).map_err(|e| Error::Io(path.clone(), e))?;
                    let value = value.map_or_else(String::new, |v| format_number(v, false));
                    let extra = manifest_extra(&settings, tag);
                    let _ = writeln!(
                        manifest,
                        "{name}.dng,synthetic:{pattern},{group},{tag},{value},{extra}"
                    );
                    files += 1;
                }
            }
        }
    }
    let manifest_path = out.join("manifest.csv");
    std::fs::write(&manifest_path, manifest).map_err(|e| Error::Io(manifest_path.clone(), e))?;
    Ok(KitSummary {
        files,
        manifest: manifest_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_presets::Preset;

    #[test]
    fn plan_tags_are_known_and_in_range() {
        let registry = amb_presets::registry::Registry::builtin();
        for sweep in default_plan() {
            let info = registry
                .lookup(&sweep.tag)
                .unwrap_or_else(|| panic!("{} not in registry", sweep.tag));
            for v in &sweep.values {
                if let (Some(lo), Some(hi)) = (info.min, info.max) {
                    assert!(
                        (lo..=hi).contains(v),
                        "{} = {v} outside {lo}..{hi}",
                        sweep.tag
                    );
                }
            }
        }
    }

    #[test]
    fn generates_kit_and_rejects_dng() {
        let dir = std::env::temp_dir().join(format!("amb-testkit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("ramp.nef");
        std::fs::write(&raw, b"not really a raw").unwrap();
        let plan = vec![Sweep::new("tone", "Exposure2012", &[-0.5, 1.0])];
        let out = dir.join("kit");
        let summary = generate(&[Input::Raw(raw.clone())], &plan, &out).unwrap();
        assert_eq!(summary.files, 3); // baseline + 2 steps
        let sidecar = Preset::from_path(&out.join("ramp__Exposure2012_m0.5.xmp")).unwrap();
        assert_eq!(sidecar.get("Exposure2012").unwrap().as_f64(), Some(-0.5));
        assert_eq!(sidecar.get("Sharpness").unwrap().as_f64(), Some(0.0));
        let manifest = std::fs::read_to_string(&summary.manifest).unwrap();
        assert!(manifest.contains("ramp__Exposure2012_p1.nef,"));

        let dng = dir.join("x.dng");
        std::fs::write(&dng, b"").unwrap();
        assert!(matches!(
            generate(&[Input::Raw(dng)], &plan, &out),
            Err(Error::UnsupportedInput(_))
        ));

        // Synthetic targets embed the settings in the DNG.
        let summary = generate(&[Input::Synthetic("colorchecker".into())], &plan, &out).unwrap();
        assert_eq!(summary.files, 3);
        let embedded = Preset::from_path(&out.join("colorchecker__Exposure2012_p1.dng")).unwrap();
        assert_eq!(embedded.get("Exposure2012").unwrap().as_f64(), Some(1.0));
        assert!(matches!(
            generate(&[Input::Synthetic("nope".into())], &plan, &out),
            Err(Error::UnknownPattern(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
