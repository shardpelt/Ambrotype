// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering the open photo: develop, previews, regions, pixel picks and the compare page.

use super::*;

impl Session {
    /// Develop settings for `settings`, with the lens profile resolved (cached).
    pub(super) fn develop(
        &mut self,
        settings: &BTreeMap<String, Value>,
        lens: &LensSources,
    ) -> DevelopSettings {
        let (mut develop, _) = DevelopSettings::from_settings(settings);
        if develop.lens.profile_enable || develop.lens.auto_ca {
            let key = develop.lens.profile_filename.clone();
            let cached = self.lens_cache.as_ref().filter(|(k, _)| *k == key);
            let profile = match cached {
                Some((_, p)) => p.clone(),
                None => {
                    let mut shot = self.full.shot.clone();
                    if shot.crop_factor.is_none() {
                        shot.crop_factor = lens
                            .db
                            .as_ref()
                            .and_then(|db| db.camera_crop(&shot.camera_make, &shot.camera_model));
                    }
                    self.full.shot.crop_factor = shot.crop_factor;
                    let hint = amb_lens::ProfileHint {
                        name: develop.lens.profile_name.clone(),
                        filename: key.clone(),
                    };
                    let p = amb_lens::resolve(&shot, &hint, lens.lcp.as_ref(), lens.db.as_ref());
                    self.lens_cache = Some((key, p.clone()));
                    p
                }
            };
            develop.lens.profile = profile;
        }
        if let Some(name) = develop.profile_name.clone() {
            develop.camera_profile = (!self.full.rendered)
                .then(|| lens.dcp.find(&self.full.make, &self.full.model, &name))
                .flatten();
        }
        self.profile_note = develop.profile_note();
        let mode = develop.transform.upright_mode;
        if mode.is_solved() {
            let l = &develop.lens;
            let key = format!(
                "{mode:?} {:?} {} {} {} {} {}",
                l.profile_filename,
                l.profile_enable && l.profile.is_some(),
                l.distortion_scale,
                l.manual_distortion,
                l.constrain_crop,
                l.auto_ca,
            );
            let found = match self.upright_cache.as_ref().filter(|(k, _)| *k == key) {
                Some((_, u)) => *u,
                None => {
                    let cal = &develop.calibration;
                    let u = amb_engine::upright::analyse(
                        &self.full,
                        &develop.lens,
                        &cal.lens,
                        mode,
                        &cal.upright,
                    );
                    self.upright_cache = Some((key, u));
                    u
                }
            };
            // `None` (too few lines) stays uncorrected rather than being
            // searched again on the preview.
            develop.transform.upright = Some(found.unwrap_or(NO_UPRIGHT));
        }
        develop
    }

    /// Render a preview at most `max_edge` pixels on the long side. `overrides`
    /// are applied on top of the state without touching history (slider drags);
    /// `before` renders with no edits.
    pub fn preview(
        &mut self,
        max_edge: usize,
        overrides: &BTreeMap<String, Value>,
        before: bool,
        clipping: bool,
        lens: &LensSources,
    ) -> Preview {
        self.preview_masked(max_edge, overrides, before, clipping, lens, None, None)
    }

