// SPDX-License-Identifier: GPL-3.0-or-later
//! Camera colour to the working space: the camera matrix, white balance
//! (As Shot, custom, incremental), highlight unclipping and the first stage.

use super::*;

/// Linear ProPhoto to linear sRGB, and back.
pub(crate) fn srgb_matrices() -> ([[f32; 3]; 3], [[f32; 3]; 3]) {
    use amb_color::matrix::xyz_d50_to_srgb;
    let to_srgb = mul3(&xyz_d50_to_srgb(), &PROPHOTO_TO_XYZ_D50);
    let to_pro = invert3(&to_srgb);
    let m = |m: &[[f64; 3]; 3]| m.map(|r| r.map(|v| v as f32));
    (m(&to_srgb), m(&to_pro))
}

/// A `.cube` look on display-linear ProPhoto: through sRGB-encoded values,
/// as LUTs expect (colours outside sRGB are clipped for the table), mixed
/// with the original by the amount.
pub(crate) fn apply_lut(out: &mut ProPhotoImage, l: &LookLut) {
    let (to_srgb, to_pro) = srgb_matrices();
    let mat = |m: &[[f32; 3]; 3], c: [f32; 3]| {
        [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2])
    };
    let amount = l.amount as f32;
    out.map_pixels(|c| {
        let srgb = mat(&to_srgb, c);
        let enc = srgb.map(|v| srgb_encode(v.clamp(0.0, 1.0)));
        let looked = l.table.apply(enc);
        let mixed = [0, 1, 2].map(|i| {
            let lin = srgb_decode(looked[i].clamp(0.0, 1.0));
            srgb[i] + (lin - srgb[i]) * amount
        });
        mat(&to_pro, mixed)
    });
}

/// White against which Temperature/Tint are read on a converted negative
/// (the conversion itself already balances the image).
pub const NEGATIVE_REFERENCE_WB: (f64, f64) = (5500.0, 0.0);

/// Camera RGB → linear ProPhoto matrix for a given white (camera-space neutral).
/// The neutral maps to ProPhoto (1, 1, 1) (i.e. D50 white).
pub fn camera_to_prophoto(xyz_to_cam: &Mat3, neutral: [f64; 3]) -> Mat3 {
    let cam_to_xyz = invert3(xyz_to_cam);
    let white = apply3(&cam_to_xyz, neutral);
    let white = [white[0] / white[1], 1.0, white[2] / white[1]];
    let adapt = bradford(white, D50_XYZ);
    let m = mul3(&invert3(&PROPHOTO_TO_XYZ_D50), &mul3(&adapt, &cam_to_xyz));
    // Scale so the neutral lands exactly on 1.0 (removes the neutral's own magnitude).
    let n = apply3(&m, neutral);
    let k = 1.0 / n[1];
    m.map(|row| row.map(|v| v * k))
}

/// Clipped highlights: where a channel reached its clip level its real value
/// is unknown, and white balance would scale the clipped channels unequally
/// (white turns pink or cyan). Clipped channels are raised towards the
/// brightest white-balanced channel, so blown areas go neutral and keep
/// their brightness for the tone curve to roll off. A channel also counts as
/// clipped next to where it clips: demosaicing mixes the clipped values into
/// the edge, which would otherwise leave a coloured fringe.
pub(crate) struct Unclip {
    pub(crate) clip: [f32; 3],
    /// White-balance gains (1 / neutral).
    pub(crate) gains: [f32; 3],
    /// Only where every channel clipped: partly clipped pixels were rebuilt
    /// ([`crate::highlights`]).
    pub(crate) full_only: bool,
}

impl Unclip {
    pub(super) fn new(clip: [f32; 3], neutral: [f64; 3], full_only: bool) -> Self {
        Self {
            clip,
            gains: neutral.map(|n| if n > 1e-9 { (1.0 / n) as f32 } else { 1.0 }),
            full_only,
        }
    }

