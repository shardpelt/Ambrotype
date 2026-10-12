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
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct DehazeParams {
    w: u32,
    h: u32,
    factor: u32,
    sw: u32,
    sh: u32,
    omega: f32,
    min_transmission: f32,
    shadow_keep: f32,
    airlight: [f32; 3],
    inv_airlight: [f32; 3],
    fog: f32,
    fog_depth: f32,
    fog_colour: [f32; 3],
    comp: f32,
    inv_airlight_y: f32,
    lum: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct CnrParams {
    w: u32,
    h: u32,
    factor: u32,
    sw: u32,
    sh: u32,
    tw: u32,
    th: u32,
    mix: f32,
    detail: u32,
    garrote: u32,
    half: f32,
    lum: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VignetteParams {
    w: u32,
    h: u32,
    x0: u32,
    y0: u32,
    p: f32,
    inner: f32,
    outer: f32,
    mirrored: u32,
    base: f32,
    norm: f32,
    q_skip: f32,
    style: u32,
    stops: f32,
    protect: f32,
    opacity: f32,
    target_white: f32,
    gamma: f32,
    lum: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GrainParams {
    w: u32,
    h: u32,
    margin: u32,
    fine_len: u32,
    coarse_len: u32,
    with_coarse: u32,
    seed_lo: u32,
    seed_hi: u32,
    coarse_weight: f32,
    inv_std: f32,
    strength: f32,
    variance: f32,
    lum: [f32; 3],
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
    pub(crate) noise: crate::denoise::NoiseKernels,
    unclip_weights: Kernel,
    matrix: Kernel,
    base_look: Kernel,
    tone_curve: Kernel,
    curves: Kernel,
    color: Kernel,
    vignette: Kernel,
    grain_white: Kernel,
    grain_horizontal: Kernel,
    grain_apply: Kernel,
    sanitise: Kernel,
    dehaze_downscale: Kernel,
    dehaze_apply: Kernel,
    cnr_downscale: Kernel,
    cnr_residual: Kernel,
    cnr_tiles: Kernel,
    cnr_apply: Kernel,
}

impl Kernels {
    pub(crate) fn new(gpu: &Gpu) -> Kernels {
        let module = |name: &str, src: &str| format!("{COMMON}\n// ---- {name}\n{src}");
        let matrix = module("matrix", include_str!("shaders/matrix.wgsl"));
        let tone = module("tone", include_str!("shaders/tone.wgsl"));
        let color = module("color", include_str!("shaders/color.wgsl"));
        let effects = module("effects", include_str!("shaders/effects.wgsl"));
        let grain = module("grain", include_str!("shaders/grain.wgsl"));
        // Bindings: image, white noise, fine and coarse rows, taps, weights, parameters.
        const GRAIN: &[bool] = &[false, false, false, false, true, true, true];
        let g = |entry: &str| Kernel::new(gpu, entry, &grain, entry, GRAIN);
        let dehaze = module("dehaze", include_str!("shaders/dehaze.wgsl"));
        // Bindings: image, small copy, coefficients A and B, parameters.
        const DEHAZE: &[bool] = &[false, false, true, true, true];
        let cnr = module("cnr", include_str!("shaders/cnr.wgsl"));
        // Bindings: image, small copy, coefficients, energy, levels, tiles, tau, parameters.
        const CNR: &[bool] = &[false, false, true, false, false, false, true, true];
        let c = |entry: &str| Kernel::new(gpu, entry, &cnr, entry, CNR);
        // Bindings: camera, image, weights, size, parameters, tables.
        const MATRIX: &[bool] = &[true, false, false, true, true, true];
        // Bindings: image, size, parameters, tables.
        const IMAGE: &[bool] = &[false, true, true, true];
        Kernels {
            local: crate::local::LocalKernels::new(gpu, COMMON),
            noise: crate::denoise::NoiseKernels::new(gpu, COMMON),
            unclip_weights: Kernel::new(gpu, "unclip_weights", &matrix, "unclip_weights", MATRIX),
            matrix: Kernel::new(gpu, "matrix", &matrix, "matrix", MATRIX),
            base_look: Kernel::new(gpu, "base_look", &tone, "base_look", IMAGE),
            tone_curve: Kernel::new(gpu, "tone_curve", &tone, "tone_curve", IMAGE),
            curves: Kernel::new(gpu, "curves", &tone, "curves", IMAGE),
            color: Kernel::new(gpu, "color", &color, "color", IMAGE),
            // Bindings: image, columns, rows, parameters.
            vignette: Kernel::new(
                gpu,
                "vignette",
                &effects,
                "vignette",
                &[false, true, true, true],
            ),
            grain_white: g("grain_white"),
            grain_horizontal: g("grain_horizontal"),
            grain_apply: g("grain_apply"),
            sanitise: Kernel::new(gpu, "sanitise", &dehaze, "sanitise", DEHAZE),
            dehaze_downscale: Kernel::new(gpu, "downscale", &dehaze, "downscale", DEHAZE),
            dehaze_apply: Kernel::new(gpu, "dehaze_apply", &dehaze, "dehaze_apply", DEHAZE),
            cnr_downscale: c("cnr_downscale"),
            cnr_residual: c("cnr_residual"),
            cnr_tiles: c("cnr_tiles"),
            cnr_apply: c("cnr_apply"),
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
                Step::Sharpen(p) => {
                    crate::local::Planes::new(self, &mut enc).sharpen(
                        &img,
                        plan.width,
                        plan.height,
                        p,
                    );
                }
                Step::Dehaze {
                    amount,
                    k: dk,
                    factor,
                } => {
                    // Zero non-finite values and downscale here, measure the
                    // haze on the small copy on the CPU, apply it here.
                    let (w, h, f) = (plan.width, plan.height, *factor);
                    let (sw, sh) = (w.div_ceil(f), h.div_ceil(f));
                    let mut p = DehazeParams {
                        w: w as u32,
                        h: h as u32,
                        factor: f as u32,
                        sw: sw as u32,
                        sh: sh as u32,
                        lum: amb_engine::buffer::luminance_weights(),
                        ..Default::default()
                    };
                    let params = self.storage(&p);
                    let small = self.buffer(sw * sh * 3);
                    let (u1, u2) = (self.buffer(1), self.buffer(1));
                    let bindings = [&img, &small, &u1, &u2, &params];
                    k.sanitise.dispatch(self, &mut enc, &bindings, pixels * 3);
                    if f > 1 {
                        k.dehaze_downscale
                            .dispatch(self, &mut enc, &bindings, sw * sh);
                    }
                    let done = std::mem::replace(
                        &mut enc,
                        self.device.create_command_encoder(&Default::default()),
                    );
                    self.queue.submit([done.finish()]);
                    let copy = self.download(if f > 1 { &small } else { &img }, sw * sh * 3);
                    let est = amb_engine::dehaze::Estimate::from_small(copy, sw, sh, f, dk);
                    let a = amb_engine::dehaze::apply_numbers(*amount, &est, dk);
                    p.omega = a.omega;
                    p.min_transmission = a.min_transmission;
                    p.shadow_keep = a.shadow_keep;
                    p.airlight = a.airlight;
                    p.inv_airlight = a.inv_airlight;
                    p.fog = a.fog;
                    p.fog_depth = a.fog_depth;
                    p.fog_colour = a.fog_colour;
                    p.comp = a.comp;
                    p.inv_airlight_y = a.inv_airlight_y;
                    let params = self.storage(&p);
                    let (ca, cb) = (self.upload(&a.coef_a), self.upload(&a.coef_b));
                    k.dehaze_apply
                        .dispatch(self, &mut enc, &[&img, &small, &ca, &cb, &params], n);
                }
                Step::LuminanceNr(lp) => {
                    self.luminance_nr(&mut enc, &img, plan.width, plan.height, lp);
                }
                Step::ColorNr(cp) => {
                    self.color_nr(&mut enc, &img, plan.width, plan.height, cp);
                }
                Step::Vignette(v) => {
                    use amb_engine::effects::VignetteStyle;
                    let (m, op) = (&v.mask, &v.op);
                    let params = self.storage(&VignetteParams {
                        w: plan.width as u32,
                        h: plan.height as u32,
                        x0: 0,
                        y0: 0,
                        p: m.p,
                        inner: m.inner,
                        outer: m.outer,
                        mirrored: m.mirrored as u32,
                        base: m.base,
                        norm: m.norm,
                        q_skip: m.q_skip,
                        style: match op.style {
                            VignetteStyle::HighlightPriority => 0,
                            VignetteStyle::ColorPriority => 1,
                            VignetteStyle::PaintOverlay => 2,
                        },
                        stops: op.stops,
                        protect: op.protect,
                        opacity: op.opacity,
                        target_white: op.target,
                        gamma: op.gamma,
                        lum: op.lum,
                    });
                    let (col, row) = (self.upload(&m.col), self.upload(&m.row));
                    k.vignette
                        .dispatch(self, &mut enc, &[&img, &col, &row, &params], n);
                }
                Step::Grain(gp) => {
                    let (f, op) = (&gp.field, &gp.op);
                    let with_coarse = f.coarse_weight != 0.0;
                    let m = f.margin;
                    let (w, h) = (plan.width, plan.height);
                    let params = self.storage(&GrainParams {
                        w: w as u32,
                        h: h as u32,
                        margin: m as u32,
                        fine_len: f.fine.len() as u32,
                        coarse_len: f.coarse.len() as u32,
                        with_coarse: with_coarse as u32,
                        seed_lo: f.seed_key as u32,
                        seed_hi: (f.seed_key >> 32) as u32,
                        coarse_weight: f.coarse_weight,
                        inv_std: f.inv_std,
                        strength: op.strength,
                        variance: op.variance,
                        lum: op.lum,
                    });
                    let taps = self.upload(&[f.fine.as_slice(), &f.coarse].concat());
                    let weight = self.upload(&op.weight);
                    let rows = w * (h + 2 * m);
                    let white = self.buffer((w + 2 * m) * (h + 2 * m));
                    let fine = self.buffer(rows);
                    let coarse = self.buffer(if with_coarse { rows } else { 1 });
                    let b = [&img, &white, &fine, &coarse, &taps, &weight, &params];
                    k.grain_white
                        .dispatch(self, &mut enc, &b, (w + 2 * m) * (h + 2 * m));
                    k.grain_horizontal.dispatch(self, &mut enc, &b, rows);
                    k.grain_apply.dispatch(self, &mut enc, &b, n);
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

impl Gpu {
    /// Finish what `enc` holds and start a new encoder.
    pub(crate) fn flush(&self, enc: &mut wgpu::CommandEncoder) {
        let done = std::mem::replace(enc, self.device.create_command_encoder(&Default::default()));
        self.queue.submit([done.finish()]);
    }

    /// Colour noise reduction: downscale here, the guided filter's
    /// coefficients on the CPU, the residual and its tile sums here, the
    /// noise measured on the CPU, the garrote here.
    fn color_nr(
        &self,
        enc: &mut wgpu::CommandEncoder,
        img: &wgpu::Buffer,
        w: usize,
        h: usize,
        plan: &amb_engine::detail::ColorNrPlan,
    ) {
        use amb_engine::detail::{TileSums, color_nr_low, color_nr_tau};
        let k = self.kernels();
        let f = plan.factor;
        let (sw, sh) = (w.div_ceil(f), h.div_ceil(f));
        let (tw, th) = (w.div_ceil(16), h.div_ceil(16));
        // The garrote's exponent, as `Garrote::new` reads it.
        let power = if plan.garrote_power.is_finite() {
            plan.garrote_power
        } else {
            amb_engine::detail::DEFAULT_STRENGTHS.color_garrote_power
        }
        .clamp(0.1, 64.0);
        let mut p = CnrParams {
            w: w as u32,
            h: h as u32,
            factor: f as u32,
            sw: sw as u32,
            sh: sh as u32,
            tw: tw as u32,
            th: th as u32,
            mix: plan.mix,
            detail: plan.detail_factor.is_some() as u32,
            garrote: if power == 2.0 {
                2
            } else if power == 4.0 {
                4
            } else {
                0
            },
            half: (power / 2.0) as f32,
            lum: amb_engine::buffer::luminance_weights(),
        };
        let small = self.buffer(sw * sh * 3);
        let (energy, lc, tiles) = if plan.detail_factor.is_some() {
            (
                self.buffer(w * h),
                self.buffer(w * h * 5),
                self.buffer(tw * th * 14),
            )
        } else {
            (self.buffer(1), self.buffer(1), self.buffer(1))
        };
        let params = self.storage(&p);
        let (coefs0, tau0) = (self.buffer(1), self.buffer(2));
        k.cnr_downscale.dispatch(
            self,
            enc,
            &[img, &small, &coefs0, &energy, &lc, &tiles, &tau0, &params],
            sw * sh,
        );
        self.flush(enc);
        let low = color_nr_low(&self.download(&small, sw * sh * 3), sw, sh, plan);
        // Six grids: a for R, G, B, then b for R, G, B.
        let packed: Vec<f32> = (0..3)
            .flat_map(|c| low.coefs[c].0.iter().copied())
            .chain((0..3).flat_map(|c| low.coefs[c].1.iter().copied()))
            .collect();
        let coefs = self.upload(&packed);
        let tau = if plan.detail_factor.is_some() {
            let b = [img, &small, &coefs, &energy, &lc, &tiles, &tau0, &params];
            k.cnr_residual.dispatch(self, enc, &b, w * h);
            k.cnr_tiles.dispatch(self, enc, &b, tw * th);
            self.flush(enc);
            let sums = self.download(&tiles, tw * th * 14);
            let at = |t: &[f32]| TileSums {
                power: [t[0], t[1], t[2]],
                lum: t[3],
                lo: t[4],
                hi: t[5],
                n: t[6] as u32,
            };
            let energy_tiles: Vec<TileSums> = sums.chunks_exact(14).map(|t| at(&t[..7])).collect();
            let fine: Vec<TileSums> = sums.chunks_exact(14).map(|t| at(&t[7..])).collect();
            match color_nr_tau(&low, &energy_tiles, &fine, plan) {
                Some(t) => self.upload(&t),
                None => {
                    // No threshold: no residual is kept.
                    p.detail = 0;
                    self.buffer(2)
                }
            }
        } else {
            self.buffer(2)
        };
        let params = self.storage(&p);
        k.cnr_apply.dispatch(
            self,
            enc,
            &[img, &small, &coefs, &energy, &lc, &tiles, &tau, &params],
            w * h,
        );
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

    #[test]
    fn sharpening() {
        let only = |tags: &[(&'static str, Value)]| {
            let mut t = vec![("ColorNoiseReduction", n(0.0))];
            t.extend_from_slice(tags);
            t
        };
        check("default sharpening", &only(&[]));
        check(
            "sharpening, radius and detail",
            &only(&[
                ("Sharpness", n(90.0)),
                ("SharpenRadius", n(1.7)),
                ("SharpenDetail", n(70.0)),
            ]),
        );
        check(
            "sharpening with masking",
            &only(&[("Sharpness", n(70.0)), ("SharpenEdgeMasking", n(45.0))]),
        );
    }

    #[test]
    fn spot_removal_runs_between_the_gpu_stages() {
        use amb_presets::retouch::{SpotKind, new_area};
        let spots = Value::Seq(vec![
            Value::Struct(new_area(
                SpotKind::Heal,
                (0.3, 0.4),
                0.05,
                Some((0.6, 0.5)),
                false,
                0.5,
                1.0,
            )),
            Value::Struct(new_area(
                SpotKind::Clone,
                (0.7, 0.3),
                0.04,
                Some((0.2, 0.7)),
                false,
                0.3,
                0.8,
            )),
        ]);
        let tags = pixel_stages(&[("Contrast2012", n(15.0)), ("RetouchAreas", spots)]);
        check("spots", &tags);
    }

    #[test]
    fn masks_run_between_the_gpu_stages() {
        let t = |v: &str| Value::Text(v.into());
        let st = |p: Vec<(&str, Value)>| {
            Value::Struct(p.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        };
        let gradient = st(vec![
            ("What", t("Mask/Gradient")),
            ("MaskBlendMode", t("0")),
            ("MaskValue", t("1")),
            ("ZeroX", t("0.5")),
            ("ZeroY", t("0.8")),
            ("FullX", t("0.5")),
            ("FullY", t("0.2")),
        ]);
        let correction = |adjust: &[(&str, &str)]| {
            let mut p = vec![
                ("What", t("Correction")),
                ("CorrectionAmount", t("1")),
                ("CorrectionMasks", Value::Seq(vec![gradient.clone()])),
            ];
            p.extend(adjust.iter().map(|(k, v)| (*k, t(v))));
            st(p)
        };
        let masks = |adjust: &[(&str, &str)]| {
            (
                "MaskGroupBasedCorrections",
                Value::Seq(vec![correction(adjust)]),
            )
        };
        // One mask step at a time, then all of them with the global stages.
        let each = [
            (
                "light",
                &[("LocalExposure2012", "0.25"), ("LocalTemperature", "0.3")][..],
            ),
            ("dehaze", &[("LocalDehaze", "0.4")]),
            (
                "tone",
                &[("LocalContrast2012", "0.3"), ("LocalShadows2012", "0.4")],
            ),
            (
                "texture",
                &[("LocalTexture", "0.3"), ("LocalClarity2012", "0.3")],
            ),
            (
                "colour",
                &[
                    ("LocalSaturation", "-0.4"),
                    ("LocalToningSaturation", "0.3"),
                ],
            ),
            (
                "detail",
                &[("LocalSharpness", "0.5"), ("LocalLuminanceNoise", "0.4")],
            ),
        ];
        for (name, adjust) in each {
            check(&format!("mask {name}"), &pixel_stages(&[masks(adjust)]));
        }
        let all: Vec<(&str, &str)> = each.iter().flat_map(|(_, a)| a.iter().copied()).collect();
        let tags = [
            ("Exposure2012", n(0.3)),
            ("Dehaze", n(10.0)),
            ("Contrast2012", n(15.0)),
            ("Clarity2012", n(10.0)),
            ("Saturation", n(10.0)),
            masks(&all),
        ];
        check("masks", &tags);
        // The masks do change the picture.
        let img = photo(160, 120);
        let with = pipeline::render(&img, &settings(&tags));
        let without = pipeline::render(&img, &settings(&tags[..5]));
        assert!(compare(&with.rgb, &without.rgb).1 > 1e-3);
    }

    #[test]
    fn a_lut_runs_between_the_gpu_stages() {
        // A 2-point cube that warms and lifts the blacks a little.
        let path = std::env::temp_dir().join(format!("amb-gpu-lut-{}.cube", std::process::id()));
        let mut text = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    let (r, g, b) = (r as f32, g as f32, b as f32);
                    text += &format!("{} {} {}\n", 0.05 + r * 0.95, g * 0.97, 0.03 + b * 0.85);
                }
            }
        }
        std::fs::write(&path, text).unwrap();
        let tags = pixel_stages(&[
            ("Contrast2012", n(20.0)),
            ("amb:LUT", Value::Text(path.display().to_string())),
            ("amb:LUTAmount", n(70.0)),
        ]);
        check("lut", &tags);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vignette() {
        same_as_cpu("vignette", &[("PostCropVignetteAmount", n(-60.0))]);
        same_as_cpu(
            "vignette, round and soft, highlights kept",
            &[
                ("PostCropVignetteAmount", n(-45.0)),
                ("PostCropVignetteRoundness", n(80.0)),
                ("PostCropVignetteFeather", n(90.0)),
                ("PostCropVignetteMidpoint", n(20.0)),
                ("PostCropVignetteHighlightContrast", n(60.0)),
            ],
        );
        same_as_cpu(
            "white vignette, rectangular",
            &[
                ("PostCropVignetteAmount", n(50.0)),
                ("PostCropVignetteRoundness", n(-70.0)),
            ],
        );
        same_as_cpu(
            "color priority",
            &[
                ("PostCropVignetteAmount", n(-50.0)),
                ("PostCropVignetteStyle", n(2.0)),
            ],
        );
        same_as_cpu(
            "paint overlay",
            &[
                ("PostCropVignetteAmount", n(40.0)),
                ("PostCropVignetteStyle", n(3.0)),
            ],
        );
        same_as_cpu(
            "paint overlay, dark",
            &[
                ("PostCropVignetteAmount", n(-40.0)),
                ("PostCropVignetteStyle", n(3.0)),
            ],
        );
    }

    #[test]
    fn grain() {
        let tags = pixel_stages(&[("GrainAmount", n(60.0))]);
        let plan = amb_engine::plan::plan(&photo(160, 120), &settings(&tags)).unwrap();
        assert!(
            plan.steps
                .iter()
                .any(|s| matches!(s, amb_engine::plan::Step::Grain(_))),
            "{:?}",
            plan.steps
        );
        same_as_cpu("grain", &[("GrainAmount", n(60.0))]);
        same_as_cpu(
            "large, smooth grain",
            &[
                ("GrainAmount", n(100.0)),
                ("GrainSize", n(100.0)),
                ("GrainFrequency", n(0.0)),
            ],
        );
        same_as_cpu(
            "fine, rough grain, another seed",
            &[
                ("GrainAmount", n(40.0)),
                ("GrainSize", n(0.0)),
                ("GrainFrequency", n(100.0)),
                ("GrainSeed", n(12345.0)),
            ],
        );
        check_at(
            "grain on a larger image",
            &pixel_stages(&[("GrainAmount", n(80.0)), ("GrainSize", n(60.0))]),
            (1203, 803),
        );
    }

    #[test]
    fn dehaze() {
        same_as_cpu("dehaze", &[("Dehaze", n(45.0))]);
        same_as_cpu("fog", &[("Dehaze", n(-60.0))]);
        // Downscaled for the estimate (as previews are).
        check_at(
            "dehaze, downscaled estimate",
            &pixel_stages(&[("Dehaze", n(35.0)), ("Exposure2012", n(0.5))]),
            (1703, 611),
        );
    }

    #[test]
    fn color_noise_reduction() {
        let only = |tags: &[(&'static str, Value)]| {
            let mut t = vec![("Sharpness", n(0.0))];
            t.extend_from_slice(tags);
            t
        };
        check("default colour noise reduction", &only(&[]));
        check(
            "strong, smooth, no detail",
            &only(&[
                ("ColorNoiseReduction", n(80.0)),
                ("ColorNoiseReductionDetail", n(0.0)),
                ("ColorNoiseReductionSmoothness", n(90.0)),
            ]),
        );
        check(
            "more detail",
            &only(&[
                ("ColorNoiseReduction", n(40.0)),
                ("ColorNoiseReductionDetail", n(85.0)),
            ]),
        );
        check_at(
            "default colour noise reduction, larger",
            &only(&[("Exposure2012", n(0.3))]),
            (1011, 677),
        );
    }

    #[test]
    fn luminance_noise_reduction() {
        let only = |tags: &[(&'static str, Value)]| {
            let mut t = vec![("Sharpness", n(0.0)), ("ColorNoiseReduction", n(0.0))];
            t.extend_from_slice(tags);
            t
        };
        // It does change this photo.
        let img = photo(160, 120);
        let off = pipeline::render(&img, &settings(&only(&[])));
        let on = pipeline::render(&img, &settings(&only(&[("LuminanceSmoothing", n(40.0))])));
        assert!(compare(&on.rgb, &off.rgb).0 > 1e-2);
        check(
            "luminance noise reduction",
            &only(&[("LuminanceSmoothing", n(40.0))]),
        );
        check(
            "strong, no detail",
            &only(&[
                ("LuminanceSmoothing", n(90.0)),
                ("LuminanceNoiseReductionDetail", n(0.0)),
            ]),
        );
        check(
            "detail and contrast",
            &only(&[
                ("LuminanceSmoothing", n(60.0)),
                ("LuminanceNoiseReductionDetail", n(80.0)),
                ("LuminanceNoiseReductionContrast", n(70.0)),
            ]),
        );
        check(
            "contrast, no detail",
            &only(&[
                ("LuminanceSmoothing", n(30.0)),
                ("LuminanceNoiseReductionDetail", n(0.0)),
                ("LuminanceNoiseReductionContrast", n(100.0)),
            ]),
        );
        check_at(
            "with colour noise reduction, larger",
            &[("LuminanceSmoothing", n(50.0)), ("Exposure2012", n(0.3))],
            (1011, 677),
        );
    }
}
