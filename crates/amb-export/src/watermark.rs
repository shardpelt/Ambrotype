// SPDX-License-Identifier: GPL-3.0-or-later
//! Watermarks: a line of text or a logo (PNG) on the exported photo, as
//! Lightroom's export Watermark does (docs/design/collage-and-borders.md).
//!
//! Applied after resizing and output sharpening, so the mark itself isn't
//! sharpened, and before the border, so it sits on the photo, not the frame:
//!
//! ```json
//! { "text": "© Ada Lovelace", "color": "#ffffff", "size": 20, "opacity": 70,
//!   "anchor": "bottom-right", "inset": 3, "shadow": true }
//! ```
//!
//! - `text`, or `image`: a PNG whose own transparency is kept.
//! - `size`: the mark's width as a percentage of the photo's width.
//! - `anchor`: one of nine places; `inset` is the gap to the edges, a
//!   percentage of the photo's short edge.
//! - `opacity` 0–100; `shadow`: a soft dark copy behind text, so it reads on
//!   light and dark parts alike.
//!
//! Text is set in GNU FreeSans Bold (GPL-3.0-or-later with the font
//! exception), bundled.

use std::path::PathBuf;

use amb_engine::buffer::Image;
use serde::{Deserialize, Serialize};

const FONT: &[u8] = include_bytes!("../data/fonts/FreeSansBold.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
}

impl Anchor {
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    pub fn from_name(s: &str) -> Option<Anchor> {
        serde_json::from_value(serde_json::Value::String(s.trim().to_lowercase())).ok()
    }

    /// Where along each axis: 0 start, 0.5 middle, 1 end.
    fn fractions(self) -> (f64, f64) {
        let i = Anchor::ALL.iter().position(|a| *a == self).unwrap_or(8);
        ((i % 3) as f64 / 2.0, (i / 3) as f64 / 2.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Watermark {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// Text colour, an sRGB hex colour.
    #[serde(default = "white")]
    pub color: String,
    #[serde(default = "default_size")]
    pub size: f64,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default = "default_inset")]
    pub inset: f64,
    #[serde(default = "yes")]
    pub shadow: bool,
}

fn white() -> String {
    "#ffffff".into()
}
fn default_size() -> f64 {
    20.0
}
fn default_opacity() -> f64 {
    70.0
}
fn default_inset() -> f64 {
    3.0
}
fn yes() -> bool {
    true
}

impl Watermark {
    /// A text mark with the defaults.
    pub fn text(text: &str) -> Watermark {
        Watermark {
            text: text.into(),
            image: None,
            color: white(),
            size: default_size(),
            opacity: default_opacity(),
            anchor: Anchor::default(),
            inset: default_inset(),
            shadow: true,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.image.is_none() && self.text.trim().is_empty() {
            return Err("a watermark needs text or an image".into());
        }
        if self.image.is_none() && crate::border::parse_color(&self.color).is_none() {
            return Err(format!("watermark colour {:?}: use #rrggbb", self.color));
        }
        for (what, v, lo, hi) in [
            ("size", self.size, 1.0, 100.0),
            ("opacity", self.opacity, 0.0, 100.0),
            ("inset", self.inset, 0.0, 45.0),
        ] {
            if !(lo..=hi).contains(&v) {
                return Err(format!("watermark {what} must be {lo} to {hi}"));
            }
        }
        Ok(())
    }
}

/// A mark to lay over the photo: colour (output values) and coverage per pixel.
struct Layer {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
    alpha: Vec<f32>,
}

/// The text set at `px` pixels: coverage, and its size.
fn set_text(text: &str, px: f32) -> (Vec<f32>, usize, usize) {
    use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
    let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default())
        .expect("the bundled font reads");
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings::default());
    layout.append(&[&font], &TextStyle::new(text, px, 0));
    let glyphs = layout.glyphs();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for g in glyphs.iter().filter(|g| g.width > 0 && g.height > 0) {
        x0 = x0.min(g.x);
        y0 = y0.min(g.y);
        x1 = x1.max(g.x + g.width as f32);
        y1 = y1.max(g.y + g.height as f32);
    }
    if x1 <= x0 || y1 <= y0 {
        return (Vec::new(), 0, 0);
    }
    let (w, h) = ((x1 - x0).ceil() as usize + 1, (y1 - y0).ceil() as usize + 1);
    let mut cov = vec![0.0f32; w * h];
    for g in glyphs.iter().filter(|g| g.width > 0 && g.height > 0) {
        let (m, bitmap) = font.rasterize_config(g.key);
        let (ox, oy) = ((g.x - x0).round() as usize, (g.y - y0).round() as usize);
        for row in 0..m.height {
            for col in 0..m.width {
                let (x, y) = (ox + col, oy + row);
                if x < w && y < h {
                    let c = &mut cov[y * w + x];
                    *c = c.max(f32::from(bitmap[row * m.width + col]) / 255.0);
                }
            }
        }
    }
    (cov, w, h)
}

