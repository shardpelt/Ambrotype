// SPDX-License-Identifier: GPL-3.0-or-later
//! The editor's panel layout: Lightroom-like panels and sections, with each
//! control's range, default, label and help text taken from the settings
//! registry and descriptions (so the UI never drifts from the engine).

use amb_presets::registry::Registry;
use amb_protocol::catalog::Catalog;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Control {
    pub name: String,
    pub label: String,
    /// "number", "bool" or "choice".
    pub kind: &'static str,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: f64,
    pub default: Option<serde_json::Value>,
    pub units: Option<String>,
    pub description: String,
    /// For choices: (stored value, label).
    pub options: Vec<(serde_json::Value, String)>,
    /// The setting this control drives instead on a rendered image (JPEG
    /// and the like): Temp and Tint become Lightroom's relative sliders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<Box<Control>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub label: String,
    /// Layout hint for the UI: "sliders" or "hsl" (bands × hue/sat/lum tabs).
    pub layout: &'static str,
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Panel {
    pub id: &'static str,
    pub label: &'static str,
    /// Registry groups that "reset panel" clears.
    pub groups: Vec<&'static str>,
    pub sections: Vec<Section>,
}

const BANDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];

fn choice_options(name: &str) -> Vec<(serde_json::Value, String)> {
    use serde_json::json;
    match name {
        "WhiteBalance" => vec![
            (json!("As Shot"), "As Shot".into()),
            (json!("Custom"), "Custom".into()),
        ],
        "PostCropVignetteStyle" => vec![
            (json!(1), "Highlight Priority".into()),
            (json!(2), "Color Priority".into()),
            (json!(3), "Paint Overlay".into()),
        ],
        // Filled per photo by the UI with the profiles found for its camera.
        "CameraProfile" => vec![(json!(""), "Default (Ambrotype)".into())],
        "PerspectiveUpright" => vec![
            (json!(0), "Off".into()),
            (json!(1), "Auto".into()),
            (json!(2), "Level".into()),
            (json!(3), "Vertical".into()),
            (json!(4), "Full".into()),
        ],
        "amb:NegativeType" => vec![
            (json!("color"), "Colour".into()),
            (json!("bw"), "Black & white".into()),
        ],
        "amb:FilmStock" => amb_engine::negative::STOCKS
            .iter()
            .map(|s| (json!(s.id), s.name.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn control(name: &str, label_override: Option<&str>) -> Control {
    let registry = Registry::builtin();
    let catalog = Catalog::builtin();
    let info = registry
        .lookup(name)
        .unwrap_or_else(|| panic!("{name} is not in the registry"));
    let text = catalog.text(name);
    let options = choice_options(name);
    let kind = if !options.is_empty() {
        "choice"
    } else if info.kind == "bool" {
        "bool"
    } else {
        "number"
    };
    let span = match (info.min, info.max) {
        (Some(a), Some(b)) => b - a,
        _ => 100.0,
    };
    let step = match name {
        "PerspectiveRotate" => 0.1,
        _ if span <= 10.0 => 0.01,
        _ => 1.0,
    };
    Control {
        name: name.to_string(),
        label: label_override
            .map(str::to_string)
            .or_else(|| text.map(|t| t.label.clone()))
            .unwrap_or_else(|| name.to_string()),
        kind,
        min: info.min,
        max: info.max,
        step,
        default: info.default.clone().filter(|d| !d.is_null()),
        units: text.and_then(|t| t.units.clone()),
        description: text.map(|t| t.description.clone()).unwrap_or_default(),
        options,
        rendered: match name {
            "Temperature" => Some(Box::new(control("IncrementalTemperature", label_override))),
            "Tint" => Some(Box::new(control("IncrementalTint", label_override))),
            _ => None,
        },
    }
}

fn section(label: &str, names: &[(&str, Option<&str>)]) -> Section {
    Section {
        label: label.to_string(),
        layout: "sliders",
        controls: names.iter().map(|(n, l)| control(n, *l)).collect(),
    }
}

fn plain<'a>(names: &[&'a str]) -> Vec<(&'a str, Option<&'a str>)> {
    names.iter().map(|n| (*n, None)).collect()
}

/// The full editor layout.
pub fn panels() -> Vec<Panel> {
    let hsl_names: Vec<String> = ["Hue", "Saturation", "Luminance"]
        .iter()
        .flat_map(|k| BANDS.iter().map(move |b| format!("{k}Adjustment{b}")))
        .collect();
    let grading = |region: &str| {
        let names = ["Hue", "Sat", "Lum"].map(|k| format!("ColorGrade{region}{k}"));
        section(
            region,
            &[
                (names[0].as_str(), Some("Hue")),
                (names[1].as_str(), Some("Saturation")),
                (names[2].as_str(), Some("Luminance")),
            ],
        )
    };
    vec![
        Panel {
            id: "basic",
            label: "Basic",
            groups: vec!["white_balance", "tone", "presence"],
            sections: vec![
                section("Profile", &[("CameraProfile", Some("Profile"))]),
                section(
                    "White Balance",
                    &[
                        ("WhiteBalance", Some("WB")),
                        ("Temperature", Some("Temp")),
                        ("Tint", None),
                    ],
                ),
                section(
                    "Tone",
                    &plain(&[
                        "Exposure2012",
                        "Contrast2012",
                        "Highlights2012",
                        "Shadows2012",
                        "Whites2012",
                        "Blacks2012",
                    ]),
                ),
                section(
                    "Presence",
                    &plain(&["Texture", "Clarity2012", "Dehaze", "Vibrance", "Saturation"]),
                ),
            ],
        },
        Panel {
            id: "tone_curve",
            label: "Tone Curve",
            groups: vec!["tone_curve"],
            sections: vec![section(
                "Parametric",
                &plain(&[
                    "ParametricHighlights",
                    "ParametricLights",
                    "ParametricDarks",
                    "ParametricShadows",
                    "ParametricShadowSplit",
                    "ParametricMidtoneSplit",
                    "ParametricHighlightSplit",
                ]),
            )],
        },
        Panel {
            id: "color_mixer",
            label: "Color Mixer",
            groups: vec!["hsl"],
            sections: vec![Section {
                label: "HSL".into(),
                layout: "hsl",
                controls: hsl_names.iter().map(|n| control(n, None)).collect(),
            }],
        },
        Panel {
            id: "black_white",
            label: "Black & White",
            groups: vec!["bw_mixer"],
            sections: vec![
                section(
                    "Treatment",
                    &[("ConvertToGrayscale", Some("Black & white"))],
                ),
                Section {
                    label: "Mix".into(),
                    layout: "sliders",
                    controls: BANDS
                        .iter()
                        .map(|b| control(&format!("GrayMixer{b}"), Some(b)))
                        .collect(),
                },
            ],
        },
        Panel {
            id: "color_grading",
            label: "Color Grading",
            groups: vec!["color_grading", "split_toning"],
            sections: vec![
                grading("Shadow"),
                grading("Midtone"),
                grading("Highlight"),
                grading("Global"),
                section(
                    "Mix",
                    &[
                        ("ColorGradeBlending", Some("Blending")),
                        ("SplitToningBalance", Some("Balance")),
                    ],
                ),
            ],
        },
        Panel {
            id: "detail",
            label: "Detail",
            groups: vec!["detail"],
            sections: vec![
                section(
                    "Sharpening",
                    &[
                        ("Sharpness", Some("Amount")),
                        ("SharpenRadius", Some("Radius")),
                        ("SharpenDetail", Some("Detail")),
                        ("SharpenEdgeMasking", Some("Masking")),
                    ],
                ),
                section(
                    "Noise Reduction",
                    &[
                        ("LuminanceSmoothing", Some("Luminance")),
                        ("LuminanceNoiseReductionDetail", Some("Detail")),
                        ("LuminanceNoiseReductionContrast", Some("Contrast")),
                        ("ColorNoiseReduction", Some("Color")),
                        ("ColorNoiseReductionDetail", Some("Color Detail")),
                        ("ColorNoiseReductionSmoothness", Some("Smoothness")),
                    ],
                ),
            ],
        },
        Panel {
            id: "lens",
            label: "Lens Corrections",
            groups: vec!["lens"],
            sections: vec![
                section(
                    "Profile",
                    &[
                        ("LensProfileEnable", Some("Enable Profile Corrections")),
                        ("AutoLateralCA", Some("Remove Chromatic Aberration")),
                        ("LensProfileDistortionScale", Some("Distortion")),
                        ("LensProfileVignettingScale", Some("Vignetting")),
                    ],
                ),
                section(
                    "Manual",
                    &[
                        ("LensManualDistortionAmount", Some("Distortion")),
                        ("CropConstrainToWarp", Some("Constrain Crop")),
                        ("VignetteAmount", Some("Vignetting")),
                        ("VignetteMidpoint", Some("Midpoint")),
                    ],
                ),
                section(
                    "Defringe",
                    &[
                        ("DefringePurpleAmount", Some("Purple Amount")),
                        ("DefringePurpleHueLo", Some("Purple Hue (low)")),
                        ("DefringePurpleHueHi", Some("Purple Hue (high)")),
                        ("DefringeGreenAmount", Some("Green Amount")),
                        ("DefringeGreenHueLo", Some("Green Hue (low)")),
                        ("DefringeGreenHueHi", Some("Green Hue (high)")),
                    ],
                ),
            ],
        },
        Panel {
            id: "transform",
            label: "Transform",
            groups: vec!["transform"],
            sections: vec![
                section("Upright", &[("PerspectiveUpright", Some("Upright"))]),
                section(
                    "Transform",
                    &[
                        ("PerspectiveVertical", Some("Vertical")),
                        ("PerspectiveHorizontal", Some("Horizontal")),
                        ("PerspectiveRotate", Some("Rotate")),
                        ("PerspectiveAspect", Some("Aspect")),
                        ("PerspectiveScale", Some("Scale")),
                        ("PerspectiveX", Some("Offset X")),
                        ("PerspectiveY", Some("Offset Y")),
                    ],
                ),
            ],
        },
        Panel {
            id: "effects",
            label: "Effects",
            groups: vec!["effects"],
            sections: vec![
                section(
                    "Post-Crop Vignetting",
                    &[
                        ("PostCropVignetteStyle", Some("Style")),
                        ("PostCropVignetteAmount", Some("Amount")),
                        ("PostCropVignetteMidpoint", Some("Midpoint")),
                        ("PostCropVignetteRoundness", Some("Roundness")),
                        ("PostCropVignetteFeather", Some("Feather")),
                        ("PostCropVignetteHighlightContrast", Some("Highlights")),
                    ],
                ),
                section(
                    "Grain",
                    &[
                        ("GrainAmount", Some("Amount")),
                        ("GrainSize", Some("Size")),
                        ("GrainFrequency", Some("Roughness")),
                    ],
                ),
            ],
        },
        Panel {
            id: "calibration",
            label: "Calibration",
            groups: vec!["calibration"],
            sections: vec![
                section("Shadows", &[("ShadowTint", Some("Tint"))]),
                section(
                    "Red Primary",
                    &[
                        ("RedHue", Some("Hue")),
                        ("RedSaturation", Some("Saturation")),
                    ],
                ),
                section(
                    "Green Primary",
                    &[
                        ("GreenHue", Some("Hue")),
                        ("GreenSaturation", Some("Saturation")),
                    ],
                ),
                section(
                    "Blue Primary",
                    &[
                        ("BlueHue", Some("Hue")),
                        ("BlueSaturation", Some("Saturation")),
                    ],
                ),
            ],
        },
        Panel {
            id: "negative",
            label: "Film Negative",
            groups: vec!["negative"],
            sections: vec![
                section(
                    "Conversion",
                    &[
                        ("amb:NegativeConvert", Some("Convert negative")),
                        ("amb:NegativeAuto", Some("Auto")),
                        ("amb:NegativeType", Some("Type")),
                        ("amb:FilmStock", Some("Film stock")),
                        ("amb:FilmStockAmount", Some("Stock amount")),
                    ],
                ),
                section(
                    "Print",
                    &[
                        ("amb:NegativeExposure", Some("Exposure")),
                        ("amb:NegativeContrast", Some("Contrast")),
                        ("amb:NegativeToe", Some("Toe")),
                        ("amb:NegativeShoulder", Some("Shoulder")),
                        ("amb:NegativeSaturation", Some("Saturation")),
                        ("amb:NegativeCyanRed", Some("Cyan ↔ Red")),
                        ("amb:NegativeMagentaGreen", Some("Magenta ↔ Green")),
                        ("amb:NegativeYellowBlue", Some("Yellow ↔ Blue")),
                    ],
                ),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_is_rendered_by_the_engine() {
        for panel in panels() {
            for section in &panel.sections {
                for c in &section.controls {
                    assert!(
                        amb_engine::pipeline::is_rendered(&c.name),
                        "{} in {} is not rendered",
                        c.name,
                        panel.label
                    );
                    if c.kind == "number" {
                        assert!(
                            c.min.is_some() && c.max.is_some(),
                            "{} has no range",
                            c.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn layout_serialises() {
        let json = serde_json::to_value(panels()).unwrap();
        assert!(json.as_array().unwrap().len() >= 8);
        let basic = &json[0]["sections"][2]["controls"][0];
        assert_eq!(basic["name"], "Exposure2012");
        assert_eq!(basic["step"], 0.01);
    }
}
