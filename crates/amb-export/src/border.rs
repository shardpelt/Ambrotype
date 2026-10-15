// SPDX-License-Identifier: GPL-3.0-or-later
//! Borders: a frame around the exported photo (docs/design/collage-and-borders.md).
//!
//! A border is part of the export, applied after resizing and output
//! sharpening (so the frame's edge isn't sharpened into a halo), and saved and
//! shared as a small JSON preset:
//!
//! ```json
//! { "format": "ambrotype-border/1", "name": "Polaroid", "width": 4, "bottom": 16,
//!   "unit": "percent", "color": "#f7f5f0", "keyline": { "width": 0.15, "color": "#000000" },
//!   "radius": 0, "fit": false }
//! ```
//!
//! - `width` (each side) and `bottom` (defaults to `width`) are a percentage
//!   of the short edge, or pixels with `"unit": "px"`. The short edge is the
//!   output's for an exact size, otherwise the resized photo's.
//! - `color` is an sRGB hex colour, or `"auto"`: the photo's average colour.
//! - `keyline`: an optional thin line around the photo, inside the border.
//! - `radius`: rounded photo corners, a percentage of the photo's short edge.
//! - `fit`: with an exact size (e.g. Instagram's 1080×1350), the whole photo
//!   is fitted inside the frame and the rest filled with the border colour.
//!   Without it, the photo is cropped to fill the space inside the frame.
//!   Either way the file is exactly that size.
//! - `caption`: a line of text in the bottom band, such as the camera and
//!   exposure (`{ "text": "{camera} · {exposure}", "size": 30, "align":
//!   "center", "color": "#333333" }`). `text` takes the tokens in
//!   [`CAPTION_TOKENS`]; parts left empty (no lens known, say) are dropped
//!   with their separator. `size` is the text's height as a percentage of
//!   the bottom band; `color` defaults to dark on a light frame and light on
//!   a dark one. `font` is one of [`crate::fonts::FONTS`] (`"handwritten"`,
//!   `"marker"`, …; empty: Sans).

use crate::geometry::Resize;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-border/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Percent,
    Px,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyline {
    pub width: f64,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    #[serde(default = "default_caption")]
    pub text: String,
    #[serde(default = "default_caption_size")]
    pub size: f64,
    #[serde(default)]
    pub align: Align,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// A font from [`crate::fonts::FONTS`] (empty: the default).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub font: String,
}

pub const DEFAULT_CAPTION: &str = "{camera} · {exposure}";

fn default_caption() -> String {
    DEFAULT_CAPTION.into()
}

fn default_caption_size() -> f64 {
    30.0
}

impl Caption {
    pub fn new(text: &str) -> Caption {
        Caption {
            text: text.into(),
            size: default_caption_size(),
            align: Align::Center,
            color: None,
            font: String::new(),
        }
    }
}

/// The caption's tokens and what they stand for.
pub const CAPTION_TOKENS: &[(&str, &str)] = &[
    ("{camera}", "the camera (make and model)"),
    ("{lens}", "the lens"),
    ("{exposure}", "focal length, aperture, shutter and ISO"),
    ("{focal}", "the focal length, 35 mm"),
    ("{aperture}", "the aperture, f/1.8"),
    ("{shutter}", "the shutter speed, 1/250 s"),
    ("{iso}", "the ISO, ISO 100"),
    ("{date}", "the capture date, 2026-10-13"),
    ("{year}", "the capture year"),
    ("{title}", "the photo's title"),
    ("{creator}", "the creator"),
    ("{copyright}", "the copyright"),
];