/// Box-blur `v` (w×h) by `r` pixels, twice (close to a Gaussian).
fn soften(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0; src.len()];
        let (n, lines) = if horizontal { (w, h) } else { (h, w) };
        for line in 0..lines {
            let at = |i: usize| {
                if horizontal {
                    line * w + i
                } else {
                    i * w + line
                }
            };
            for i in 0..n {
                let (a, b) = (i.saturating_sub(r), (i + r).min(n - 1));
                let sum: f32 = (a..=b).map(|j| src[at(j)]).sum();
                out[at(i)] = sum / (2 * r + 1) as f32;
            }
        }
        out
    };
    let mut v = v.to_vec();
    for _ in 0..2 {
        v = pass(&pass(&v, true), false);
    }
    v
}

fn text_layer(w: &Watermark, target_width: usize, colour: [f32; 3]) -> Option<Layer> {
    // Measure at a reference size, then set at the size that gives the width.
    let (_, ref_w, _) = set_text(&w.text, 100.0);
    if ref_w == 0 {
        return None;
    }
    let px = (100.0 * target_width as f32 / ref_w as f32).clamp(4.0, 2000.0);
    let (cov, tw, th) = set_text(&w.text, px);
    if !w.shadow {
        return Some(Layer {
            width: tw,
            height: th,
            rgb: vec![colour; tw * th],
            alpha: cov,
        });
    }
    // A blurred dark copy, a little lower and to the right, behind the text.
    let pad = (px * 0.08).ceil() as usize + 1;
    let (lw, lh) = (tw + 2 * pad, th + 2 * pad);
    let place = |dx: usize, dy: usize| {
        let mut out = vec![0.0f32; lw * lh];
        for y in 0..th {
            out[(y + dy) * lw + dx..(y + dy) * lw + dx + tw]
                .copy_from_slice(&cov[y * tw..(y + 1) * tw]);
        }
        out
    };
    let shift = (px * 0.03).round().max(1.0) as usize;
    let shadow = soften(
        &place(pad + shift, pad + shift),
        lw,
        lh,
        (px * 0.03).ceil() as usize,
    );
    let text = place(pad, pad);
    let mut rgb = Vec::with_capacity(lw * lh);
    let mut alpha = Vec::with_capacity(lw * lh);
    for (t, s) in text.iter().zip(&shadow) {
        let s = s * 0.6;
        let a = t + s * (1.0 - t);
        // Text over a black shadow.
        let k = if a > 0.0 { t / a } else { 0.0 };
        rgb.push(colour.map(|c| c * k));
        alpha.push(a);
    }
    Some(Layer {
        width: lw,
        height: lh,
        rgb,
        alpha,
    })
}

fn image_layer(
    path: &std::path::Path,
    target_width: usize,
    to_output: &dyn Fn([f64; 3]) -> [f32; 3],
) -> Result<Layer, String> {
    let logo = image::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .to_rgba32f();
    let (sw, sh) = (logo.width() as usize, logo.height() as usize);
    if sw == 0 || sh == 0 {
        return Err(format!("{}: empty image", path.display()));
    }
    let tw = target_width.max(1);
    let th = ((sh as f64 * tw as f64 / sw as f64).round() as usize).max(1);
    let px = logo.as_raw();
    // Area average (or nearest when enlarging), with colour weighted by alpha.
    let mut rgb = Vec::with_capacity(tw * th);
    let mut alpha = Vec::with_capacity(tw * th);
    for y in 0..th {
        let (ya, yb) = (
            y * sh / th,
            ((y + 1) * sh / th).max(y * sh / th + 1).min(sh),
        );
        for x in 0..tw {
            let (xa, xb) = (
                x * sw / tw,
                ((x + 1) * sw / tw).max(x * sw / tw + 1).min(sw),
            );
            let (mut c, mut a, mut n) = ([0.0f64; 3], 0.0f64, 0.0f64);
            for sy in ya..yb {
                for sx in xa..xb {
                    let p = &px[(sy * sw + sx) * 4..(sy * sw + sx) * 4 + 4];
                    let pa = f64::from(p[3]);
                    for i in 0..3 {
                        c[i] += f64::from(p[i]) * pa;
                    }
                    a += pa;
                    n += 1.0;
                }
            }
            let colour = if a > 0.0 { c.map(|v| v / a) } else { [0.0; 3] };
            rgb.push(to_output(colour));
            alpha.push((a / n) as f32);
        }
    }
    Ok(Layer {
        width: tw,
        height: th,
        rgb,
        alpha,
    })
}

