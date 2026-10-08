// SPDX-License-Identifier: GPL-3.0-or-later
//! Collages: several photos on one canvas (docs/design/collage-and-borders.md).
//!
//! A *template* is the shareable part: the canvas size, background, gaps,
//! margin, rounded corners and the cells, as fractions of the canvas. It holds
//! no photos. A *collage* is a template plus, per cell, the photo in it and how
//! it sits there (pan, zoom, quarter turns).
//!
//! ```json
//! { "format": "ambrotype-collage/1", "name": "Hero and two",
//!   "width": 1080, "height": 1350, "background": "#ffffff",
//!   "gap": 1.5, "margin": 3, "radius": 0,
//!   "cells": [ [0, 0, 1, 0.6], [0, 0.6, 0.5, 0.4], [0.5, 0.6, 0.5, 0.4] ] }
//! ```
//!
//! `gap`, `margin` and `radius` are percentages of the canvas's short edge.
//! A cell is `[x, y, width, height]` within the area inside the margin; gaps
//! are taken off the edges cells share, so cells that meet stay `gap` apart.
//!
//! A *free* template (`"free": true`) is arranged by hand, like layers: cells
//! are fractions of the whole canvas (no margin or gaps), may overlap (later
//! cells on top) and stick out of the canvas, and may be turned to any angle,
//! written as a fifth number: `[x, y, width, height, degrees clockwise]`.
//!
//! Photos fill their cell ("cover"): at zoom 1 the whole cell is covered and
//! nothing is distorted; zoom > 1 crops in further, and pan moves the window
//! (kept inside the photo). Rendering works in the export's working space
//! (linear ProPhoto), one photo at a time, and the canvas then goes through
//! the normal export (formats, colour spaces, metadata).

use crate::geometry;
use amb_engine::buffer::Image;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "ambrotype-collage/1";
const MAX_CELLS: usize = 64;
const MAX_SIDE: u32 = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default)]
    pub gap: f64,
    #[serde(default)]
    pub margin: f64,
    #[serde(default)]
    pub radius: f64,
    /// Cells placed by hand: see the module notes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub free: bool,
    pub cells: Vec<Cell>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A cell: a rectangle as fractions, and (free templates only) an angle in
/// degrees, clockwise about its centre. Written `[x, y, w, h]`, or
/// `[x, y, w, h, angle]` when turned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cell {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub angle: f64,
}

impl From<[f64; 4]> for Cell {
    fn from([x, y, w, h]: [f64; 4]) -> Cell {
        Cell {
            x,
            y,
            w,
            h,
            angle: 0.0,
        }
    }
}

impl Serialize for Cell {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut v = vec![self.x, self.y, self.w, self.h];
        if self.angle != 0.0 {
            v.push(self.angle);
        }
        v.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Cell {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Cell, D::Error> {
        let v = Vec::<f64>::deserialize(d)?;
        match v[..] {
            [x, y, w, h] => Ok(Cell {
                x,
                y,
                w,
                h,
                angle: 0.0,
            }),
            [x, y, w, h, angle] => Ok(Cell { x, y, w, h, angle }),
            _ => Err(serde::de::Error::custom(
                "a cell is [x, y, width, height] or [x, y, width, height, angle]",
            )),
        }
    }
}

fn default_format() -> String {
    FORMAT.into()
}

fn default_background() -> String {
    "#ffffff".into()
}

/// How a photo sits in its cell.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// The centre of the visible window, as fractions of the (turned) photo.
    #[serde(default = "half")]
    pub cx: f64,
    #[serde(default = "half")]
    pub cy: f64,
    /// 1 = just covers the cell; larger crops in.
    #[serde(default = "one")]
    pub zoom: f64,
    /// Clockwise quarter turns, 0..=3.
    #[serde(default)]
    pub turns: u8,
}

fn half() -> f64 {
    0.5
}

fn one() -> f64 {
    1.0
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            cx: 0.5,
            cy: 0.5,
            zoom: 1.0,
            turns: 0,
        }
    }
}

/// A cell's pixel rectangle on the canvas (before turning: the angle is
/// about its centre; free cells can start left of or above the canvas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRect {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub angle: f64,
}