/// A caption template filled in from the photo's metadata. Parts that come
/// out empty are dropped with their " · " or " | " separator.
pub fn caption_text(
    template: &str,
    meta: &amb_engine::raw::PhotoMetadata,
    tags: &crate::exif::Tags,
) -> String {
    let ratio = |r: Option<(u32, u32)>| {
        r.filter(|r| r.1 != 0)
            .map(|(n, d)| f64::from(n) / f64::from(d))
    };
    let num = |v: f64| {
        let s = format!("{v:.1}");
        s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
    };
    let focal = ratio(meta.focal_length)
        .map(|v| format!("{} mm", num(v)))
        .unwrap_or_default();
    let aperture = ratio(meta.fnumber)
        .map(|v| format!("f/{}", num(v)))
        .unwrap_or_default();
    let shutter = ratio(meta.exposure_time)
        .map(|v| {
            if v < 0.3 {
                format!("1/{:.0} s", 1.0 / v)
            } else {
                format!("{} s", num(v))
            }
        })
        .unwrap_or_default();
    let iso = meta.iso.map(|i| format!("ISO {i}")).unwrap_or_default();
    let exposure = [&focal, &aperture, &shutter, &iso]
        .into_iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    // "NIKON D750" already names its maker; "X-T4" doesn't.
    let make = meta.make.trim();
    let model = meta.model.trim();
    let first = make.split_whitespace().next().unwrap_or("").to_lowercase();
    let camera = if model.to_lowercase().starts_with(&first) || make.is_empty() {
        model.to_string()
    } else {
        format!("{make} {model}")
    };
    let date = meta
        .date_time_original
        .as_deref()
        .and_then(|d| d.get(..10))
        .map(|d| d.replace(':', "-"))
        .unwrap_or_default();
    let text = |o: &Option<String>| o.as_deref().unwrap_or("").trim().to_string();
    let creator = tags.creator.clone().or_else(|| meta.artist.clone());
    let copyright = tags.copyright.clone().or_else(|| meta.copyright.clone());
    let values = [
        ("{camera}", camera),
        ("{lens}", text(&meta.lens_model)),
        ("{exposure}", exposure),
        ("{focal}", focal),
        ("{aperture}", aperture),
        ("{shutter}", shutter),
        ("{iso}", iso),
        ("{year}", date.get(..4).unwrap_or("").to_string()),
        ("{date}", date),
        ("{title}", text(&tags.title)),
        ("{creator}", text(&creator)),
        ("{copyright}", text(&copyright)),
    ];
    let mut out = template.to_string();
    for (token, value) in &values {
        out = out.replace(token, value);
    }
    for sep in ["·", "|"] {
        if out.contains(sep) {
            out = out
                .split(sep)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(&format!(" {sep} "));
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Border {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub name: String,
    pub width: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f64>,
    #[serde(default)]
    pub unit: Unit,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyline: Option<Keyline>,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub fit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
}

fn default_format() -> String {
    FORMAT.into()
}

fn default_color() -> String {
    "#ffffff".into()
}

/// A colour for the frame: an sRGB value (0..1, encoded), or the photo's average.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    Srgb([f64; 3]),
    Auto,
}

pub fn parse_color(s: &str) -> Option<Fill> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(Fill::Auto);
    }
    let h = s.strip_prefix('#')?;
    let digits: Vec<u32> = h.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
    let rgb = match digits.len() {
        3 => [digits[0] * 17, digits[1] * 17, digits[2] * 17],
        6 => [
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        ],
        _ => return None,
    };
    Some(Fill::Srgb(rgb.map(|v| f64::from(v) / 255.0)))
}

impl Border {
    /// A plain border: `width` percent of the short edge in `color`.
    pub fn simple(width: f64, color: &str) -> Border {
        Border {
            format: FORMAT.into(),
            name: String::new(),
            width,
            bottom: None,
            unit: Unit::Percent,
            color: color.into(),
            keyline: None,
            radius: 0.0,
            fit: false,
            caption: None,
        }
    }