    /// How clipped each channel is: 0 below 90% of its clip level, 1 from 98%.
    pub(super) fn weights(&self, c: &[f32]) -> [f32; 3] {
        let w: [f32; 3] = std::array::from_fn(|i| {
            let k = self.clip[i];
            if !k.is_finite() {
                return 0.0;
            }
            let t = ((c[i] - 0.9 * k) / (0.08 * k)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        });
        if self.full_only {
            [w[0].min(w[1]).min(w[2]); 3]
        } else {
            w
        }
    }

    /// The weights of row `y`, each the most clipped within `r` pixels.
    pub(super) fn row_weights(
        &self,
        rgb: &[f32],
        w: usize,
        h: usize,
        y: usize,
        r: usize,
        out: &mut Vec<[f32; 3]>,
    ) {
        out.clear();
        out.resize(w, [0.0; 3]);
        for sy in y.saturating_sub(r)..(y + r + 1).min(h) {
            let row = &rgb[sy * w * 3..(sy + 1) * w * 3];
            for (x, p) in row.chunks(3).enumerate() {
                let k = self.weights(p);
                if k == [0.0; 3] {
                    continue;
                }
                for slot in &mut out[x.saturating_sub(r)..(x + r + 1).min(w)] {
                    for i in 0..3 {
                        slot[i] = slot[i].max(k[i]);
                    }
                }
            }
        }
    }

    pub(super) fn apply(&self, c: [f32; 3], w: [f32; 3]) -> [f32; 3] {
        if w == [0.0; 3] {
            return c;
        }
        let balanced: [f32; 3] = std::array::from_fn(|i| c[i] * self.gains[i]);
        let top = balanced[0].max(balanced[1]).max(balanced[2]);
        std::array::from_fn(|i| (balanced[i] + w[i] * (top - balanced[i])) / self.gains[i])
    }
}

/// Relative white balance: mireds per step of `IncrementalTemperature`, and
/// Tint units per step of `IncrementalTint`. Adobe doesn't document its
/// scale; these are ours, unfitted.
pub(super) const INCREMENTAL_MIRED_PER_STEP: f64 = 1.0;

pub(super) const INCREMENTAL_TINT_PER_STEP: f64 = 1.0;

/// Temperature and Tint of a rendered image's own white (equal RGB).
pub(super) fn rendered_white(img: &CameraImage) -> TempTint {
    let (x, y) = xy_for_neutral(&img.xyz_to_cam, [1.0; 3]);
    xy_to_temp_tint(x, y)
}

/// Camera-space white for relative white balance: the image's own white,
/// moved in mireds (+ warmer) and tint (+ magenta), as Kelvin and Tint do on raws.
pub fn incremental_neutral(img: &CameraImage, wb: IncrementalWhiteBalance) -> [f64; 3] {
    if wb == IncrementalWhiteBalance::default() {
        return [1.0; 3];
    }
    let base = rendered_white(img);
    let mired = (1e6 / base.temperature - INCREMENTAL_MIRED_PER_STEP * wb.temperature).max(20.0);
    let (x, y) = temp_tint_to_xy(1e6 / mired, base.tint + INCREMENTAL_TINT_PER_STEP * wb.tint);
    let (bx, by) = temp_tint_to_xy(base.temperature, base.tint);
    let n = neutral_for_xy(&img.xyz_to_cam, x, y);
    let r = neutral_for_xy(&img.xyz_to_cam, bx, by);
    [n[0] / r[0], 1.0, n[2] / r[2]]
}

/// The relative white balance that makes the white (x, y) neutral on a
/// rendered image: the inverse of [`incremental_neutral`].
pub fn incremental_for_xy(img: &CameraImage, x: f64, y: f64) -> IncrementalWhiteBalance {
    let base = rendered_white(img);
    let t = xy_to_temp_tint(x, y);
    IncrementalWhiteBalance {
        temperature: (1e6 / base.temperature - 1e6 / t.temperature) / INCREMENTAL_MIRED_PER_STEP,
        tint: (t.tint - base.tint) / INCREMENTAL_TINT_PER_STEP,
    }
}

/// A camera-style white balance shift, per step: red minus blue warms by
/// this many mireds, and red plus blue adds this much Tint (magenta).
/// **Ours**, not measured against a camera yet.
pub const WB_SHIFT_MIRED: f64 = 4.0;
pub const WB_SHIFT_TINT: f64 = 3.0;

/// The white (x, y) moved by a white balance shift (`[red, blue]` steps).
pub fn wb_shifted(x: f64, y: f64, shift: [f64; 2]) -> (f64, f64) {
    let t = xy_to_temp_tint(x, y);
    let mired = (1e6 / t.temperature - WB_SHIFT_MIRED * (shift[0] - shift[1])).max(20.0);
    temp_tint_to_xy(1e6 / mired, t.tint + WB_SHIFT_TINT * (shift[0] + shift[1]))
}

/// Temperature/Tint that Lightroom would show for this image's as-shot white.
pub fn as_shot_temp_tint(img: &CameraImage) -> (f64, f64) {
    let (x, y) = img.white_xy(img.as_shot_neutral);
    let tt = xy_to_temp_tint(x, y);
    (tt.temperature, tt.tint)
}

/// Stage 0's result: camera RGB after Lens Corrections, Transform, crop and
/// straighten (one resample), and a film negative turned positive.
pub(crate) struct CameraStage {
    pub crop: Option<crate::crop::CropGeometry>,
    /// The output's size (the crop's, or the photo's).
    pub out: (usize, usize),
    pub tile: Tile,
    pub full: (usize, usize),
    /// The size of the rendered part.
    pub width: usize,
    pub height: usize,
    pub lens_pass: Option<lens::CameraPass>,
    pub(super) negative_rgb: Option<Vec<f32>>,
    pub negative: Option<negative::Analysis>,
}

impl CameraStage {
    /// The camera RGB the next stages start from.
    pub fn rgb<'a>(&'a self, img: &'a CameraImage) -> &'a [f32] {
        match (&self.negative_rgb, &self.lens_pass) {
            (Some(rgb), _) => rgb,
            (None, Some(p)) => &p.rgb,
            (None, None) => &img.rgb,
        }
    }
}

/// Stage 0 for `img` developed with `s`.
pub(crate) fn camera_stage(img: &CameraImage, s: &DevelopSettings, pass: &Pass) -> CameraStage {
    let cal = &s.calibration;
    let lens = s.lens.for_image(&img.shot);
    let crop = s.crop.geometry(img.width, img.height);
    let mut transform = s.transform;
    if transform.upright.is_none() && transform.upright_mode.is_solved() {
        transform.upright =
            upright::analyse(img, &lens, &cal.lens, transform.upright_mode, &cal.upright);
    }
    let warp = transform.sampling_map(img.width, img.height, &cal.transform);
    let (out_w, out_h) = crop
        .as_ref()
        .map_or((img.width, img.height), |c| (c.width, c.height));
    let tile = match pass.window {
        Some(r) => Tile {
            x: r.x,
            y: r.y,
            full_width: out_w,
            full_height: out_h,
        },
        None => Tile::whole(out_w, out_h),
    };
    let full = pass.full.unwrap_or((out_w, out_h));
    let lens_pass =
        (lens.needs_camera_pass() || crop.is_some() || warp.is_some() || pass.window.is_some())
            .then(|| {
                lens::apply_camera_window(
                    &img.rgb,
                    img.width,
                    img.height,
                    img.shot.crop_factor,
                    &lens,
                    &cal.lens,
                    crop.as_ref(),
                    warp.as_ref(),
                    pass.window,
                )
            });
    let (width, height) = lens_pass
        .as_ref()
        .map_or((img.width, img.height), |p| (p.width, p.height));
    let camera_rgb = lens_pass.as_ref().map_or(&img.rgb, |p| &p.rgb);
    // 0b. Film negative → positive (on camera RGB, before white balance):
    // the film base from the whole scan (the rebate is outside the crop),
    // the levels inside the crop.
    let mut negative = None;
    let negative_rgb = s.negative.enabled.then(|| {
        let a = match pass.given.and_then(|g| g.negative) {
            Some(a) => a,
            None => {
                let whole = negative::Scan {
                    rgb: &img.rgb,
                    width: img.width,
                    height: img.height,
                };
                let metered = negative::Scan {
                    rgb: camera_rgb,
                    width,
                    height,
                };
                negative::resolve(&s.negative, whole, metered, &cal.negative)
            }
        };
        negative = Some(a);
        let conversion = negative::Conversion::new(&s.negative, a, &cal.negative);
        let mut rgb = camera_rgb.clone();
        negative::apply(&mut rgb, &conversion);
        rgb
    });

    CameraStage {
        crop,
        out: (out_w, out_h),
        tile,
        full,
        width,
        height,
        lens_pass,
        negative_rgb,
        negative,
    }
}

/// Stage 1's parameters: camera RGB to scene-linear ProPhoto, with white
/// balance, the camera matrix (or profile), the Calibration panel and
/// Exposure, and the recovery of clipped highlights.
pub(crate) struct CameraToWorking<'a> {
    pub profile: Option<&'a crate::profile::Dcp>,
    pub m: [[f32; 3]; 3],
    pub gain: f32,
    pub hue_sat_map: Option<std::borrow::Cow<'a, crate::profile::HsvTable>>,
    pub look_table: Option<&'a crate::profile::HsvTable>,
    pub gain_before_look: f32,
    pub gain_after_look: f32,
    pub shadow_tint: Option<(f32, f32)>,
    pub clipped: Option<Unclip>,
    /// The window of most-clipped neighbours: wider on large photos.
    pub unclip_reach: usize,
}

