// SPDX-License-Identifier: GPL-3.0-or-later
//! Running a [`Plan`] on the GPU: one kernel per stage, in the CPU
//! pipeline's order, on one image buffer that stays on the GPU; a stage
//! with no kernel yet runs on the CPU in between.

use amb_engine::buffer::Image;
use amb_engine::color::ColorPlan;
use amb_engine::pipeline::BaseLook;
use amb_engine::plan::{Plan, Step};
use bytemuck::{Pod, Zeroable};

use crate::device::{Gpu, Kernel};

const COMMON: &str = include_str!("shaders/common.wgsl");

/// Marks an unused table in the shaders.
const NONE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Size {
    width: u32,
    height: u32,
    pixels: u32,
    lut_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MatrixParams {
    m: [f32; 9],
    gain: f32,
    unclip: u32,
    clip: [f32; 3],
    gains: [f32; 3],
    reach: u32,
    tint: u32,
    tint_stops: f32,
    tint_end: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ToneParams {
    base: u32,
    lum: [f32; 3],
    tone_lut: u32,
    curves: [u32; 5],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ColorParams {
    to_lms: [f32; 9],
    from_lms: [f32; 9],
    m2: [f32; 9],
    m2_inv: [f32; 9],
    k: [f32; 5],
    vibrance: f32,
    saturation: f32,
    band_hue: [f32; 8],
    band_amount: [f32; 32],
    grayscale: u32,
    orange_index: u32,
    grade_dirs: [f32; 8],
    grade_amounts: [f32; 8],
    pivot: f32,
    width: f32,
}

/// Every stage's compiled kernels.
pub(crate) struct Kernels {
    pub(crate) local: crate::local::LocalKernels,
    unclip_weights: Kernel,
    matrix: Kernel,
    base_look: Kernel,
    tone_curve: Kernel,
    curves: Kernel,
    color: Kernel,
}

impl Kernels {
    pub(crate) fn new(gpu: &Gpu) -> Kernels {
        let module = |name: &str, src: &str| format!("{COMMON}\n// ---- {name}\n{src}");
        let matrix = module("matrix", include_str!("shaders/matrix.wgsl"));
        let tone = module("tone", include_str!("shaders/tone.wgsl"));
        let color = module("color", include_str!("shaders/color.wgsl"));
        // Bindings: camera, image, weights, size, parameters, tables.
        const MATRIX: &[bool] = &[true, false, false, true, true, true];
        // Bindings: image, size, parameters, tables.
        const IMAGE: &[bool] = &[false, true, true, true];
        Kernels {
            local: crate::local::LocalKernels::new(gpu, COMMON),
            unclip_weights: Kernel::new(gpu, "unclip_weights", &matrix, "unclip_weights", MATRIX),
            matrix: Kernel::new(gpu, "matrix", &matrix, "matrix", MATRIX),
            base_look: Kernel::new(gpu, "base_look", &tone, "base_look", IMAGE),
            tone_curve: Kernel::new(gpu, "tone_curve", &tone, "tone_curve", IMAGE),
            curves: Kernel::new(gpu, "curves", &tone, "curves", IMAGE),
            color: Kernel::new(gpu, "color", &color, "color", IMAGE),
        }
    }
}

fn flat(m: &[[f32; 3]; 3]) -> [f32; 9] {
    std::array::from_fn(|i| m[i / 3][i % 3])
}

impl Gpu {
    /// Develop `plan`: its GPU steps here, its CPU steps (stages with no GPU
    /// version yet) on the CPU in between, in order.
    pub fn render(&self, plan: &Plan) -> Image {
        let k = self.kernels();
        let pixels = plan.width * plan.height;
        let size = self.storage(&Size {
            width: plan.width as u32,
            height: plan.height as u32,
            pixels: pixels as u32,
            lut_size: plan.lut_size as u32,
        });
        let img = self.buffer(pixels * 3);
        let n = pixels;
        let mut enc = self.device.create_command_encoder(&Default::default());
        for step in &plan.steps {
            match step {
                Step::Matrix {
                    m,
                    gain,
                    unclip,
                    shadow_tint,
                } => {
                    let camera = self.upload(&plan.camera_rgb);
                    let weights = self.buffer(if unclip.is_some() { pixels * 3 } else { 1 });
                    let (tint, tint_stops, tint_end) = match shadow_tint {
                        Some((s, e)) => (1, *s, *e),
                        None => (0, 0.0, 1.0),
                    };
                    let params = self.storage(&MatrixParams {
                        m: flat(m),
                        gain: *gain,
                        unclip: unclip.is_some() as u32,
                        clip: unclip.map_or([0.0; 3], |u| u.clip),
                        gains: unclip.map_or([1.0; 3], |u| u.gains),
                        reach: unclip.map_or(0, |u| u.reach as u32),
                        tint,
                        tint_stops,
                        tint_end,
                    });
                    let luts = self.upload(&[0.0]);
                    let bindings = [&camera, &img, &weights, &size, &params, &luts];
                    if unclip.is_some() {
                        k.unclip_weights.dispatch(self, &mut enc, &bindings, n);
                    }
                    k.matrix.dispatch(self, &mut enc, &bindings, n);
                }
                Step::BaseLook(look) => {
                    if *look == BaseLook::Default {
                        let params = self.storage(&ToneParams {
                            base: 1,
                            lum: [0.0; 3],
                            tone_lut: NONE,
                            curves: [NONE; 5],
                        });
                        let luts = self.upload(&[0.0]);
                        k.base_look
                            .dispatch(self, &mut enc, &[&img, &size, &params, &luts], n);
                    }
                }
                Step::HighlightsShadows(p) => {
                    crate::local::Planes::new(self, &mut enc).highlights_shadows(
                        &img,
                        plan.width,
                        plan.height,
                        p,
                    );
                }
                Step::Local(p) => {
                    crate::local::Planes::new(self, &mut enc).local_contrast(
                        &img,
                        plan.width,
                        plan.height,
                        p,
                    );
                }
                Step::ToneCurve { lut, lum_weights } => {
                    let params = self.storage(&ToneParams {
                        base: 0,
                        lum: *lum_weights,
                        tone_lut: 0,
                        curves: [NONE; 5],
                    });
                    let luts = self.upload(lut);
                    k.tone_curve
                        .dispatch(self, &mut enc, &[&img, &size, &params, &luts], n);
                }
                Step::Color(cp) => {
                    let params = self.storage(&color_params(cp));
                    let luts = self.upload(&[0.0]);
                    k.color
                        .dispatch(self, &mut enc, &[&img, &size, &params, &luts], n);
                }
                Step::Curves(c) => {
                    // All the tables in one buffer, with their offsets.
                    let mut luts: Vec<f32> = Vec::new();
                    let mut place = |t: &Option<Vec<f32>>| match t {
                        Some(t) => {
                            let at = luts.len() as u32;
                            luts.extend_from_slice(t);
                            at
                        }
                        None => NONE,
                    };
                    let curves = [
                        place(&c.parametric),
                        place(&c.rgb),
                        place(&c.channels[0]),
                        place(&c.channels[1]),
                        place(&c.channels[2]),
                    ];
                    let params = self.storage(&ToneParams {
                        base: 0,
                        lum: [0.0; 3],
                        tone_lut: NONE,
                        curves,
                    });
                    let luts = self.upload(&luts);
                    k.curves
                        .dispatch(self, &mut enc, &[&img, &size, &params, &luts], n);
                }
                Step::Cpu { run, .. } => {
                    // Finish what's queued, bring the image back, run the
                    // engine's stage on it and send it up again.
                    let done = std::mem::replace(
                        &mut enc,
                        self.device.create_command_encoder(&Default::default()),
                    );
                    self.queue.submit([done.finish()]);
                    let mut image = Image {
                        width: plan.width,
                        height: plan.height,
                        rgb: self.download(&img, pixels * 3),
                    };
                    run(&mut image);
                    self.queue
                        .write_buffer(&img, 0, bytemuck::cast_slice(&image.rgb));
                }
            }
        }
        self.queue.submit([enc.finish()]);
        Image {
            width: plan.width,
            height: plan.height,
            rgb: self.download(&img, pixels * 3),
        }
    }
}

fn color_params(cp: &ColorPlan) -> ColorParams {
    let b = cp.bands;
    ColorParams {
        to_lms: flat(&cp.to_lms),
        from_lms: flat(&cp.from_lms),
        m2: flat(&cp.m2),
        m2_inv: flat(&cp.m2_inv),
        k: [
            cp.k.hsl_hue_deg,
            cp.k.hsl_lum,
            cp.k.vibrance,
            cp.k.grade_tint,
            cp.k.grade_lum,
        ],
        vibrance: cp.vibrance,
        saturation: cp.saturation,
        band_hue: b.map(|(h, _)| h),
        band_amount: std::array::from_fn(|i| b[i / 4].1[i % 4]),
        grayscale: cp.grayscale as u32,
        orange_index: cp.orange_index as u32,
        grade_dirs: std::array::from_fn(|i| cp.grade_dirs[i / 2][i % 2]),
        grade_amounts: std::array::from_fn(|i| cp.grade_amounts[i / 2][i % 2]),
        pivot: cp.pivot,
        width: cp.width,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use amb_engine::pipeline::{self, DevelopSettings};
    use amb_engine::raw::CameraImage;
    use amb_presets::Value;

    use crate::device::tests::gpu;

    /// A synthetic raw: gradients, hard edges, fine texture, saturated
    /// colours and a clipped patch, with a camera-like matrix and white.
    pub(crate) fn photo(w: usize, h: usize) -> CameraImage {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                let fine = ((x * 7 + y * 13) % 17) as f32 / 17.0 * 0.05;
                let mut p = [
                    0.02 + 0.6 * u * u + fine,
                    0.03 + 0.5 * v + 0.2 * (u * 9.0).sin().abs(),
                    0.05 + 0.4 * (1.0 - u) * v + fine,
                ];
                if (x / 8 + y / 8) % 3 == 0 {
                    p = [p[2] * 1.5, p[0] * 0.5, p[1]];
                }
                if u > 0.75 && v < 0.3 {
                    // Clipped in green and blue, not red.
                    p = [0.85, 1.0, 1.0];
                }
                rgb.extend_from_slice(&p);
            }
        }
        CameraImage {
            width: w,
            height: h,
            rgb,
            // A D750-like camera matrix (XYZ to camera) and daylight white.
            xyz_to_cam: [
                [0.7186, -0.2108, -0.0606],
                [-0.4386, 1.2001, 0.2584],
                [-0.0577, 0.1271, 0.6585],
            ],
            as_shot_neutral: [0.45, 1.0, 0.62],
            clip: [1.0, 1.0, 1.0],
            rendered: false,
            make: "Test".into(),
            model: "Synthetic".into(),
            shot: Default::default(),
            metadata: Default::default(),
        }
    }

    pub(crate) fn settings(tags: &[(&str, Value)]) -> DevelopSettings {
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        for (k, v) in tags {
            map.insert(k.to_string(), v.clone());
        }
        DevelopSettings::from_settings(&map).0
    }

    /// Largest and mean absolute difference.
    pub(crate) fn compare(a: &[f32], b: &[f32]) -> (f32, f32) {
        assert_eq!(a.len(), b.len());
        let mut max = 0.0f32;
        let mut sum = 0.0f64;
        for (x, y) in a.iter().zip(b) {
            let d = (x - y).abs();
            max = max.max(d);
            sum += d as f64;
        }
        (max, (sum / a.len() as f64) as f32)
    }

    /// The per-pixel stages: noise reduction and sharpening (which raws get
    /// by default) off.
    fn same_as_cpu(name: &str, tags: &[(&'static str, Value)]) {
        check(name, &pixel_stages(tags));
    }

    /// The GPU render of `tags` against the CPU's.
    fn check(name: &str, tags: &[(&str, Value)]) {
        check_at(name, tags, (160, 120));
    }

    fn check_at(name: &str, tags: &[(&str, Value)], (w, h): (usize, usize)) {
        let Some(gpu) = gpu() else { return };
        let img = photo(w, h);
        let s = settings(tags);
        let plan = amb_engine::plan::plan(&img, &s).unwrap();
        let ours = gpu.render(&plan);
        let cpu = pipeline::render(&img, &s);
        assert_eq!((ours.width, ours.height), (cpu.width, cpu.height));
        let (max, mean) = compare(&ours.rgb, &cpu.rgb);
        eprintln!("{name}: max {max:.2e}, mean {mean:.2e}");
        assert!(max < 2e-3 && mean < 1e-4, "{name}: max {max}, mean {mean}");
    }

    /// The per-pixel stages alone (no detail stages, which raws get by default).
    fn pixel_stages(tags: &[(&'static str, Value)]) -> Vec<(&'static str, Value)> {
        let mut t = vec![
            ("Sharpness", Value::Number(0.0)),
            ("ColorNoiseReduction", Value::Number(0.0)),
        ];
        t.extend_from_slice(tags);
        t
    }

    fn n(v: f64) -> Value {
        Value::Number(v)
    }

    fn points(xy: &[f64]) -> Value {
        Value::Seq(xy.iter().map(|v| Value::Number(*v)).collect())
    }

    #[test]
    fn matrix_and_highlight_recovery() {
        same_as_cpu("as shot", &[]);
        same_as_cpu("exposure", &[("Exposure2012", n(1.3))]);
        same_as_cpu(
            "custom white, shadow tint",
            &[
                ("WhiteBalance", Value::Text("Custom".into())),
                ("Temperature", n(4300.0)),
                ("Tint", n(12.0)),
                ("ShadowTint", n(-40.0)),
                ("RedHue", n(20.0)),
                ("BlueSaturation", n(-30.0)),
            ],
        );
    }

    #[test]
    fn tone_colour_and_curves() {
        same_as_cpu(
            "tone",
            &[
                ("Contrast2012", n(40.0)),
                ("Whites2012", n(-20.0)),
                ("Blacks2012", n(15.0)),
            ],
        );
        same_as_cpu(
            "colour",
            &[
                ("Vibrance", n(45.0)),
                ("Saturation", n(-15.0)),
                ("HueAdjustmentGreen", n(30.0)),
                ("SaturationAdjustmentBlue", n(25.0)),
                ("LuminanceAdjustmentOrange", n(-20.0)),
                ("ColorGradeShadowHue", n(200.0)),
                ("ColorGradeShadowSat", n(40.0)),
                ("ColorGradeHighlightHue", n(40.0)),
                ("ColorGradeHighlightSat", n(30.0)),
                ("ColorGradeMidtoneLum", n(-10.0)),
                ("ColorGradeBlending", n(70.0)),
            ],
        );
        same_as_cpu(
            "black and white",
            &[
                ("ConvertToGrayscale", Value::Bool(true)),
                ("GrayMixerBlue", n(-40.0)),
                ("GrayMixerYellow", n(30.0)),
            ],
        );
        same_as_cpu(
            "curves",
            &[
                ("ParametricShadows", n(20.0)),
                ("ParametricHighlights", n(-15.0)),
                (
                    "ToneCurvePV2012",
                    points(&[0.0, 0.0, 64.0, 50.0, 192.0, 210.0, 255.0, 255.0]),
                ),
                ("ToneCurvePV2012Blue", points(&[0.0, 20.0, 255.0, 240.0])),
            ],
        );
    }

    #[test]
    fn a_full_edit_with_cpu_steps_in_between() {
        // Defaults (sharpening, colour noise reduction) and every local stage.
        check(
            "full edit",
            &[
                ("Exposure2012", n(0.4)),
                ("Dehaze", n(25.0)),
                ("Highlights2012", n(-50.0)),
                ("Shadows2012", n(40.0)),
                ("Contrast2012", n(20.0)),
                ("Clarity2012", n(30.0)),
                ("Texture", n(20.0)),
                ("Vibrance", n(20.0)),
                ("LuminanceSmoothing", n(30.0)),
                ("PostCropVignetteAmount", n(-30.0)),
                ("GrainAmount", n(20.0)),
            ],
        );
    }

    #[test]
    fn highlights_shadows_texture_and_clarity() {
        same_as_cpu(
            "highlights and shadows",
            &[("Highlights2012", n(-70.0)), ("Shadows2012", n(60.0))],
        );
        same_as_cpu("shadows only", &[("Shadows2012", n(-40.0))]);
        same_as_cpu("clarity", &[("Clarity2012", n(55.0))]);
        same_as_cpu("negative clarity", &[("Clarity2012", n(-60.0))]);
        same_as_cpu(
            "texture and clarity",
            &[("Texture", n(45.0)), ("Clarity2012", n(30.0))],
        );
        same_as_cpu(
            "smoothing texture",
            &[("Texture", n(-50.0)), ("Clarity2012", n(25.0))],
        );
    }

    #[test]
    fn gaussian_blurs_match() {
        let Some(gpu) = gpu() else { return };
        let (w, h) = (97, 61);
        let plane: Vec<f32> = (0..w * h)
            .map(|i| ((i * 37 % 101) as f32 / 101.0) + if i % w < 40 { 0.3 } else { 0.0 })
            .collect();
        for sigma in [0.2f32, 1.4, 2.9, 5.0, 11.0] {
            let cpu = amb_engine::filters::gaussian_blur(&plane, w, h, sigma);
            let src = gpu.upload(&plane);
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            let out = crate::local::Planes::new(&gpu, &mut enc).gaussian(
                &src,
                w,
                h,
                &amb_engine::filters::gaussian_plan(sigma),
            );
            gpu.queue.submit([enc.finish()]);
            let ours = gpu.download(out.as_ref().unwrap_or(&src), w * h);
            let (max, _) = compare(&ours, &cpu);
            assert!(max < 1e-5, "sigma {sigma}: {max}");
        }
    }

    #[test]
    fn subsampled_guided_filters() {
        // Large enough that the guided filters work on 2× subsampled copies
        // (as previews do), with partial blocks at the odd edges.
        let (w, h) = (1203, 803);
        let tone = amb_engine::local_tone::plan(
            &amb_engine::tone::ToneControls {
                shadows: 50.0,
                ..Default::default()
            },
            &amb_engine::tone::DEFAULT_STRENGTHS,
            amb_engine::local::reference_scale(w, h),
            w,
            h,
        )
        .unwrap();
        assert!(tone.guided.s > 1, "{:?}", tone.guided);
        check_at(
            "subsampled",
            &pixel_stages(&[
                ("Highlights2012", n(-60.0)),
                ("Shadows2012", n(50.0)),
                ("Clarity2012", n(40.0)),
                ("Texture", n(30.0)),
            ]),
            (w, h),
        );
    }
}