    pub fn from_json(text: &str) -> Result<Border, String> {
        let b: Border =
            serde_json::from_str(text).map_err(|e| format!("not a border preset: {e}"))?;
        b.validate()?;
        Ok(b)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The compact command-line form: `5%`, `40px`, `5%,#ffffff`, `4%,auto`.
    pub fn from_spec(spec: &str) -> Result<Border, String> {
        let (size, color) = match spec.split_once(',') {
            Some((s, c)) => (s.trim(), c.trim()),
            None => (spec.trim(), "#ffffff"),
        };
        let (num, unit) = if let Some(n) = size.strip_suffix('%') {
            (n, Unit::Percent)
        } else if let Some(n) = size.strip_suffix("px") {
            (n, Unit::Px)
        } else {
            (size, Unit::Percent)
        };
        let mut b = Border::simple(
            num.trim()
                .parse()
                .map_err(|_| format!("bad border width {size:?} (e.g. 5% or 40px)"))?,
            color,
        );
        b.unit = unit;
        b.validate()?;
        Ok(b)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("\"format\" must be \"{FORMAT}\""));
        }
        let max = match self.unit {
            Unit::Percent => 45.0,
            Unit::Px => 20_000.0,
        };
        for (what, v) in [("width", Some(self.width)), ("bottom", self.bottom)] {
            if let Some(v) = v {
                if !(0.0..=max).contains(&v) || !v.is_finite() {
                    return Err(format!("{what} must be 0 to {max}"));
                }
            }
        }
        if parse_color(&self.color).is_none() {
            return Err(format!(
                "bad colour {:?} (a hex colour such as #ffffff, or auto)",
                self.color
            ));
        }
        if let Some(k) = &self.keyline {
            if !(0.0..=max).contains(&k.width) {
                return Err(format!("keyline width must be 0 to {max}"));
            }
            if !matches!(parse_color(&k.color), Some(Fill::Srgb(_))) {
                return Err(format!("bad keyline colour {:?}", k.color));
            }
        }
        if !(0.0..=50.0).contains(&self.radius) {
            return Err("radius must be 0 to 50 (percent of the photo's short edge)".into());
        }
        if let Some(c) = &self.caption {
            if !crate::fonts::is_known(&c.font) {
                return Err(format!("no font {:?}", c.font));
            }
            if !(5.0..=90.0).contains(&c.size) {
                return Err("caption size must be 5 to 90 (percent of the bottom band)".into());
            }
            if let Some(colour) = &c.color
                && !matches!(parse_color(colour), Some(Fill::Srgb(_)))
            {
                return Err(format!("bad caption colour {colour:?}"));
            }
        }
        Ok(())
    }

    /// Pixels for a length in this border's unit, given the reference short edge.
    fn px(&self, v: f64, short: usize) -> usize {
        match self.unit {
            Unit::Percent => (v / 100.0 * short as f64).round() as usize,
            Unit::Px => v.round() as usize,
        }
    }

    /// Left/right/top width and the bottom width, in pixels.
    fn sides(&self, short: usize) -> (usize, usize) {
        let side = self.px(self.width, short);
        (side, self.px(self.bottom.unwrap_or(self.width), short))
    }
}

/// Where everything goes: the canvas, the photo's size and position on it,
/// and a crop of the upright photo to make first (for a filled exact size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub canvas: (usize, usize),
    pub photo: (usize, usize),
    pub at: (usize, usize),
    pub crop: Option<(usize, usize, usize, usize)>,
    pub keyline: usize,
    pub radius: usize,
}

/// Lay out a `w × h` upright photo with this border for the export's resize.
pub fn plan(b: &Border, resize: Resize, enlarge: bool, w: usize, h: usize) -> Plan {
    let keyline = |short| {
        b.keyline
            .as_ref()
            .map_or(0, |k| b.px(k.width, short).max(1))
    };
    if let Resize::Exact(cw, ch) = resize {
        let (cw, ch) = (cw.max(1) as usize, ch.max(1) as usize);
        let short = cw.min(ch);
        let (side, bottom) = b.sides(short);
        // The space inside the frame (at least a pixel).
        let iw = cw.saturating_sub(2 * side).max(1);
        let ih = ch.saturating_sub(side + bottom).max(1);
        let (photo, crop) = if b.fit {
            let k = (iw as f64 / w as f64).min(ih as f64 / h as f64);
            let pw = ((w as f64 * k).round() as usize).clamp(1, iw);
            let ph = ((h as f64 * k).round() as usize).clamp(1, ih);
            ((pw, ph), None)
        } else {
            (
                (iw, ih),
                Resize::Exact(iw as u32, ih as u32).cover_window(w, h),
            )
        };
        let at = (
            (side + (iw - photo.0) / 2).min(cw - 1),
            (side.min(ch - 1) + (ih - photo.1) / 2).min(ch - 1),
        );
        let p_short = photo.0.min(photo.1);
        return Plan {
            canvas: (cw, ch),
            photo,
            at,
            crop,
            keyline: keyline(short),
            radius: (b.radius / 100.0 * p_short as f64).round() as usize,
        };
    }
    let (pw, ph) = resize.target(w, h, enlarge);
    let short = pw.min(ph);
    let (side, bottom) = b.sides(short);
    Plan {
        canvas: (pw + 2 * side, ph + side + bottom),
        photo: (pw, ph),
        at: (side, side),
        crop: None,
        keyline: keyline(short),
        radius: (b.radius / 100.0 * short as f64).round() as usize,
    }
}

