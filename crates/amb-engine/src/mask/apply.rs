// SPDX-License-Identifier: GPL-3.0-or-later
//! Applying a mask: what a render draws, and the local adjustments through it.

use super::*;

/// Masks drawn for one render, with the adjustments they carry.
pub struct Drawn {
    planes: Vec<(LocalAdjustments, Vec<f32>)>,
    /// The masks with point curves, and where they apply.
    curves: Vec<(std::sync::Arc<LocalCurves>, Vec<f32>)>,
    overlay: Option<Vec<f32>>,
    /// The part of the output the planes cover.
    tile: Tile,
    /// The colours the colour range masks selected (see [`draw_in`]).
    pub colour_targets: Vec<Vec<[f32; 3]>>,
    /// Where Select Sky found the sky.
    pub sky_maps: Vec<SkyMap>,
}

/// Draw every mask for an image at the end of the scene-linear stage.
/// `frame` is the uncropped frame's size, `crop` the output → frame map,
/// `encode` maps scene-linear luminance to the perceptual 0..1 scale range
/// masks select on.
pub fn draw(
    masks: &Masks,
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    encode: impl Fn(f32) -> f32 + Sync,
    strengths: &MaskStrengths,
) -> Drawn {
    draw_in(
        masks,
        img,
        frame,
        crop,
        encode,
        strengths,
        &Tile::of(img),
        None,
        None,
    )
}

/// [`draw`] for `img` holding `tile` of the output. Colour range masks
/// select `colour_targets` when given (as measured by a render of the whole
/// output, [`Drawn::colour_targets`]); otherwise the colours under their
/// samples here.
#[allow(clippy::too_many_arguments)]
pub fn draw_in(
    masks: &Masks,
    img: &Image,
    frame: (usize, usize),
    crop: Option<&CropGeometry>,
    encode: impl Fn(f32) -> f32 + Sync,
    strengths: &MaskStrengths,
    tile: &Tile,
    colour_targets: Option<&[Vec<[f32; 3]>]>,
    sky_maps: Option<&[SkyMap]>,
) -> Drawn {
    let frame = Frame::new(frame, crop).in_tile(tile);
    let lw = luminance_weights();
    let luminance: Option<Vec<f32>> =
        masks.needs(|s| matches!(s, Shape::Luminance(_))).then(|| {
            img.rgb
                .par_chunks(3)
                .map(|p| encode((lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(0.0)))
                .collect()
        });
    let lab: Option<Vec<[f32; 3]>> = masks
        .needs(|s| matches!(s, Shape::Color { .. } | Shape::Sky))
        .then(|| {
            let ok = OkLab::new();
            img.rgb
                .par_chunks(3)
                .map(|p| {
                    // Compare colours as they look: tone-encode the light first.
                    let y = (lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2]).max(1e-6);
                    let s = encode(y) / y;
                    ok.from_working([p[0] * s, p[1] * s, p[2] * s])
                })
                .collect()
        });
    // The subject model sees the photo as it looks, in sRGB (a tile gets the
    // whole photo's map, and refines its edges on its own pixels).
    let srgb: Option<Vec<f32>> = masks.needs(|s| matches!(s, Shape::Model(_))).then(|| {
        let (to_srgb, _) = crate::pipeline::srgb_matrices();
        img.rgb
            .par_chunks(3)
            .flat_map_iter(|p| {
                (0..3).map(move |i| {
                    let v = to_srgb[i][0] * p[0] + to_srgb[i][1] * p[1] + to_srgb[i][2] * p[2];
                    v.max(0.0)
                })
            })
            .map(|v| encode(v).clamp(0.0, 1.0))
            .collect()
    });
    let reference = Reference {
        luminance: luminance.as_deref(),
        lab: lab.as_deref(),
        srgb: srgb.as_deref(),
        width: img.width,
        height: img.height,
        strengths: *strengths,
        tile: *tile,
    };
    let mut planes = Vec::new();
    let mut curves = Vec::new();
    let mut overlay = None;
    let mut targets = Targets {
        given: colour_targets,
        measured: Vec::new(),
        sky_given: sky_maps,
        sky_measured: Vec::new(),
    };
    for c in &masks.corrections {
        let shown = masks.overlay.is_some() && masks.overlay == c.index;
        if !shown && c.adjust.is_neutral() && c.curves.is_none() {
            continue;
        }
        let alpha = draw_correction(c, &frame, &reference, &mut targets);
        if shown {
            overlay = Some(alpha.clone());
        }
        if let Some(k) = &c.curves {
            curves.push((k.clone(), alpha.clone()));
        }
        if !c.adjust.is_neutral() {
            planes.push((c.adjust, alpha));
        }
    }
    Drawn {
        planes,
        curves,
        overlay,
        tile: *tile,
        colour_targets: targets.measured,
        sky_maps: targets.sky_measured,
    }
}

