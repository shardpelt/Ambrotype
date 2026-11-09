// SPDX-License-Identifier: GPL-3.0-or-later
//! The editor tools on the open photo: crop, levelling, white and film-base picks, upright, negatives, auto and match look.

use super::*;

impl Session {
    /// The mask tool's coordinate map for the committed crop.
    pub fn mask_frame(&self) -> MaskFrame {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let (w, h) = (self.full.width, self.full.height);
        let o = self.full.metadata.orientation;
        let g = develop.crop.geometry(w, h);
        let (cw, ch) = g.as_ref().map_or((w, h), |g| (g.width, g.height));
        let to_frame = |u: f64, v: f64| -> (f64, f64) {
            let (s, t) = amb_export::geometry::stored_fraction(o, u, v);
            match &g {
                Some(g) => {
                    let (fx, fy) = g.source(s * cw as f64 - 0.5, t * ch as f64 - 0.5);
                    ((fx + 0.5) / w as f64, (fy + 0.5) / h as f64)
                }
                None => (s, t),
            }
        };
        let o0 = to_frame(0.0, 0.0);
        let ou = to_frame(1.0, 0.0);
        let ov = to_frame(0.0, 1.0);
        let (vw, vh) = amb_export::geometry::oriented_size(cw, ch, o);
        MaskFrame {
            matrix: [
                ou.0 - o0.0,
                ov.0 - o0.0,
                o0.0,
                ou.1 - o0.1,
                ov.1 - o0.1,
                o0.1,
            ],
            frame_width: w,
            frame_height: h,
            view_width: vw,
            view_height: vh,
        }
    }

    /// The committed crop, upright on the uncropped photo.
    pub fn crop_state(&self) -> CropState {
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let r = amb_export::geometry::upright_crop(
            &develop.crop,
            self.full.width,
            self.full.height,
            self.full.metadata.orientation,
        );
        CropState {
            enabled: develop.crop.enabled,
            rect: CropRect {
                cx: r.cx,
                cy: r.cy,
                width: r.width,
                height: r.height,
                angle: r.angle,
            },
            angle_sign: amb_engine::crop::ANGLE_SIGN,
        }
    }

    /// The straighten angle that levels the photo, in the crop tool's
    /// degrees (the upright view): Upright's Level fit, which turns the
    /// photo until its long lines near horizontal and vertical are. `None`
    /// when it has too few of them.
    pub fn level_angle(&mut self, lens: &LensSources) -> Option<f64> {
        let settings = self.state.settings().clone();
        let develop = self.develop(&settings, lens);
        let cal = &develop.calibration;
        let k = amb_engine::upright::analyse(
            &self.full,
            &develop.lens,
            &cal.lens,
            amb_engine::transform::UprightMode::Level,
            &cal.upright,
        )?;
        // The fit turns the photo by θ counter-clockwise, as a positive
        // CropAngle does; the upright view flips it for mirrored photos.
        let crop = amb_engine::crop::Crop {
            enabled: true,
            angle: k.theta.to_degrees() * amb_engine::crop::ANGLE_SIGN,
            ..Default::default()
        };
        let r = amb_export::geometry::upright_crop(
            &crop,
            self.full.width,
            self.full.height,
            self.full.metadata.orientation,
        );
        Some((r.angle * 100.0).round() / 100.0)
    }

    /// The photo's long straight lines for the crop tool to show and snap
    /// to while straightening, in upright pixels (the tool's frame).
    pub fn guide_lines(&mut self, lens: &LensSources) -> Vec<[f64; 4]> {
        let settings = self.state.settings().clone();
        let develop = self.develop(&settings, lens);
        let cal = &develop.calibration;
        let o = self.full.metadata.orientation;
        let (uw, uh) = amb_export::geometry::oriented_size(self.full.width, self.full.height, o);
        amb_engine::upright::guide_lines(&self.full, &develop.lens, &cal.lens, &cal.upright, 40)
            .into_iter()
            .map(|[x0, y0, x1, y1]| {
                let a = amb_export::geometry::upright_fraction(o, x0, y0);
                let b = amb_export::geometry::upright_fraction(o, x1, y1);
                [
                    a.0 * uw as f64,
                    a.1 * uh as f64,
                    b.0 * uw as f64,
                    b.1 * uh as f64,
                ]
            })
            .collect()
    }