    /// [`preview`](Self::preview), with mask `overlay` (its position in
    /// `MaskGroupBasedCorrections`) tinted red as the mask tool shows it.
    #[allow(clippy::too_many_arguments)]
    pub fn preview_masked(
        &mut self,
        max_edge: usize,
        overrides: &BTreeMap<String, Value>,
        before: bool,
        clipping: bool,
        lens: &LensSources,
        overlay: Option<usize>,
        gpu: Option<&amb_gpu::Gpu>,
    ) -> Preview {
        let start = Instant::now();
        let settings = self.effective_settings(overrides, before);
        let mut develop = self.develop(&settings, lens);
        develop.masks.overlay = overlay;
        // `max_edge` is for what is shown: a crop needs a larger source to
        // come out at that size.
        let long = self.full.width.max(self.full.height);
        let crop_long = develop
            .crop
            .geometry(self.full.width, self.full.height)
            .map_or(long, |g| g.width.max(g.height).max(1));
        let max_edge = (max_edge as f64 * long as f64 / crop_long as f64).round() as usize;
        let max_edge = max_edge.clamp(256, 4096);
        if self.preview_src.as_ref().map(|(e, _)| *e as usize) != Some(max_edge) {
            self.preview_src = Some((max_edge as u32, downscale(&self.full, max_edge)));
        }
        let src = &self.preview_src.as_ref().expect("set above").1;
        let lens_profile =
            (develop.lens.for_image(&src.shot).active_profile()).map(|c| c.description.clone());
        // On the GPU when there is one and the edit allows it.
        let plan = gpu.map(|g| (g, amb_engine::plan::plan(src, &develop)));
        let (developed, gpu) = match plan {
            Some((g, Ok(plan))) => (g.render(&plan), Some(Ok(g.name()))),
            Some((_, Err(why))) => (pipeline::render(src, &develop), Some(Err(why))),
            None => (pipeline::render(src, &develop), None),
        };
        let upright = amb_export::geometry::orient(&developed, self.full.metadata.orientation);
        let (mut bytes, gamut) = display(self.proof, &upright);
        let mut histogram = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
        for p in bytes.chunks(3) {
            for c in 0..3 {
                histogram[c][p[c] as usize] += 1;
            }
        }
        if let Some(out) = &gamut {
            mark_gamut(&mut bytes, out);
        }
        if clipping {
            mark_clipping(&mut bytes);
        }
        Preview {
            jpeg: encode_jpeg(
                self.on_screen,
                &mut bytes,
                upright.width,
                upright.height,
                88,
            ),
            width: upright.width,
            height: upright.height,
            histogram,
            millis: start.elapsed().as_millis(),
            lens_profile,
            gpu,
        }
    }

    /// The settings to render: the state (or, for "before", only its framing:
    /// crop, geometry and lens corrections, so it lines up with the edit in the
    /// split view) plus uncommitted slider overrides.
    pub(super) fn effective_settings(
        &self,
        overrides: &BTreeMap<String, Value>,
        before: bool,
    ) -> BTreeMap<String, Value> {
        if before {
            return amb_protocol::catalog::Catalog::builtin().framing(self.state.settings());
        }
        let mut settings = self.state.settings().clone();
        for (k, v) in overrides {
            settings.insert(k.clone(), v.clone());
        }
        // A Temperature/Tint drag implies a custom white balance.
        if [
            "Temperature",
            "Tint",
            "IncrementalTemperature",
            "IncrementalTint",
        ]
        .iter()
        .any(|k| overrides.contains_key(*k))
        {
            settings.insert("WhiteBalance".into(), Value::Text("Custom".into()));
        }
        settings
    }