/// How far (pixels) the masks' local adjustments reach at `scale` (the
/// shapes themselves are drawn per pixel).
pub fn reach(masks: &Masks, cal: &Calibration, scale: f32) -> usize {
    let k = &cal.masks;
    masks
        .corrections
        .iter()
        .map(|c| {
            let a = &c.adjust;
            let contrast = LocalContrast {
                texture: a.texture,
                clarity: a.clarity,
            };
            let noise = (a.noise > 0.0).then_some(NoiseReduction {
                luminance: a.noise,
                luminance_detail: 50.0,
                luminance_contrast: 0.0,
                color: 0.0,
                color_detail: 50.0,
                color_smoothness: 50.0,
            });
            let sharpness = if a.sharpness > 0.0 {
                let s = Sharpening {
                    amount: k.sharpness * a.sharpness / 100.0,
                    radius: 1.0,
                    detail: 25.0,
                    masking: 0.0,
                };
                detail::sharpening_reach(&s, &cal.detail)
            } else if a.sharpness < 0.0 {
                gaussian_reach(((k.soften_sigma * -a.sharpness / 100.0) as f32 * scale).max(0.3))
            } else {
                0
            };
            local::reach(&contrast, &cal.local, scale)
                + noise.map_or(0, |n| detail::noise_reduction_reach(&n, &cal.detail))
                + sharpness
        })
        .sum()
}

/// Blend `alt` into `img` through `alpha`.
pub(super) fn blend(img: &mut Image, alt: &Image, alpha: &[f32]) {
    img.rgb
        .par_chunks_mut(3)
        .zip(alt.rgb.par_chunks(3))
        .zip(alpha.par_iter())
        .for_each(|((p, q), &a)| {
            if a > 0.0 {
                for c in 0..3 {
                    p[c] += (q[c] - p[c]) * a;
                }
            }
        });
}

impl Drawn {
    fn each(
        &self,
        active: impl Fn(&LocalAdjustments) -> bool,
    ) -> impl Iterator<Item = &(LocalAdjustments, Vec<f32>)> {
        self.planes.iter().filter(move |(a, _)| active(a))
    }