    /// The commands that set the crop to `rect` (the crop tool's Done). The
    /// whole photo with no angle turns the crop off.
    pub fn crop_commands(&self, rect: &CropRect) -> Vec<Command> {
        let (w, h) = (self.full.width, self.full.height);
        let o = self.full.metadata.orientation;
        let (ow, oh) = amb_export::geometry::oriented_size(w, h, o);
        let (ow, oh) = (ow as f64, oh as f64);
        let set = |name: &str, value: serde_json::Value| Command::SetParam {
            name: name.into(),
            value,
        };
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        if near(rect.cx, ow / 2.0)
            && near(rect.cy, oh / 2.0)
            && near(rect.width, ow)
            && near(rect.height, oh)
            && rect.angle.abs() < 0.005
        {
            let mut cmds = vec![set("HasCrop", false.into())];
            cmds.extend(
                CROP_TAGS
                    .iter()
                    .map(|t| Command::ResetParam { name: (*t).into() }),
            );
            return cmds;
        }
        let crop = amb_export::geometry::stored_crop(
            &amb_export::geometry::UprightCrop {
                cx: rect.cx,
                cy: rect.cy,
                width: rect.width,
                height: rect.height,
                angle: rect.angle,
            },
            w,
            h,
            o,
        );
        // Six decimals like Lightroom; tiny overshoots from rounding stay in range.
        let frac = |v: f64| ((v * 1e6).round() / 1e6).clamp(0.0, 1.0);
        vec![
            set("HasCrop", true.into()),
            set("CropLeft", frac(crop.left).into()),
            set("CropTop", frac(crop.top).into()),
            set("CropRight", frac(crop.right).into()),
            set("CropBottom", frac(crop.bottom).into()),
            set("CropAngle", ((crop.angle * 1e4).round() / 1e4).into()),
        ]
    }

    /// The film base under a point of the photo as shown (the clear film
    /// between frames), and a warning when it is denser than the clearest
    /// film the scan holds, which suggests the picture rather than bare film.
    pub fn pick_film_base(&self, u: f64, v: f64) -> Option<([f64; 3], Option<String>)> {
        let base = self.shown_patch(u, v)?;
        let kind = amb_library::negative::film_type(self.state.settings());
        let small = downscale(&self.full, pipeline::GLOBALS_LONG_SIDE);
        let clearest = amb_engine::negative::estimate_base(
            &small.rgb,
            small.width,
            small.height,
            kind,
            &amb_engine::negative::DEFAULT_STRENGTHS,
        );
        let denser = (0..3).all(|c| (clearest[c] / base[c]).log10() > 0.15);
        let warning = denser.then(|| {
            "That spot is denser than the clearest film in the scan, so it may be part of the picture. \
             Pick the clear strip between frames (turn the crop off to see it)."
                .to_string()
        });
        Some((base, warning))
    }