    /// A 1:1 crop of the full-resolution render around (cx, cy) (fractions of
    /// the upright image), at most `max_w × max_h` pixels. Only the tiles the
    /// crop needs are rendered ([`pipeline::render_region`]); they are kept
    /// until the committed settings change, so panning back is instant.
    #[allow(clippy::too_many_arguments)]
    pub fn region(
        &mut self,
        cx: f64,
        cy: f64,
        max_w: usize,
        max_h: usize,
        before: bool,
        clipping: bool,
        lens: &LensSources,
    ) -> Region {
        let start = Instant::now();
        let settings = self.effective_settings(&BTreeMap::new(), before);
        let key = serde_json::to_string(&settings).unwrap_or_default();
        let develop = self.develop(&settings, lens);
        let orientation = self.full.metadata.orientation;
        let (sw, sh) = pipeline::output_size(&self.full, &develop);
        let (uw, uh) = amb_export::geometry::oriented_size(sw, sh, orientation);
        if self.region_cache.as_ref().map(|c| &c.key) != Some(&key) {
            let globals = if pipeline::needs_globals(&develop) {
                let full = &self.full;
                let small = self
                    .globals_src
                    .get_or_insert_with(|| raw::downscale(full, pipeline::GLOBALS_LONG_SIDE));
                pipeline::measure_globals(full, small, &develop)
            } else {
                pipeline::Globals::default()
            };
            let tiles = uw.div_ceil(REGION_TILE) * uh.div_ceil(REGION_TILE);
            self.region_cache = Some(RegionCache {
                key,
                srgb: vec![0; uw * uh * 3],
                done: vec![false; tiles],
                globals,
            });
        }
        let proof = self.proof;
        let cache = self.region_cache.as_mut().expect("set above");
        let w = max_w.clamp(16, uw);
        let h = max_h.clamp(16, uh);
        let x = ((cx * uw as f64) as isize - w as isize / 2).clamp(0, (uw - w) as isize) as usize;
        let y = ((cy * uh as f64) as isize - h as isize / 2).clamp(0, (uh - h) as isize) as usize;

        // Render the tiles the crop needs that aren't rendered yet, as one
        // rectangle (the filters' margin is paid once).
        let tiles_x = uw.div_ceil(REGION_TILE);
        let (tx0, tx1) = (x / REGION_TILE, (x + w).div_ceil(REGION_TILE));
        let (ty0, ty1) = (y / REGION_TILE, (y + h).div_ceil(REGION_TILE));
        let missing: Vec<(usize, usize)> = (ty0..ty1)
            .flat_map(|ty| (tx0..tx1).map(move |tx| (tx, ty)))
            .filter(|&(tx, ty)| !cache.done[ty * tiles_x + tx])
            .collect();
        if !missing.is_empty() {
            let (mx0, mx1) = missing.iter().fold((usize::MAX, 0), |(a, b), &(tx, _)| {
                (a.min(tx), b.max(tx + 1))
            });
            let (my0, my1) = missing.iter().fold((usize::MAX, 0), |(a, b), &(_, ty)| {
                (a.min(ty), b.max(ty + 1))
            });
            let (rx, ry) = (mx0 * REGION_TILE, my0 * REGION_TILE);
            let rw = (mx1 * REGION_TILE).min(uw) - rx;
            let rh = (my1 * REGION_TILE).min(uh) - ry;
            // The same rectangle in the stored orientation the engine renders.
            let corner = |ux: usize, uy: usize| {
                amb_export::geometry::stored_position(orientation, sw, sh, ux, uy)
            };
            let (ax, ay) = corner(rx, ry);
            let (bx, by) = corner(rx + rw - 1, ry + rh - 1);
            let stored = amb_engine::buffer::Rect {
                x: ax.min(bx),
                y: ay.min(by),
                width: ax.abs_diff(bx) + 1,
                height: ay.abs_diff(by) + 1,
            };
            let part = pipeline::render_region(&self.full, &develop, stored, &cache.globals);
            let upright = amb_export::geometry::orient(&part, orientation);
            let (mut srgb, gamut) = display(proof, &upright);
            if let Some(out) = &gamut {
                mark_gamut(&mut srgb, out);
            }
            for row in 0..rh {
                let dst = ((ry + row) * uw + rx) * 3;
                cache.srgb[dst..dst + rw * 3]
                    .copy_from_slice(&srgb[row * rw * 3..(row + 1) * rw * 3]);
            }
            for ty in my0..my1 {
                for tx in mx0..mx1 {
                    cache.done[ty * tiles_x + tx] = true;
                }
            }
        }
        let mut crop = Vec::with_capacity(w * h * 3);
        for row in y..y + h {
            let start = (row * uw + x) * 3;
            crop.extend_from_slice(&cache.srgb[start..start + w * 3]);
        }
        if clipping {
            mark_clipping(&mut crop);
        }
        Region {
            jpeg: encode_jpeg(self.on_screen, &mut crop, w, h, 92),
            x,
            y,
            width: w,
            height: h,
            full_width: uw,
            full_height: uh,
            millis: start.elapsed().as_millis(),
        }
    }