/// Put the (already resized, output-encoded) photo on the canvas: fill with the
/// frame colour, draw the keyline, then the photo with its corners rounded.
/// `frame` and `line` are output-space values, like the photo's.
pub fn compose(
    photo: &amb_engine::buffer::Image,
    p: &Plan,
    frame: [f32; 3],
    line: [f32; 3],
) -> amb_engine::buffer::Image {
    let (cw, ch) = p.canvas;
    let (ax, ay) = p.at;
    let (pw, ph) = (photo.width, photo.height);
    let k = p.keyline as isize;
    let r = (p.radius as f32).min(pw.min(ph) as f32 / 2.0);
    amb_engine::buffer::Image::from_fn(cw, ch, |x, y| {
        let (dx, dy) = (x as isize - ax as isize, y as isize - ay as isize);
        let inside = dx >= 0 && dy >= 0 && (dx as usize) < pw && (dy as usize) < ph;
        // The keyline is a band of width k around the photo's rectangle.
        let near = dx >= -k && dy >= -k && dx < pw as isize + k && dy < ph as isize + k;
        let under = if k > 0 && near { line } else { frame };
        if !inside {
            return under;
        }
        let px = photo.get(dx as usize, dy as usize);
        if r <= 0.0 {
            return px;
        }
        // Rounded corners: how much of this pixel is inside the rounded rectangle.
        let fx = dx as f32 + 0.5;
        let fy = dy as f32 + 0.5;
        let cx = fx.clamp(r, pw as f32 - r);
        let cy = fy.clamp(r, ph as f32 - r);
        let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
        let cover = (r - d + 0.5).clamp(0.0, 1.0);
        if cover >= 1.0 {
            px
        } else {
            [0, 1, 2].map(|i| px[i] * cover + under[i] * (1.0 - cover))
        }
    })
}

