// SPDX-License-Identifier: GPL-3.0-or-later
//! Contact sheets: many photos small on one page, for picking and printing.
//!
//! Two styles:
//! - **Plain:** a white page, a grid of photos each with its name under it.
//! - **Film:** the photos as strips of 35 mm film on the page, the way a
//!   darkroom contact print looks: dark film with sprocket holes along both
//!   edges, frame numbers and a line of edge print (the roll's name).
//!
//! Pages are filled in order; [`Sheet::per_page`] says how many fit.

use amb_engine::buffer::Image;

use crate::watermark::set_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Film,
}

impl Style {
    pub fn from_name(name: &str) -> Option<Style> {
        match name.trim().to_ascii_lowercase().as_str() {
            "plain" | "grid" => Some(Style::Plain),
            "film" | "strips" | "film-strips" => Some(Style::Film),
            _ => None,
        }
    }
}

/// A sheet's page size and look.
#[derive(Debug, Clone)]
pub struct Sheet {
    /// Page size in pixels (A4 at 300 dpi: 2480 × 3508).
    pub width: usize,
    pub height: usize,
    /// Photos across (frames per strip, for film).
    pub columns: usize,
    pub style: Style,
    /// Printed at the top, and along the film's edge.
    pub title: String,
}

impl Default for Sheet {
    fn default() -> Sheet {
        Sheet {
            width: 2480,
            height: 3508,
            columns: 6,
            style: Style::Film,
            title: String::new(),
        }
    }
}

/// Paper sizes by name, portrait, at 300 dpi.
pub fn paper(name: &str) -> Option<(usize, usize)> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "a4" => (2480, 3508),
        "a3" => (3508, 4961),
        "letter" => (2550, 3300),
        "8x10" => (2400, 3000),
        "11x14" => (3300, 4200),
        _ => return None,
    })
}

/// Where one photo goes on a page: its frame, and where its label goes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

// Proportions, as fractions of the page width (margin) or of a frame.
const MARGIN: f64 = 0.05;
const TITLE_BAND: f64 = 0.045;
/// Film: the band above and below the frames (holes and edge print), as a
/// share of the frame's height; the gap between frames; between strips.
const FILM_BAND: f64 = 0.26;
const FILM_GAP: f64 = 0.03;
const STRIP_GAP: f64 = 0.12;
/// Plain: the label line under each photo, as a share of the cell's width.
const LABEL: f64 = 0.1;

const PAPER: [f32; 3] = [1.0, 1.0, 1.0];
const FILM: [f32; 3] = [0.012, 0.010, 0.009];
const HOLE: [f32; 3] = [0.92, 0.92, 0.9];
/// Edge print: the warm orange printed on film's edge (working space).
const EDGE_PRINT: [f32; 3] = [0.85, 0.32, 0.03];
const INK: [f32; 3] = [0.08, 0.08, 0.08];
const GREY: [f32; 3] = [0.25, 0.25, 0.25];

impl Sheet {
    fn margin(&self) -> usize {
        (self.width.min(self.height) as f64 * MARGIN) as usize
    }

    fn title_band(&self) -> usize {
        if self.title.trim().is_empty() {
            0
        } else {
            (self.height as f64 * TITLE_BAND) as usize
        }
    }

    /// The slots of one page, in reading order.
    fn slots(&self) -> Vec<Slot> {
        let m = self.margin();
        let top = m + self.title_band();
        let inner_w = self.width.saturating_sub(2 * m);
        let inner_h = self.height.saturating_sub(top + m);
        let cols = self.columns.clamp(1, 20);
        let mut out = Vec::new();
        match self.style {
            Style::Film => {
                // Frames 3:2 side by side, a strip a row.
                let gap = |w: f64| (w * FILM_GAP).round();
                let fw = inner_w as f64 / (cols as f64 + cols.saturating_sub(1) as f64 * FILM_GAP);
                let fh = fw * 2.0 / 3.0;
                let strip = fh * (1.0 + 2.0 * FILM_BAND);
                let rows = ((inner_h as f64 + strip * STRIP_GAP) / (strip * (1.0 + STRIP_GAP)))
                    .floor() as usize;
                for r in 0..rows.max(1) {
                    let y = top as f64 + r as f64 * strip * (1.0 + STRIP_GAP) + fh * FILM_BAND;
                    for c in 0..cols {
                        let x = m as f64 + c as f64 * (fw + gap(fw));
                        out.push(Slot {
                            x: x as usize,
                            y: y as usize,
                            w: fw as usize,
                            h: fh as usize,
                        });
                    }
                }
            }
            Style::Plain => {
                let cw = inner_w as f64 / cols as f64;
                let ch = cw * 2.0 / 3.0 + cw * LABEL;
                let rows = (inner_h as f64 / ch).floor() as usize;
                for r in 0..rows.max(1) {
                    for c in 0..cols {
                        let pad = cw * 0.05;
                        out.push(Slot {
                            x: (m as f64 + c as f64 * cw + pad) as usize,
                            y: (top as f64 + r as f64 * ch + pad) as usize,
                            w: (cw - 2.0 * pad) as usize,
                            h: ((cw - 2.0 * pad) * 2.0 / 3.0) as usize,
                        });
                    }
                }
            }
        }
        out
    }