/// Lay the watermark over `img` (output values). `colour` turns an sRGB hex
/// colour into output values; `to_output` an sRGB-encoded colour.
pub fn apply(
    img: &mut Image,
    w: &Watermark,
    colour: &dyn Fn(&str) -> [f32; 3],
    to_output: &dyn Fn([f64; 3]) -> [f32; 3],
) -> Result<(), String> {
    w.validate()?;
    let target = ((img.width as f64 * w.size / 100.0).round() as usize).clamp(1, img.width.max(1));
    let layer = match &w.image {
        Some(path) => image_layer(path, target, to_output)?,
        None => match text_layer(w, target, colour(&w.color)) {
            Some(l) => l,
            None => return Ok(()),
        },
    };
    let inset = (img.width.min(img.height) as f64 * w.inset / 100.0).round() as i64;
    let (fx, fy) = w.anchor.fractions();
    let x0 =
        inset + ((img.width as i64 - 2 * inset - layer.width as i64) as f64 * fx).round() as i64;
    let y0 =
        inset + ((img.height as i64 - 2 * inset - layer.height as i64) as f64 * fy).round() as i64;
    let opacity = (w.opacity / 100.0) as f32;
    for ly in 0..layer.height {
        let y = y0 + ly as i64;
        if y < 0 || y >= img.height as i64 {
            continue;
        }
        for lx in 0..layer.width {
            let x = x0 + lx as i64;
            if x < 0 || x >= img.width as i64 {
                continue;
            }
            let i = ly * layer.width + lx;
            let a = layer.alpha[i] * opacity;
            if a <= 0.0 {
                continue;
            }
            let o = (y as usize * img.width + x as usize) * 3;
            for c in 0..3 {
                let v = &mut img.rgb[o + c];
                *v += (layer.rgb[i][c] - *v) * a;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(w: usize, h: usize) -> Image {
        Image::from_fn(w, h, |_, _| [0.5, 0.5, 0.5])
    }

    fn changed(img: &Image) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for y in 0..img.height {
            for x in 0..img.width {
                if (img.get(x, y)[0] - 0.5).abs() > 0.01 {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn text_lands_in_its_corner_at_its_size() {
        let mut img = grey(400, 300);
        let mut w = Watermark::text("© Test");
        w.shadow = false;
        w.opacity = 100.0;
        apply(&mut img, &w, &|_| [1.0; 3], &|c| c.map(|v| v as f32)).unwrap();
        let px = changed(&img);
        assert!(!px.is_empty());
        let (x0, x1) = (
            px.iter().map(|p| p.0).min().unwrap(),
            px.iter().map(|p| p.0).max().unwrap(),
        );
        let (y0, y1) = (
            px.iter().map(|p| p.1).min().unwrap(),
            px.iter().map(|p| p.1).max().unwrap(),
        );
        // 20% of the width, 3% of the short edge (9 px) from the bottom right.
        assert!((x1 - x0 + 1).abs_diff(80) <= 4, "width {}", x1 - x0 + 1);
        assert!((388..=391).contains(&x1), "right {x1}");
        assert!((287..=291).contains(&y1), "bottom {y1}");
        assert!(y0 > 200, "top {y0}");
        // Top left with half opacity: lighter, but never all the way.
        let mut img = grey(400, 300);
        w.anchor = Anchor::TopLeft;
        w.opacity = 50.0;
        apply(&mut img, &w, &|_| [1.0; 3], &|c| c.map(|v| v as f32)).unwrap();
        let px = changed(&img);
        assert!(px.iter().all(|p| p.0 < 200 && p.1 < 100));
        let top = px
            .iter()
            .map(|&(x, y)| img.get(x, y)[0])
            .fold(0.0f32, f32::max);
        assert!((top - 0.75).abs() < 0.02, "{top}");
    }

    #[test]
    fn a_logo_keeps_its_transparency() {
        let dir = std::env::temp_dir().join(format!("amb-wm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logo.png");
        // Left half red and opaque, right half transparent.
        let logo = image::RgbaImage::from_fn(40, 20, |x, _| {
            if x < 20 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 255, 0, 0])
            }
        });
        logo.save(&path).unwrap();
        let mut img = grey(200, 100);
        let w = Watermark {
            image: Some(path),
            opacity: 100.0,
            anchor: Anchor::Center,
            ..Watermark::text("")
        };
        apply(&mut img, &w, &|_| [0.0; 3], &|c| c.map(|v| v as f32)).unwrap();
        // 40 px wide in the middle: red on the left, untouched on the right.
        assert_eq!(img.get(85, 50), [1.0, 0.0, 0.0]);
        assert_eq!(img.get(115, 50), [0.5, 0.5, 0.5]);
        assert_eq!(img.get(50, 50), [0.5, 0.5, 0.5]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_marks_are_refused() {
        assert!(Watermark::text("  ").validate().is_err());
        assert!(
            Watermark {
                size: 0.0,
                ..Watermark::text("x")
            }
            .validate()
            .is_err()
        );
        assert!(
            Watermark {
                color: "red".into(),
                ..Watermark::text("x")
            }
            .validate()
            .is_err()
        );
        assert_eq!(Anchor::from_name("Top-Left"), Some(Anchor::TopLeft));
        assert_eq!(Anchor::from_name("middle"), None);
    }
}