impl Template {
    pub fn from_json(text: &str) -> Result<Template, String> {
        let t: Template =
            serde_json::from_str(text).map_err(|e| format!("not a collage template: {e}"))?;
        t.validate()?;
        Ok(t)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("\"format\" must be \"{FORMAT}\""));
        }
        if !(16..=MAX_SIDE).contains(&self.width) || !(16..=MAX_SIDE).contains(&self.height) {
            return Err(format!("width and height must be 16 to {MAX_SIDE} px"));
        }
        if crate::border::parse_color(&self.background)
            .is_none_or(|c| c == crate::border::Fill::Auto)
        {
            return Err(format!(
                "bad background {:?} (a hex colour such as #ffffff)",
                self.background
            ));
        }
        for (name, v, max) in [
            ("gap", self.gap, 20.0),
            ("margin", self.margin, 30.0),
            ("radius", self.radius, 50.0),
        ] {
            if !(0.0..=max).contains(&v) {
                return Err(format!(
                    "{name} must be 0 to {max} (percent of the short edge)"
                ));
            }
        }
        if self.cells.is_empty() || self.cells.len() > MAX_CELLS {
            return Err(format!("a template needs 1 to {MAX_CELLS} cells"));
        }
        for (i, c) in self.cells.iter().enumerate() {
            let Cell { x, y, w, h, angle } = *c;
            let finite = [x, y, w, h, angle].iter().all(|v| v.is_finite());
            let ok = if self.free {
                // At least part of it on the canvas, and not huge or tiny.
                finite
                    && w > 0.01
                    && h > 0.01
                    && w <= 3.0
                    && h <= 3.0
                    && x < 1.0
                    && y < 1.0
                    && x + w > 0.0
                    && y + h > 0.0
                    && (-360.0..=360.0).contains(&angle)
            } else {
                finite
                    && x >= -1e-9
                    && y >= -1e-9
                    && w > 0.01
                    && h > 0.01
                    && x + w <= 1.0 + 1e-6
                    && y + h <= 1.0 + 1e-6
            };
            if !ok {
                return Err(format!(
                    "cell {} must lie within 0..1 and not be tiny{}",
                    i + 1,
                    if self.free {
                        " (a free cell: partly on the canvas, turned at most 360°)"
                    } else {
                        ""
                    }
                ));
            }
            if !self.free && angle != 0.0 {
                return Err(format!(
                    "cell {} is turned: that needs \"free\": true",
                    i + 1
                ));
            }
        }
        Ok(())
    }

    fn short(&self) -> f64 {
        f64::from(self.width.min(self.height))
    }

    /// Each cell's pixels on the canvas, gaps and margin taken off (free
    /// templates: just the fractions of the canvas).
    pub fn cell_rects(&self) -> Vec<CellRect> {
        if self.free {
            let (cw, ch) = (f64::from(self.width), f64::from(self.height));
            return self
                .cells
                .iter()
                .map(|c| {
                    let (x0, y0) = ((c.x * cw).round() as i64, (c.y * ch).round() as i64);
                    let (x1, y1) = (
                        ((c.x + c.w) * cw).round() as i64,
                        ((c.y + c.h) * ch).round() as i64,
                    );
                    CellRect {
                        x: x0,
                        y: y0,
                        w: (x1 - x0).max(1) as usize,
                        h: (y1 - y0).max(1) as usize,
                        angle: c.angle,
                    }
                })
                .collect();
        }
        let short = self.short();
        let margin = self.margin / 100.0 * short;
        let half_gap = self.gap / 100.0 * short / 2.0;
        let (iw, ih) = (
            (f64::from(self.width) - 2.0 * margin).max(1.0),
            (f64::from(self.height) - 2.0 * margin).max(1.0),
        );
        let edge = |v: f64| v <= 1e-6 || v >= 1.0 - 1e-6;
        self.cells
            .iter()
            .map(|&Cell { x, y, w, h, .. }| {
                // Inner edges lose half a gap; edges on the margin keep their place.
                let x0 = margin + x * iw + if edge(x) { 0.0 } else { half_gap };
                let y0 = margin + y * ih + if edge(y) { 0.0 } else { half_gap };
                let x1 = margin + (x + w) * iw - if edge(x + w) { 0.0 } else { half_gap };
                let y1 = margin + (y + h) * ih - if edge(y + h) { 0.0 } else { half_gap };
                let (x0, y0) = (x0.round().max(0.0) as i64, y0.round().max(0.0) as i64);
                let (x1, y1) = (x1.round() as i64, y1.round() as i64);
                CellRect {
                    x: x0,
                    y: y0,
                    w: (x1 - x0).max(1) as usize,
                    h: (y1 - y0).max(1) as usize,
                    angle: 0.0,
                }
            })
            .collect()
    }

    pub fn radius_px(&self) -> f64 {
        self.radius / 100.0 * self.short()
    }
}