    /// How many photos fit on a page.
    pub fn per_page(&self) -> usize {
        self.slots().len()
    }

    /// A page with nothing on it yet but the paper, the title and (for film)
    /// the strips for its `count` photos, with their holes. `page` counts
    /// from 1.
    pub fn page(&self, page: usize, pages: usize, count: usize) -> Page {
        let bg = PAPER;
        let mut img = Image::from_fn(self.width, self.height, |_, _| bg);
        let slots = self.slots();
        if !self.title.trim().is_empty() {
            let band = self.title_band();
            let title = if pages > 1 {
                format!("{}   ·   {page} / {pages}", self.title.trim())
            } else {
                self.title.trim().to_string()
            };
            text(
                &mut img,
                &title,
                self.margin(),
                self.margin(),
                (band as f32 * 0.55).max(8.0),
                INK,
            );
        }
        if self.style == Style::Film {
            // Strips: one per row of slots.
            let cols = self.columns.clamp(1, 20);
            for row in slots[..count.min(slots.len())].chunks(cols) {
                // A strip as long as its photos, as a cut roll's last strip is.
                let (first, last) = (row[0], row[row.len() - 1]);
                let band = (first.h as f64 * FILM_BAND) as usize;
                let (x0, x1) = (
                    first.x.saturating_sub(band / 2),
                    (last.x + last.w + band / 2).min(self.width),
                );
                let (y0, y1) = (
                    first.y.saturating_sub(band),
                    (first.y + first.h + band).min(self.height),
                );
                fill(&mut img, x0, y0, x1 - x0, y1 - y0, FILM);
                // Sprocket holes next to the frames, about eight a frame;
                // the edge print runs outside them.
                let hole_w = (first.w as f64 / 13.0).max(2.0) as usize;
                let hole_h = (band as f64 * 0.38).max(2.0) as usize;
                let step = first.w as f64 / 8.0;
                let mut x = x0 as f64 + step / 2.0;
                while x + (hole_w as f64) < x1 as f64 {
                    let near = (band as f64 * 0.12) as usize;
                    for y in [first.y - near - hole_h, first.y + first.h + near] {
                        rounded(&mut img, x as usize, y, hole_w, hole_h, HOLE);
                    }
                    x += step;
                }
            }
        }
        Page {
            sheet: self.clone(),
            image: img,
            slots,
            page,
        }
    }
}

/// One page being filled.
pub struct Page {
    sheet: Sheet,
    pub image: Image,
    slots: Vec<Slot>,
    page: usize,
}