    /// The 100% view's crop while a slider moves (`overrides`): rendered
    /// from a copy at a fraction of full size, so it keeps up, and shown
    /// enlarged until the full-size crop replaces it. The position and size
    /// are in full-size pixels, as [`Session::region`] gives them.
    pub fn region_draft(
        &mut self,
        cx: f64,
        cy: f64,
        max_w: usize,
        max_h: usize,
        overrides: &BTreeMap<String, Value>,
        lens: &LensSources,
    ) -> Region {
        let start = Instant::now();
        let settings = self.effective_settings(overrides, false);
        let develop = self.develop(&settings, lens);
        let orientation = self.full.metadata.orientation;
        let (sw, sh) = pipeline::output_size(&self.full, &develop);
        let (uw, uh) = amb_export::geometry::oriented_size(sw, sh, orientation);
        let w = max_w.clamp(16, uw);
        let h = max_h.clamp(16, uh);
        let x = ((cx * uw as f64) as isize - w as isize / 2).clamp(0, (uw - w) as isize) as usize;
        let y = ((cy * uh as f64) as isize - h as isize / 2).clamp(0, (uh - h) as isize) as usize;
        // About half a megapixel of draft, at least half size down.
        let factor = ((w * h) as f64 / 500_000.0).sqrt().max(2.0);
        let long = self.full.width.max(self.full.height);
        let edge = ((long as f64 / factor).round() as usize).max(256);
        if self.draft_src.as_ref().map(|(e, _)| *e) != Some(edge) {
            self.draft_src = Some((edge, downscale(&self.full, edge)));
        }
        let src = &self.draft_src.as_ref().expect("set above").1;
        let (dsw, dsh) = pipeline::output_size(src, &develop);
        let (duw, duh) = amb_export::geometry::oriented_size(dsw, dsh, orientation);
        // The same window on the reduced copy.
        let (fx, fy) = (duw as f64 / uw as f64, duh as f64 / uh as f64);
        let dx = ((x as f64 * fx) as usize).min(duw.saturating_sub(1));
        let dy = ((y as f64 * fy) as usize).min(duh.saturating_sub(1));
        let dw = ((w as f64 * fx).ceil() as usize).clamp(1, duw - dx);
        let dh = ((h as f64 * fy).ceil() as usize).clamp(1, duh - dy);
        // The whole-photo measurements of the full-size crop on screen will
        // do for a draft (measuring them again each step costs more than the
        // draft itself).
        let globals = match &self.region_cache {
            Some(c) => c.globals.clone(),
            None if pipeline::needs_globals(&develop) => {
                let full = &self.full;
                let small = self
                    .globals_src
                    .get_or_insert_with(|| raw::downscale(full, pipeline::GLOBALS_LONG_SIDE));
                pipeline::measure_globals(src, small, &develop)
            }
            None => pipeline::Globals::default(),
        };
        let corner = |ux: usize, uy: usize| {
            amb_export::geometry::stored_position(orientation, dsw, dsh, ux, uy)
        };
        let (ax, ay) = corner(dx, dy);
        let (bx, by) = corner(dx + dw - 1, dy + dh - 1);
        let stored = amb_engine::buffer::Rect {
            x: ax.min(bx),
            y: ay.min(by),
            width: ax.abs_diff(bx) + 1,
            height: ay.abs_diff(by) + 1,
        };
        let part = pipeline::render_region(src, &develop, stored, &globals);
        let upright = amb_export::geometry::orient(&part, orientation);
        Region {
            jpeg: {
                let (mut bytes, gamut) = display(self.proof, &upright);
                if let Some(out) = &gamut {
                    mark_gamut(&mut bytes, out);
                }
                encode_jpeg(
                    self.on_screen,
                    &mut bytes,
                    upright.width,
                    upright.height,
                    85,
                )
            },
            x,
            y,
            width: w,
            height: h,
            full_width: uw,
            full_height: uh,
            millis: start.elapsed().as_millis(),
        }
    }