/// Set `text` in the bottom band of a composed border (`p` its plan), in
/// `colour` (output values). Too long a line is made smaller to fit.
pub fn draw_caption(
    img: &mut amb_engine::buffer::Image,
    p: &Plan,
    caption: &Caption,
    text: &str,
    colour: [f32; 3],
) {
    let top = p.at.1 + p.photo.1 + p.keyline;
    let band = p.canvas.1.saturating_sub(top);
    if band < 6 || text.trim().is_empty() {
        return;
    }
    let mut px = (band as f64 * caption.size / 100.0) as f32;
    let room = (p.canvas.0 as f32 * 0.92).max(1.0);
    let (mut cov, mut w, mut h) = crate::watermark::set_text(&caption.font, text, px);
    if w as f32 > room {
        px *= room / w as f32;
        (cov, w, h) = crate::watermark::set_text(&caption.font, text, px);
    }
    if w == 0 || h == 0 || h > band {
        return;
    }
    let x0 = match caption.align {
        Align::Left => p.at.0,
        Align::Center => p.canvas.0.saturating_sub(w) / 2,
        Align::Right => (p.at.0 + p.photo.0).saturating_sub(w),
    }
    .min(p.canvas.0.saturating_sub(w));
    let y0 = top + (band - h) / 2;
    for y in 0..h {
        for x in 0..w {
            let a = cov[y * w + x];
            if a <= 0.0 {
                continue;
            }
            let under = img.get(x0 + x, y0 + y);
            img.set(
                x0 + x,
                y0 + y,
                [0, 1, 2].map(|i| under[i] + (colour[i] - under[i]) * a),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_and_specs() {
        assert_eq!(parse_color("#fff"), Some(Fill::Srgb([1.0, 1.0, 1.0])));
        assert_eq!(parse_color("auto"), Some(Fill::Auto));
        assert_eq!(parse_color("#12345"), None);
        let b = Border::from_spec("40px,#000").unwrap();
        assert_eq!(
            (b.width, b.unit, b.color.as_str()),
            (40.0, Unit::Px, "#000")
        );
        assert!(Border::from_spec("5%,red").is_err());
        assert!(Border::from_spec("90%").is_err());
        let json = Border::from_spec("5%").unwrap().to_json();
        assert_eq!(
            Border::from_json(&json).unwrap(),
            Border::from_spec("5%").unwrap()
        );
    }

    #[test]
    fn plans() {
        // A free size grows by the border: 5% of a 1000×600 photo's short edge = 30 px.
        let b = Border::simple(5.0, "#fff");
        let p = plan(&b, Resize::None, false, 1000, 600);
        assert_eq!(
            (p.canvas, p.photo, p.at),
            ((1060, 660), (1000, 600), (30, 30))
        );
        // Polaroid: a deeper bottom.
        let mut pol = b.clone();
        pol.bottom = Some(15.0);
        assert_eq!(
            plan(&pol, Resize::None, false, 1000, 600).canvas,
            (1060, 720)
        );

        // An exact size stays exact. Fit: the whole landscape photo inside a portrait frame.
        let mut fit = Border::simple(4.0, "#fff");
        fit.fit = true;
        let p = plan(&fit, Resize::Exact(1080, 1350), false, 6000, 4000);
        assert_eq!(p.canvas, (1080, 1350));
        let side = (0.04f64 * 1080.0).round() as usize; // 43
        assert_eq!(
            p.photo,
            (
                1080 - 2 * side,
                ((1080 - 2 * side) as f64 / 1.5).round() as usize
            )
        );
        assert_eq!(p.at.0, side);
        assert!(p.crop.is_none());
        // Fill: cropped to the inside of the frame.
        let p = plan(
            &Border::simple(4.0, "#fff"),
            Resize::Exact(1080, 1350),
            false,
            6000,
            4000,
        );
        assert_eq!(
            (p.canvas, p.photo, p.at),
            ((1080, 1350), (994, 1264), (43, 43))
        );
        let (_, _, cw, chh) = p.crop.unwrap();
        assert!(((cw as f64 / chh as f64) - 994.0 / 1264.0).abs() < 0.002);
    }

    #[test]
    fn captions_fill_in_and_sit_in_the_bottom_band() {
        let mut meta = amb_engine::raw::PhotoMetadata {
            make: "NIKON CORPORATION".into(),
            model: "NIKON D750".into(),
            focal_length: Some((35, 1)),
            fnumber: Some((18, 10)),
            exposure_time: Some((1, 250)),
            iso: Some(100),
            date_time_original: Some("2026:10:13 09:30:00".into()),
            ..Default::default()
        };
        let tags = crate::exif::Tags::default();
        assert_eq!(
            caption_text(DEFAULT_CAPTION, &meta, &tags),
            "NIKON D750 · 35 mm · f/1.8 · 1/250 s · ISO 100"
        );
        // Unknown parts go with their separator.
        assert_eq!(
            caption_text("{lens} · {camera} | {year}", &meta, &tags),
            "NIKON D750 | 2026"
        );
        meta.make = "FUJIFILM".into();
        meta.model = "X-T4".into();
        assert_eq!(caption_text("{camera}", &meta, &tags), "FUJIFILM X-T4");

        let mut b = Border::simple(4.0, "#fff");
        b.bottom = Some(16.0);
        b.caption = Some(Caption::new("{camera}"));
        let json = b.to_json();
        assert_eq!(Border::from_json(&json).unwrap(), b);
        let photo = amb_engine::buffer::Image::from_fn(400, 300, |_, _| [0.5; 3]);
        let p = plan(&b, Resize::None, false, 400, 300);
        let mut out = compose(&photo, &p, [1.0; 3], [0.0; 3]);
        let before = out.clone();
        draw_caption(
            &mut out,
            &p,
            b.caption.as_ref().unwrap(),
            "FUJIFILM X-T4",
            [0.0; 3],
        );
        let mut rows = Vec::new();
        for y in 0..out.height {
            for x in 0..out.width {
                if out.get(x, y) != before.get(x, y) {
                    rows.push(y);
                    break;
                }
            }
        }
        let band = p.at.1 + p.photo.1;
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|&y| y > band), "{rows:?}");
        // About a third of the band high, around its middle.
        let (lo, hi) = (rows[0], *rows.last().unwrap());
        let h = (out.height - band) as f64;
        assert!(
            ((hi - lo) as f64 / h - 0.25).abs() < 0.12,
            "{lo}..{hi} of {h}"
        );
        assert!(((lo + hi) as f64 / 2.0 - (band as f64 + h / 2.0)).abs() < h * 0.15);
        b.caption.as_mut().unwrap().size = 200.0;
        assert!(b.validate().is_err());
    }

    #[test]
    fn composes_frame_keyline_and_corners() {
        let photo = amb_engine::buffer::Image::from_fn(20, 10, |_, _| [0.5, 0.5, 0.5]);
        let mut b = Border::simple(0.0, "#fff");
        b.unit = Unit::Px;
        b.width = 4.0;
        b.keyline = Some(Keyline {
            width: 1.0,
            color: "#000".into(),
        });
        b.radius = 40.0; // 4 px corners on a 10 px short edge
        let p = plan(&b, Resize::None, false, 20, 10);
        let out = compose(&photo, &p, [1.0; 3], [0.0; 3]);
        assert_eq!((out.width, out.height), (28, 18));
        assert_eq!(out.get(0, 0), [1.0; 3]); // frame
        assert_eq!(out.get(3, 9), [0.0; 3]); // keyline beside the photo
        assert_eq!(out.get(14, 9), [0.5; 3]); // the photo
        let corner = out.get(4, 4)[0]; // rounded away: mostly keyline
        assert!(corner < 0.25, "{corner}");
    }
}