impl Page {
    /// Put photo `i` of this page (upright, developed, working space) in its
    /// slot, with its label (the file name; for film, the frame number is
    /// `number`).
    pub fn place(&mut self, i: usize, photo: &Image, label: &str, number: usize) {
        let Some(s) = self.slots.get(i).copied() else {
            return;
        };
        let film = self.sheet.style == Style::Film;
        // Film fills its frame (cropping the long side); plain shows the whole photo.
        let (pw, ph) = (photo.width as f64, photo.height.max(1) as f64);
        let scale = if film {
            (s.w as f64 / pw).max(s.h as f64 / ph)
        } else {
            (s.w as f64 / pw).min(s.h as f64 / ph)
        };
        let (w, h) = (
            ((pw * scale).round() as usize).max(1),
            ((ph * scale).round() as usize).max(1),
        );
        let small = crate::geometry::resize(photo, w, h);
        let (dw, dh) = (w.min(s.w), h.min(s.h));
        let (sx, sy) = ((w - dw) / 2, (h - dh) / 2);
        let (ox, oy) = (s.x + (s.w - dw) / 2, s.y + (s.h - dh) / 2);
        for y in 0..dh {
            for x in 0..dw {
                if ox + x < self.image.width && oy + y < self.image.height {
                    self.image.set(ox + x, oy + y, small.get(sx + x, sy + y));
                }
            }
        }
        if film {
            // On the film's edge, outside the holes: the frame number above
            // the frame, the roll's name below it.
            let band = (s.h as f64 * FILM_BAND) as usize;
            let px = (band as f32 * 0.3).max(6.0);
            let above = s.y.saturating_sub(band) + band / 14;
            let below = s.y + s.h + band - band / 14 - px as usize;
            text(
                &mut self.image,
                &number.to_string(),
                s.x + s.w / 2,
                above,
                px,
                EDGE_PRINT,
            );
            text(
                &mut self.image,
                &format!("{number}A"),
                s.x + s.w / 2 + s.w / 4,
                above,
                px * 0.8,
                EDGE_PRINT,
            );
            let edge = if self.sheet.title.trim().is_empty() {
                "AMBROTYPE"
            } else {
                self.sheet.title.trim()
            };
            text(
                &mut self.image,
                &edge.to_uppercase(),
                s.x + s.w / 8,
                below,
                px * 0.8,
                EDGE_PRINT,
            );
        } else {
            let px = (s.w as f64 * LABEL * 0.42).max(6.0) as f32;
            text(
                &mut self.image,
                label,
                s.x,
                s.y + s.h + (s.w as f64 * LABEL * 0.15) as usize,
                px,
                GREY,
            );
        }
        let _ = self.page;
    }
}

fn fill(img: &mut Image, x0: usize, y0: usize, w: usize, h: usize, c: [f32; 3]) {
    for y in y0..(y0 + h).min(img.height) {
        for x in x0..(x0 + w).min(img.width) {
            img.set(x, y, c);
        }
    }
}

/// A rectangle with rounded corners (a sprocket hole).
fn rounded(img: &mut Image, x0: usize, y0: usize, w: usize, h: usize, c: [f32; 3]) {
    let r = (w.min(h) as f32 * 0.3).max(0.5);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (cx, cy) = (fx.clamp(r, w as f32 - r), fy.clamp(r, h as f32 - r));
            let a = (r - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
            if a > 0.0 && x0 + x < img.width && y0 + y < img.height {
                let under = img.get(x0 + x, y0 + y);
                img.set(
                    x0 + x,
                    y0 + y,
                    [0, 1, 2].map(|i| under[i] + (c[i] - under[i]) * a),
                );
            }
        }
    }
}

/// `line` at (x, y) (its top left), `px` high, in `c`.
fn text(img: &mut Image, line: &str, x0: usize, y0: usize, px: f32, c: [f32; 3]) {
    if line.trim().is_empty() {
        return;
    }
    let (cov, w, h) = set_text(crate::fonts::DEFAULT, line, px);
    for y in 0..h {
        for x in 0..w {
            let a = cov[y * w + x];
            if a <= 0.0 || x0 + x >= img.width || y0 + y >= img.height {
                continue;
            }
            let under = img.get(x0 + x, y0 + y);
            img.set(
                x0 + x,
                y0 + y,
                [0, 1, 2].map(|i| under[i] + (c[i] - under[i]) * a),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_roll_of_36_fits_one_a4_page_of_film_strips() {
        let sheet = Sheet {
            title: "Roll 12".into(),
            ..Default::default()
        };
        assert!(sheet.per_page() >= 36, "{}", sheet.per_page());
        let plain = Sheet {
            style: Style::Plain,
            columns: 5,
            ..sheet.clone()
        };
        assert!(plain.per_page() >= 30, "{}", plain.per_page());
    }

    #[test]
    fn photos_land_in_their_frames_on_dark_film() {
        let sheet = Sheet {
            width: 600,
            height: 800,
            columns: 3,
            ..Default::default()
        };
        let mut page = sheet.page(1, 1, 1);
        let red = Image::from_fn(300, 200, |_, _| [1.0, 0.0, 0.0]);
        page.place(0, &red, "a.nef", 1);
        let s = sheet.slots()[0];
        let mid = page.image.get(s.x + s.w / 2, s.y + s.h / 2);
        assert!(
            (mid[0] - 1.0).abs() < 1e-4 && mid[1].abs() < 1e-4,
            "{mid:?}"
        );
        // Film between the first and second frame.
        let gap_x = s.x + s.w + 1;
        assert_eq!(page.image.get(gap_x, s.y + s.h / 2), FILM);
        // Paper outside the strip.
        assert_eq!(page.image.get(2, 2), PAPER);
    }
}