    /// The white balance settings that make the spot at (`u`, `v`) of the
    /// photo as shown neutral: Temperature and Tint on a raw, their relative
    /// versions on a rendered image.
    pub fn pick_white(&self, u: f64, v: f64) -> Option<[(&'static str, f64); 2]> {
        let img = &self.full;
        let mean = self.shown_patch(u, v)?;
        let neutral = [mean[0] / mean[1], 1.0, mean[2] / mean[1]];
        let (x, y) = img.white_xy(neutral);
        if img.rendered {
            let wb = amb_engine::pipeline::incremental_for_xy(img, x, y);
            return (wb.temperature.is_finite() && wb.tint.is_finite()).then_some([
                (
                    "IncrementalTemperature",
                    wb.temperature.round().clamp(-100.0, 100.0),
                ),
                ("IncrementalTint", wb.tint.round().clamp(-100.0, 100.0)),
            ]);
        }
        let tt = amb_color::xy_to_temp_tint(x, y);
        (tt.temperature.is_finite() && tt.tint.is_finite()).then_some([
            ("Temperature", tt.temperature.round()),
            ("Tint", tt.tint.round()),
        ])
    }

    /// `cmds` with the as-shot white balance set first when they move
    /// Temperature or Tint from As Shot, so `temp+=500` starts from the
    /// camera's value (the protocol can't read it from the raw).
    pub fn white_balance_from_as_shot(&self, cmds: Vec<Command>) -> Vec<Command> {
        let settings = self.state.settings();
        let moves_wb = cmds.iter().any(|c| match c {
            Command::SetParam { name, .. } | Command::AdjustParam { name, .. } => {
                matches!(name.as_str(), "Temperature" | "Tint")
            }
            _ => false,
        });
        if !moves_wb || settings.contains_key("Temperature") || self.full.rendered {
            return cmds;
        }
        let info = self.info();
        let mut start = vec![Command::SetParam {
            name: "Temperature".into(),
            value: serde_json::json!(info.as_shot_temperature.round()),
        }];
        if !settings.contains_key("Tint") {
            start.push(Command::SetParam {
                name: "Tint".into(),
                value: serde_json::json!(info.as_shot_tint.round()),
            });
        }
        start.extend(cmds);
        start
    }

    /// Guided Upright: the commands that make `guides` (each `[u1, v1, u2,
    /// v2]`, fractions of the upright, uncropped photo) vertical or
    /// horizontal with the Transform sliders, keep the guides, and turn
    /// Upright off. Also the largest angle a guide is still off by.
    pub fn guided_upright(&self, guides: &[[f64; 4]]) -> Result<(Vec<Command>, f64), String> {
        use amb_engine::upright::{self, Segment};
        let o = self.full.metadata.orientation;
        let stored: Vec<Segment> = guides
            .iter()
            .map(|g| Segment {
                a: amb_export::geometry::stored_fraction(o, g[0], g[1]),
                b: amb_export::geometry::stored_fraction(o, g[2], g[3]),
            })
            .collect();
        let (develop, _) = DevelopSettings::from_settings(self.state.settings());
        let k = &develop.calibration.transform;
        let found = upright::guided(&stored, self.full.width, self.full.height, k)
            .ok_or("draw a guide along a line that should be vertical or horizontal")?;
        let set = |name: &str, v: f64| {
            if v == 0.0 {
                Command::ResetParam { name: name.into() }
            } else {
                Command::SetParam {
                    name: name.into(),
                    value: serde_json::json!(v),
                }
            }
        };
        let text: Vec<String> = stored
            .iter()
            .map(|s| format!("{:.5},{:.5},{:.5},{:.5}", s.a.0, s.a.1, s.b.0, s.b.1))
            .collect();
        Ok((
            vec![
                Command::ResetParam {
                    name: "PerspectiveUpright".into(),
                },
                set("PerspectiveVertical", found.vertical),
                set("PerspectiveHorizontal", found.horizontal),
                set("PerspectiveRotate", found.rotate),
                Command::SetParam {
                    name: "amb:UprightGuides".into(),
                    value: serde_json::json!(text.join(";")),
                },
            ],
            found.worst_degrees,
        ))
    }

    /// The guides kept by Guided Upright, as fractions of the upright,
    /// uncropped photo.
    pub fn upright_guides(&self) -> Vec<[f64; 4]> {
        let o = self.full.metadata.orientation;
        let Some(Value::Text(t)) = self.state.settings().get("amb:UprightGuides") else {
            return Vec::new();
        };
        t.split(';')
            .filter_map(|g| {
                let v: Vec<f64> = g.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                let [x1, y1, x2, y2] = v[..] else { return None };
                let a = amb_export::geometry::upright_fraction(o, x1, y1);
                let b = amb_export::geometry::upright_fraction(o, x2, y2);
                Some([a.0, a.1, b.0, b.1])
            })
            .collect()
    }

    /// The film negative analysis this photo renders with now (film base,
    /// levels, midtone), as Freeze stores it.
    pub fn negative_analysis(&self) -> amb_engine::negative::Analysis {
        amb_library::negative::analyze_image(&self.full, self.state.settings(), false)
    }

    /// The crop to a scan's frame (without rebate or holder edges), as
    /// commands; `None` when no border is found.
    pub fn negative_frame(&self) -> Option<Vec<Command>> {
        let kind = amb_library::negative::film_type(self.state.settings());
        let (l, t, r, b) = amb_engine::negative::detect_frame(
            &self.full.rgb,
            self.full.width,
            self.full.height,
            kind,
            &amb_engine::negative::DEFAULT_STRENGTHS,
        )?;
        let set = |name: &str, v: serde_json::Value| Command::SetParam {
            name: name.into(),
            value: v,
        };
        let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
        Some(vec![
            set("HasCrop", serde_json::json!(true)),
            set("CropLeft", serde_json::json!(round(l))),
            set("CropTop", serde_json::json!(round(t))),
            set("CropRight", serde_json::json!(round(r))),
            set("CropBottom", serde_json::json!(round(b))),
            Command::ResetParam {
                name: "CropAngle".into(),
            },
        ])
    }

    /// Lightroom-style Auto for the photo: tone and/or white balance.
    pub fn auto(
        &mut self,
        tone: bool,
        white: bool,
        lens: &LensSources,
    ) -> amb_engine::matching::MatchResult {
        use amb_engine::matching;
        let small = raw::downscale(&self.full, amb_library::look::EDGE);
        let base = self.state.settings().clone();
        let base_dev = self.develop(&base, lens);
        let develop = |s: &matching::Settings| {
            let mut d = matching::develop_plain(s);
            d.lens.profile = base_dev.lens.profile.clone();
            d.camera_profile = base_dev.camera_profile.clone();
            d.transform.upright = base_dev.transform.upright;
            d
        };
        matching::auto(&small, &base, tone, white, &develop)
    }

    /// The settings that bring the photo's look to `target`, searched on a
    /// small copy, with this photo's lens and camera profiles and Upright.
    pub fn match_look(
        &mut self,
        target: &amb_engine::matching::LookStats,
        parts: Vec<amb_engine::matching::Part>,
        lens: &LensSources,
    ) -> amb_engine::matching::MatchResult {
        use amb_engine::matching;
        let small = raw::downscale(&self.full, amb_library::look::EDGE);
        let base = self.state.settings().clone();
        let base_dev = self.develop(&base, lens);
        let develop = |s: &matching::Settings| {
            let mut d = matching::develop_plain(s);
            d.lens.profile = base_dev.lens.profile.clone();
            d.camera_profile = base_dev.camera_profile.clone();
            d.transform.upright = base_dev.transform.upright;
            d
        };
        let opts = matching::MatchOptions {
            parts,
            ..Default::default()
        };
        matching::solve(&small, &base, target, &opts, &develop)
    }
}