/// The part of a `pw × ph` photo that a `cw × ch` cell shows: (x, y, w, h)
/// in photo pixels. Pan is kept so the window stays inside the photo.
pub fn window(pw: usize, ph: usize, cw: usize, ch: usize, p: &Placement) -> (f64, f64, f64, f64) {
    let (pw, ph) = (pw as f64, ph as f64);
    let target = cw as f64 / ch as f64;
    // The largest window of the cell's shape, then smaller by the zoom.
    let (mut w, mut h) = if pw / ph > target {
        (ph * target, ph)
    } else {
        (pw, pw / target)
    };
    let zoom = p.zoom.clamp(1.0, 20.0);
    w /= zoom;
    h /= zoom;
    let x = (p.cx * pw - w / 2.0).clamp(0.0, pw - w);
    let y = (p.cy * ph - h / 2.0).clamp(0.0, ph - h);
    (x, y, w, h)
}

/// Turn a photo by clockwise quarter turns.
pub fn turn(img: &Image, turns: u8) -> Image {
    match turns % 4 {
        1 => geometry::orient(img, 6),
        2 => geometry::orient(img, 3),
        3 => geometry::orient(img, 8),
        _ => img.clone(),
    }
}

/// What the canvas starts as: the background colour in the working space
/// (linear ProPhoto), from an sRGB hex colour.
pub fn background(t: &Template) -> [f32; 3] {
    use amb_color::space::OutputSpace;
    let Some(crate::border::Fill::Srgb(rgb)) = crate::border::parse_color(&t.background) else {
        return [1.0; 3];
    };
    let srgb = OutputSpace::Srgb;
    let lin = rgb.map(|v| srgb.transfer().decode(v));
    let to_pro = amb_color::matrix::invert3(&srgb.from_prophoto());
    [0, 1, 2]
        .map(|i| (to_pro[i][0] * lin[0] + to_pro[i][1] * lin[1] + to_pro[i][2] * lin[2]) as f32)
}

/// A collage being built: the canvas, filled cell by cell (so only one
/// developed photo needs to be in memory at a time).
pub struct Canvas {
    pub template: Template,
    pub image: Image,
    rects: Vec<CellRect>,
}

impl Canvas {
    pub fn new(template: &Template) -> Canvas {
        let bg = background(template);
        Canvas {
            image: Image::from_fn(template.width as usize, template.height as usize, |_, _| bg),
            rects: template.cell_rects(),
            template: template.clone(),
        }
    }

    pub fn rects(&self) -> &[CellRect] {
        &self.rects
    }

    /// Put an upright, developed photo (working space) into cell `i`.
    pub fn place(&mut self, i: usize, photo: &Image, p: &Placement) {
        let Some(r) = self.rects.get(i).copied() else {
            return;
        };
        let photo = turn(photo, p.turns);
        let (x, y, w, h) = window(photo.width, photo.height, r.w, r.h, p);
        // Cut a whole-pixel window just around it, resize, then trim to the cell.
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = (
            ((x + w).ceil() as usize).min(photo.width),
            ((y + h).ceil() as usize).min(photo.height),
        );
        let cut = geometry::cut(&photo, x0, y0, (x1 - x0).max(1), (y1 - y0).max(1));
        let sx = r.w as f64 / w;
        let sy = r.h as f64 / h;
        let (rw, rh) = (
            ((cut.width as f64) * sx).round().max(1.0) as usize,
            ((cut.height as f64) * sy).round().max(1.0) as usize,
        );
        let scaled = geometry::resize(&cut, rw, rh);
        let (ox, oy) = (
            ((x - x0 as f64) * sx).round() as usize,
            ((y - y0 as f64) * sy).round() as usize,
        );
        let radius = self.template.radius_px().min(r.w.min(r.h) as f64 / 2.0) as f32;
        // The cell, `scaled` from (ox, oy), turned about its centre and laid
        // over what is already there with soft edges.
        let (w, h) = (r.w as f32, r.h as f32);
        let (cx, cy) = (r.x as f32 + w / 2.0, r.y as f32 + h / 2.0);
        let (sin, cos) = (r.angle as f32).to_radians().sin_cos();
        // The turned cell's bounding box on the canvas.
        let ext_x = (w * cos.abs() + h * sin.abs()) / 2.0 + 1.0;
        let ext_y = (w * sin.abs() + h * cos.abs()) / 2.0 + 1.0;
        let (iw, ih) = (self.image.width as i64, self.image.height as i64);
        let bx0 = ((cx - ext_x).floor() as i64).clamp(0, iw);
        let bx1 = ((cx + ext_x).ceil() as i64).clamp(0, iw);
        let by0 = ((cy - ext_y).floor() as i64).clamp(0, ih);
        let by1 = ((cy + ext_y).ceil() as i64).clamp(0, ih);
        for py in by0..by1 {
            for px in bx0..bx1 {
                // Canvas pixel centre into the cell's own coordinates.
                let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let u = dx * cos + dy * sin + w / 2.0;
                let v = -dx * sin + dy * cos + h / 2.0;
                let edge = (u.min(w - u).min(v).min(h - v) + 0.5).clamp(0.0, 1.0);
                let cover = edge.min(corner_cover(u, v, w, h, radius));
                if cover <= 0.0 {
                    continue;
                }
                let photo_px = sample(&scaled, ox as f32 + u - 0.5, oy as f32 + v - 0.5);
                let at = (py as usize * self.image.width + px as usize) * 3;
                let under = &mut self.image.rgb[at..at + 3];
                for c in 0..3 {
                    under[c] = photo_px[c] * cover + under[c] * (1.0 - cover);
                }
            }
        }
    }
}