/// Stage 1's parameters for `img` developed with `s`, at output size `out`.
pub(crate) fn camera_to_working<'a>(
    img: &CameraImage,
    s: &'a DevelopSettings,
    out: (usize, usize),
) -> CameraToWorking<'a> {
    let cal = &s.calibration;
    // A camera profile brings its own matrices, tables and tone curve (not
    // for rendered images or converted negatives).
    let profile = s
        .camera_profile
        .as_deref()
        .filter(|_| !img.rendered && !s.negative.enabled);
    let neutral = match (s.white_balance.clone(), s.negative.enabled) {
        // Rendered images are balanced already: white is equal RGB, moved by
        // the relative sliders.
        _ if img.rendered => incremental_neutral(img, s.incremental_wb),
        (WhiteBalance::AsShot, false) => img.as_shot_neutral,
        (WhiteBalance::Custom { temperature, tint }, false) => {
            let tint = tint.unwrap_or_else(|| as_shot_temp_tint(img).1);
            let (x, y) = temp_tint_to_xy(temperature, tint);
            match &profile {
                Some(p) => p.neutral_for_xy(x, y),
                None => img.neutral_for(x, y),
            }
        }
        // A converted negative is already balanced: equal channels are white,
        // and Temperature/Tint move relative to the reference white.
        (WhiteBalance::AsShot, true) => [1.0; 3],
        (WhiteBalance::Custom { temperature, tint }, true) => {
            let (x, y) = temp_tint_to_xy(temperature, tint.unwrap_or(0.0));
            let (rx, ry) = temp_tint_to_xy(NEGATIVE_REFERENCE_WB.0, NEGATIVE_REFERENCE_WB.1);
            let n = img.neutral_for(x, y);
            let r = img.neutral_for(rx, ry);
            [n[0] / r[0], 1.0, n[2] / r[2]]
        }
    };
    let neutral = match s.wb_shift {
        [0.0, 0.0] => neutral,
        _ if s.negative.enabled => neutral,
        shift if img.rendered => {
            let wb = s.incremental_wb;
            incremental_neutral(
                img,
                IncrementalWhiteBalance {
                    temperature: wb.temperature
                        + WB_SHIFT_MIRED * (shift[0] - shift[1]) / INCREMENTAL_MIRED_PER_STEP,
                    tint: wb.tint
                        + WB_SHIFT_TINT * (shift[0] + shift[1]) / INCREMENTAL_TINT_PER_STEP,
                },
            )
        }
        shift => {
            let (x, y) = img.white_xy(neutral);
            let (x, y) = wb_shifted(x, y, shift);
            match &profile {
                Some(p) => p.neutral_for_xy(x, y),
                None => img.neutral_for(x, y),
            }
        }
    };
    // The Calibration panel's primaries fold into the camera matrix.
    let mut m = match profile {
        Some(p) => p.camera_to_prophoto(neutral),
        None => img.to_prophoto(neutral),
    };
    if let Some(p) = s.camera_calibration.matrix(&cal.primaries) {
        m = mul3(&p, &m);
    }
    let m = m.map(|row| row.map(|v| v as f32));
    let shadow_tint = s.camera_calibration.shadow_tint(&cal.primaries);
    let gain = 2f32.powf(s.exposure_ev as f32);
    let hue_sat_map = profile.and_then(|p| p.hue_sat_map(p.white_temperature(neutral)));
    let look_table = profile.and_then(|p| p.look_table.as_ref());
    let profile_gain = profile.map_or(1.0, |p| 2f32.powf(p.exposure_offset as f32));
    // As in the DNG reference render, a brightening Exposure goes in before
    // the look table and a darkening one after it (DNG spec, look table).
    let (gain_before_look, gain_after_look) = if gain >= 1.0 {
        (profile_gain * gain, 1.0)
    } else {
        (profile_gain, gain)
    };
    let clipped = (!s.negative.enabled && img.clip.iter().any(|c| c.is_finite()))
        .then(|| Unclip::new(img.clip, neutral, img.highlights_rebuilt));
    let unclip_reach = if out.0.max(out.1) >= 2500 { 2 } else { 1 };
    CameraToWorking {
        profile,
        m,
        gain,
        hue_sat_map,
        look_table,
        gain_before_look,
        gain_after_look,
        shadow_tint,
        clipped,
        unclip_reach,
    }
}