    /// Exposure, Temp and Tint on scene-linear ProPhoto.
    pub fn apply_light(&self, img: &mut Image, k: &MaskStrengths) {
        let lw = luminance_weights();
        for (a, alpha) in self.each(|a| a.exposure != 0.0 || a.temperature != 0.0 || a.tint != 0.0)
        {
            let t = a.temperature / 100.0;
            let g = [
                (k.temperature * t).exp(),
                (-k.tint * a.tint / 100.0).exp(),
                (-k.temperature * t).exp(),
            ];
            // Keep luminance: a colour shift, not a brightness one.
            let norm = lw[0] as f64 * g[0] + lw[1] as f64 * g[1] + lw[2] as f64 * g[2];
            let log_gain =
                g.map(|v| ((v / norm).ln() + a.exposure * std::f64::consts::LN_2) as f32);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m > 0.0 {
                        for c in 0..3 {
                            p[c] *= (log_gain[c] * m).exp();
                        }
                    }
                });
        }
    }

    /// Global Defringe (`global`) held back where masks have a negative
    /// local Defringe, then each positive one: a Defringe of its own, with
    /// the global hue ranges, inside its mask.
    pub fn apply_defringe(&self, img: &mut Image, global: &Defringe, cal: &Calibration) {
        let mut protect: Option<Vec<f32>> = None;
        for (a, alpha) in self.each(|a| a.defringe < 0.0) {
            let s = (-a.defringe / 100.0).min(1.0) as f32;
            let w = protect.get_or_insert_with(|| vec![1.0; alpha.len()]);
            w.iter_mut()
                .zip(alpha)
                .for_each(|(w, &m)| *w *= 1.0 - s * m);
        }
        lens::apply_defringe_weighted(img, global, &cal.lens, &self.tile, protect.as_deref());
        lens::apply_auto_defringe_in(img, global, &cal.lens, &self.tile);
        for (a, alpha) in self.each(|a| a.defringe > 0.0) {
            let amount = cal.masks.defringe * a.defringe / 100.0;
            let d = Defringe {
                purple_amount: amount,
                green_amount: amount,
                ..*global
            };
            lens::apply_defringe_weighted(img, &d, &cal.lens, &self.tile, Some(alpha));
        }
    }

    /// Local Dehaze. Each mask's haze is measured on the image (a whole
    /// output; `full` is the full output's size), or taken from `given` (one
    /// per Dehaze mask, in order, as measured on the whole output). Returns
    /// the measured estimates.
    pub fn apply_dehaze(
        &self,
        img: &mut Image,
        cal: &Calibration,
        given: Option<&[dehaze::Estimate]>,
        full: (usize, usize),
    ) -> Vec<dehaze::Estimate> {
        let mut measured = Vec::new();
        for (i, (a, alpha)) in self.each(|a| a.dehaze != 0.0).enumerate() {
            let mut alt = img.clone();
            let haze = match given.and_then(|e| e.get(i)) {
                Some(e) => dehaze::Haze::Given(e),
                None => dehaze::Haze::Measure { full },
            };
            measured.extend(dehaze::apply_haze(
                &mut alt,
                a.dehaze,
                &cal.dehaze,
                &self.tile,
                haze,
            ));
            blend(img, &alt, alpha);
        }
        measured
    }

    /// Contrast, Highlights, Shadows, Whites, Blacks on display-linear light,
    /// hue-preserving like the global ones.
    pub fn apply_tone(&self, img: &mut Image, cal: &Calibration, lut_size: usize) {
        let lw = luminance_weights();
        for (a, alpha) in self.each(|a| !a.tone().is_neutral()) {
            let lut = tone::tone_lut(&a.tone(), &cal.tone, lut_size);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    let y = lw[0] * p[0] + lw[1] * p[1] + lw[2] * p[2];
                    if y > 1e-6 {
                        let yc = y.min(1.0);
                        let target =
                            local::srgb_decode(local::lookup(&lut, local::srgb_encode(yc)));
                        let scale = (target / yc).max(0.0).powf(m);
                        for v in p.iter_mut() {
                            *v *= scale;
                        }
                    }
                });
        }
    }

    pub fn apply_local_contrast(&self, img: &mut Image, cal: &Calibration) {
        for (a, alpha) in self.each(|a| a.texture != 0.0 || a.clarity != 0.0) {
            let mut alt = img.clone();
            let p = LocalContrast {
                texture: a.texture,
                clarity: a.clarity,
            };
            local::apply_scaled(&mut alt, &p, &cal.local, self.tile.scale());
            blend(img, &alt, alpha);
        }
    }

    /// Each mask's point curves, after the global curves, faded by the mask.
    pub fn apply_curves(&self, img: &mut Image) {
        const LUT: usize = 4096;
        for (k, alpha) in &self.curves {
            let main = k.main.as_ref().map(|s| s.lut(LUT));
            let channels: Vec<Option<Vec<f32>>> = k
                .channels
                .iter()
                .map(|c| c.as_ref().map(|s| s.lut(LUT)))
                .collect();
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    for (ch, v) in p.iter_mut().enumerate() {
                        let mut e = local::srgb_encode(v.clamp(0.0, 1.0));
                        if let Some(lut) = &main {
                            e = local::lookup(lut, e);
                        }
                        if let Some(lut) = &channels[ch] {
                            e = local::lookup(lut, e);
                        }
                        *v += (local::srgb_decode(e) - *v) * m;
                    }
                });
        }
    }

    /// Saturation and the Color tint, in OkLab.
    pub fn apply_color(&self, img: &mut Image, k: &MaskStrengths) {
        let ok = OkLab::new();
        for (a, alpha) in self.each(|a| a.saturation != 0.0 || a.toning_saturation != 0.0) {
            let sat = (a.saturation / 100.0) as f32;
            // Lightroom's hue 0 is red, about 29° in OkLab.
            let h = (a.toning_hue + 29.0).to_radians();
            let tint = (k.toning * a.toning_saturation / 100.0) as f32;
            let (ta, tb) = (tint * h.cos() as f32, tint * h.sin() as f32);
            img.rgb
                .par_chunks_mut(3)
                .zip(alpha.par_iter())
                .for_each(|(p, &m)| {
                    if m <= 0.0 {
                        return;
                    }
                    let mut lab = ok.from_working([p[0], p[1], p[2]]);
                    let s = (1.0 + sat * m).max(0.0);
                    lab[1] = lab[1] * s + ta * m;
                    lab[2] = lab[2] * s + tb * m;
                    let rgb = ok.to_working(lab);
                    p.copy_from_slice(&rgb);
                });
        }
    }

    /// Local Sharpness (added sharpening, or a soft blur below 0) and Noise.
    pub fn apply_detail(&self, img: &mut Image, cal: &Calibration) {
        let k = &cal.masks;
        for (a, alpha) in self.each(|a| a.noise > 0.0) {
            let mut alt = img.clone();
            let nr = NoiseReduction {
                luminance: a.noise,
                luminance_detail: 50.0,
                luminance_contrast: 0.0,
                color: 0.0,
                color_detail: 50.0,
                color_smoothness: 50.0,
            };
            detail::apply_noise_reduction(&mut alt, &nr, &cal.detail);
            blend(img, &alt, alpha);
        }
        for (a, alpha) in self.each(|a| a.sharpness != 0.0) {
            let mut alt = img.clone();
            if a.sharpness > 0.0 {
                let s = Sharpening {
                    amount: k.sharpness * a.sharpness / 100.0,
                    radius: 1.0,
                    detail: 25.0,
                    masking: 0.0,
                };
                detail::apply_sharpening(&mut alt, &s, &cal.detail);
            } else {
                let sigma = (k.soften_sigma * -a.sharpness / 100.0) as f32 * self.tile.scale();
                let (w, h) = (img.width, img.height);
                for c in 0..3 {
                    let plane: Vec<f32> = img.rgb.iter().skip(c).step_by(3).copied().collect();
                    let blurred = gaussian_blur(&plane, w, h, sigma.max(0.3));
                    for (i, v) in blurred.into_iter().enumerate() {
                        alt.rgb[i * 3 + c] = v;
                    }
                }
            }
            blend(img, &alt, alpha);
        }
    }

    /// Tint the shown mask red, as Lightroom's overlay does.
    pub fn apply_overlay(&self, img: &mut Image) {
        let Some(alpha) = &self.overlay else { return };
        let red = [0.55f32, 0.04, 0.04];
        img.rgb
            .par_chunks_mut(3)
            .zip(alpha.par_iter())
            .for_each(|(p, &m)| {
                let a = 0.6 * m;
                for c in 0..3 {
                    p[c] += (red[c] - p[c]) * a;
                }
            });
    }
}