/// Bilinear sample of `img` at (x, y) in pixel units (pixel centres at
/// whole numbers), clamped at the edges.
fn sample(img: &Image, x: f32, y: f32) -> [f32; 3] {
    let (maxx, maxy) = ((img.width - 1) as f32, (img.height - 1) as f32);
    let (x, y) = (x.clamp(0.0, maxx), y.clamp(0.0, maxy));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(img.width - 1), (y0 + 1).min(img.height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (
        img.get(x0, y0),
        img.get(x1, y0),
        img.get(x0, y1),
        img.get(x1, y1),
    );
    [0, 1, 2].map(|i| {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        top + (bottom - top) * fy
    })
}

/// How much of the point (x, y) of a `w × h` cell is inside its rounded
/// corners (a pixel's worth of soft edge).
fn corner_cover(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    if r <= 0.0 {
        return 1.0;
    }
    let cx = x.clamp(r, w - r);
    let cy = y.clamp(r, h - r);
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    (r - d + 0.5).clamp(0.0, 1.0)
}

/// Pages for a batch: photos (with their aspect ratios, upright) in order,
/// `cells` per page. Within a page, portrait photos go to the tallest cells
/// and landscape ones to the widest, so fewer photos are cropped hard.
/// Returns, per page, the photo index for each cell (`None` for an empty cell
/// on the last page).
pub fn pages(template: &Template, aspects: &[f64]) -> Vec<Vec<Option<usize>>> {
    let n = template.cells.len().max(1);
    let rects = template.cell_rects();
    let cell_aspect: Vec<f64> = rects.iter().map(|r| r.w as f64 / r.h as f64).collect();
    let mut out = Vec::new();
    for (page, chunk) in aspects.chunks(n).enumerate() {
        let mut cells: Vec<usize> = (0..n).collect();
        cells.sort_by(|a, b| cell_aspect[*a].total_cmp(&cell_aspect[*b]));
        let mut photos: Vec<usize> = (0..chunk.len()).collect();
        photos.sort_by(|a, b| chunk[*a].total_cmp(&chunk[*b]));
        let mut slots = vec![None; n];
        if chunk.len() == n {
            for (c, p) in cells.iter().zip(&photos) {
                slots[*c] = Some(page * n + *p);
            }
        } else {
            // A short last page fills the cells in order.
            for (i, _) in chunk.iter().enumerate() {
                slots[i] = Some(page * n + i);
            }
        }
        out.push(slots);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_up() -> Template {
        Template {
            format: FORMAT.into(),
            name: "Two".into(),
            width: 1000,
            height: 500,
            background: "#000000".into(),
            gap: 2.0,
            margin: 4.0,
            radius: 0.0,
            cells: vec![[0.0, 0.0, 0.5, 1.0].into(), [0.5, 0.0, 0.5, 1.0].into()],
            free: false,
        }
    }

    #[test]
    fn cells_keep_gap_and_margin() {
        let r = two_up().cell_rects();
        // Margin 4% of 500 = 20 px; gap 2% = 10 px between the cells.
        assert_eq!(
            r[0],
            CellRect {
                x: 20,
                y: 20,
                w: 475,
                h: 460,
                angle: 0.0
            }
        );
        assert_eq!(r[1].x - (r[0].x + r[0].w as i64), 10);
        assert_eq!(r[1].x + r[1].w as i64, 980);
        assert!(two_up().validate().is_ok());
        let mut bad = two_up();
        bad.cells.push([0.9, 0.0, 0.5, 1.0].into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn windows_cover_and_stay_inside() {
        // A 3:2 photo in a square cell: the middle square, then panned to the edge.
        let p = Placement::default();
        assert_eq!(
            window(3000, 2000, 100, 100, &p),
            (500.0, 0.0, 2000.0, 2000.0)
        );
        let left = Placement { cx: 0.0, ..p };
        assert_eq!(window(3000, 2000, 100, 100, &left).0, 0.0);
        let zoomed = Placement { zoom: 2.0, ..p };
        assert_eq!(
            window(3000, 2000, 100, 100, &zoomed),
            (1000.0, 500.0, 1000.0, 1000.0)
        );
    }

    #[test]
    fn places_photos_with_background_between() {
        let t = two_up();
        let mut c = Canvas::new(&t);
        let red = Image::from_fn(300, 200, |_, _| [1.0, 0.0, 0.0]);
        c.place(0, &red, &Placement::default());
        let px = |x, y| c.image.get(x, y);
        assert_eq!(px(5, 5), [0.0; 3], "margin is background");
        assert!(
            (px(100, 100)[0] - 1.0).abs() < 1e-4 && px(100, 100)[1].abs() < 1e-4,
            "the photo"
        );
        assert_eq!(px(500, 100), [0.0; 3], "the gap");
        assert_eq!(px(700, 100), [0.0; 3], "an empty cell");
        // Turning swaps width and height.
        assert_eq!(turn(&red, 1).width, 200);
    }

    #[test]
    fn pages_match_shapes() {
        let mut t = two_up();
        t.cells = vec![[0.0, 0.0, 1.0, 0.3].into(), [0.0, 0.3, 1.0, 0.7].into()]; // wide, then tall-ish
        // Photo 0 is portrait, 1 landscape: the landscape one goes to the wide cell.
        let p = pages(&t, &[0.67, 1.5, 1.5]);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], vec![Some(1), Some(0)]);
        assert_eq!(p[1], vec![Some(2), None]);
    }
    #[test]
    fn free_cells_turn_overlap_and_round_trip() {
        let text = r##"{ "format": "ambrotype-collage/1", "width": 200, "height": 100, "background": "#000000",
            "free": true, "cells": [ [0, 0, 1, 1], [0.25, 0.25, 0.5, 0.5, 90] ] }"##;
        let t = Template::from_json(text).unwrap();
        assert_eq!(t.cells[1].angle, 90.0);
        // Turned cells keep their fifth number; plain ones stay four numbers.
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["cells"][0].as_array().unwrap().len(), 4);
        assert_eq!(json["cells"][1][4], 90.0);
        assert_eq!(json["free"], true);
        let r = t.cell_rects();
        assert_eq!(
            (r[1].x, r[1].y, r[1].w, r[1].h, r[1].angle),
            (50, 25, 100, 50, 90.0)
        );

        // A red photo under a green one turned a quarter: the green one is
        // 50 wide and 100 tall on the canvas, centred.
        let mut canvas = Canvas::new(&t);
        let red = Image::from_fn(40, 20, |_, _| [1.0, 0.0, 0.0]);
        let green = Image::from_fn(40, 20, |_, _| [0.0, 1.0, 0.0]);
        canvas.place(0, &red, &Placement::default());
        canvas.place(1, &green, &Placement::default());
        let is = |x: usize, y: usize, want: [f32; 3]| {
            let got = canvas.image.get(x, y);
            (0..3).all(|c| (got[c] - want[c]).abs() < 1e-3)
        };
        assert!(
            is(100, 5, [0.0, 1.0, 0.0]),
            "the turned cell reaches the top"
        );
        assert!(is(60, 50, [1.0, 0.0, 0.0]), "but is only 50 px wide");
        assert!(is(100, 50, [0.0, 1.0, 0.0]));

        // A grid template can't turn cells; a free cell must touch the canvas.
        let grid = text.replace("\"free\": true,", "");
        assert!(Template::from_json(&grid).is_err());
        let off = text.replace("[0, 0, 1, 1]", "[1.5, 0, 0.5, 0.5]");
        assert!(Template::from_json(&off).is_err());
    }
}