    /// The pixel of the photo (stored orientation, uncropped) under a point
    /// of it as shown: (`u`, `v`) are fractions of the upright photo, cropped
    /// when it has a crop.
    fn shown_pixel(&self, u: f64, v: f64) -> (usize, usize) {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let (w, h) = (self.full.width, self.full.height);
        let o = self.full.metadata.orientation;
        let g = develop.crop.geometry(w, h);
        let (cw, ch) = g.as_ref().map_or((w, h), |g| (g.width, g.height));
        let (s, t) = amb_export::geometry::stored_fraction(o, u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let (x, y) = (s * cw as f64 - 0.5, t * ch as f64 - 0.5);
        let (fx, fy) = g.as_ref().map_or((x, y), |g| g.source(x, y));
        (
            fx.round().clamp(0.0, (w - 1) as f64) as usize,
            fy.round().clamp(0.0, (h - 1) as f64) as usize,
        )
    }

    /// The mean camera RGB of a small patch around a point of the photo as
    /// shown ([`Self::shown_pixel`]); `None` on a clipped or black spot.
    pub(super) fn shown_patch(&self, u: f64, v: f64) -> Option<[f64; 3]> {
        let img = &self.full;
        let (sx, sy) = self.shown_pixel(u, v);
        // Average a small patch to beat noise.
        let r = (img.width.max(img.height) / 600).max(2);
        let mut acc = [0.0f64; 3];
        let mut n = 0.0;
        for y in sy.saturating_sub(r)..(sy + r + 1).min(img.height) {
            for x in sx.saturating_sub(r)..(sx + r + 1).min(img.width) {
                let i = (y * img.width + x) * 3;
                for (a, v) in acc.iter_mut().zip(&img.rgb[i..i + 3]) {
                    *a += f64::from(*v);
                }
                n += 1.0;
            }
        }
        let mean = acc.map(|a| a / n);
        (mean[1] > 1e-4 && mean.iter().all(|c| *c < 0.98 && *c > 0.0)).then_some(mean)
    }

    /// The photo developed with its current settings, full size and upright
    /// (working space), e.g. for a collage cell; on the GPU when there is one.
    pub fn render_upright(&mut self, lens: &LensSources) -> amb_engine::buffer::Image {
        let current = self.state.settings().clone();
        let develop = self.develop(&current, lens);
        let (developed, _) = amb_gpu::full::render(amb_gpu::full::shared(), &self.full, &develop);
        amb_export::geometry::orient(&developed, self.full.metadata.orientation)
    }

    /// A before/after page (`amb_export::compare`): the photo before its
    /// edits (framed as the edit is) and after, `long_edge` pixels on the
    /// long side, in one HTML file at `out`.
    pub fn compare_page(
        &mut self,
        out: &Path,
        long_edge: usize,
        lens: &LensSources,
    ) -> Result<(), String> {
        let after = self.state.settings().clone();
        let before = amb_protocol::catalog::Catalog::builtin().framing(&after);
        let src = downscale(&self.full, long_edge.max(256) * 2);
        let tmp = std::env::temp_dir().join(format!("amb-compare-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut settings = amb_export::ExportSettings::new(amb_export::Format::Jpeg);
        settings.quality = 88;
        settings.resize = amb_export::Resize::LongEdge(long_edge as u32);
        settings.metadata = amb_export::MetadataPolicy::None;
        let mut jpegs = Vec::new();
        for (name, s) in [("before", &before), ("after", &after)] {
            let develop = self.develop(s, lens);
            let developed = pipeline::render(&src, &develop);
            let file = tmp.join(format!("{name}.jpg"));
            let report = amb_export::export(&developed, &self.full.metadata, &settings, &file)
                .map_err(|e| e.to_string())?;
            jpegs.push((std::fs::read(&file).map_err(|e| e.to_string())?, report));
        }
        let _ = std::fs::remove_dir_all(&tmp);
        let (w, h) = (jpegs[1].1.width, jpegs[1].1.height);
        let title = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Photo".into());
        let html = amb_export::compare::page(&title, &jpegs[0].0, &jpegs[1].0, w, h);
        std::fs::write(out, html).map_err(|e| format!("{}: {e}", out.display()))
    }
}

pub(super) fn to_srgb8(img: &amb_engine::buffer::Image) -> Vec<u8> {
    amb_engine::output::to_srgb(img)
        .par_iter()
        .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// A render (linear ProPhoto) as display sRGB bytes, soft-proofed when
/// `proof` is on, and the pixels to mark as beyond the proof's gamut.
fn display(
    proof: Option<(amb_color::space::OutputSpace, bool)>,
    img: &amb_engine::buffer::Image,
) -> (Vec<u8>, Option<Vec<bool>>) {
    match proof {
        None => (to_srgb8(img), None),
        Some((space, warn)) => {
            let mut proofed = img.clone();
            let out = space.soft_proof(&mut proofed.rgb);
            (to_srgb8(&proofed), warn.then_some(out))
        }
    }
}

/// Soft proofing's gamut warning: colours the proof's space can't hold, in
/// magenta (red and blue are the clipping warning's).
fn mark_gamut(bytes: &mut [u8], out: &[bool]) {
    bytes
        .par_chunks_mut(3)
        .zip(out.par_iter())
        .filter(|(_, o)| **o)
        .for_each(|(p, _)| p.copy_from_slice(&[230, 40, 210]));
}

/// Lightroom-style clipping warning: clipped highlights red, crushed shadows blue.
fn mark_clipping(bytes: &mut [u8]) {
    bytes.par_chunks_mut(3).for_each(|p| {
        if p.iter().any(|c| *c >= 254) {
            p.copy_from_slice(&[255, 40, 40]);
        } else if p.iter().all(|c| *c <= 1) {
            p.copy_from_slice(&[40, 110, 255]);
        }
    });
}

/// `bytes` (sRGB) as a JPEG; for the screen, in the monitor's colours.
fn encode_jpeg(on_screen: bool, bytes: &mut [u8], w: usize, h: usize, quality: u8) -> Vec<u8> {
    if on_screen {
        crate::monitor::apply(bytes);
    }
    let mut jpeg = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut jpeg, quality);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
    let _ = enc.encode(bytes, w as u16, h as u16, jpeg_encoder::ColorType::Rgb);
    jpeg
}
