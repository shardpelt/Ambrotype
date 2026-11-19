// SPDX-License-Identifier: GPL-3.0-or-later
//! A selection turned into a Remove spot.

use super::geometry::{bounds, cover, distance_inside};
use super::{MIN_WIDEN, Object, Selection, WIDEN};
use amb_protocol::Command;

impl Selection {
    /// Its shadow's size, as a share of the object's (0 when none was found).
    pub fn shadow_share(&self) -> f64 {
        let own = self.object.iter().filter(|s| **s).count();
        self.shadow.iter().filter(|s| **s).count() as f64 / own.max(1) as f64
    }

    /// Take its shadow with it (or not): what Remove, Move and the masks act on.
    pub fn set_shadow(&mut self, on: bool) {
        let on = on && !self.shadow.is_empty();
        self.with_shadow = on;
        self.mask = if on {
            self.object
                .iter()
                .zip(&self.shadow)
                .map(|(a, b)| *a || *b)
                .collect()
        } else {
            self.object.clone()
        };
        (self.lo, self.hi) = bounds(&self.mask, self.width);
    }

    pub fn with_shadow(&self) -> bool {
        self.with_shadow
    }

    fn size(&self) -> f64 {
        (self.hi.0 - self.lo.0).max(self.hi.1 - self.lo.1) as f64
    }

    /// The middle of its bounding box, in fractions.
    pub fn centre(&self) -> [f64; 2] {
        [
            (self.lo.0 + self.hi.0) as f64 / 2.0 / self.width as f64,
            (self.lo.1 + self.hi.1) as f64 / 2.0 / self.height as f64,
        ]
    }

    /// One Remove spot over all of it, widened a little (shadows, soft
    /// edges and the model's blur go with it).
    pub fn remove(&self) -> Object {
        let (wf, hf) = (self.width as f64, self.height as f64);
        let r = (WIDEN * self.size()).max(MIN_WIDEN);
        let points: Vec<[f64; 2]> = cover(&self.mask, self.width, self.height, r)
            .iter()
            .map(|&(x, y)| [(x + 0.5) / wf, (y + 0.5) / hf])
            .collect();
        Object {
            command: Command::AddSpot {
                at: self.centre(),
                radius: r / wf,
                kind: Some("remove".into()),
                source: None,
                source_auto: true,
                feather: 30.0,
                opacity: 100.0,
                points: Some(points),
                separate: true,
            },
            share: self.share,
            selected: self.selected,
        }
    }

    /// Move it by `by` (fractions): a clone of it at the new place, then
    /// one Remove spot where it was (in this order, so the clone copies
    /// the object, not the fill), leaving out what the clone now covers
    /// (spots apply one after another, so it would wipe the clone).
    /// A copy of it `by` (fractions of the width and height) away, as one
    /// clone spot: Move without filling where it was.
    pub fn copy_by(&self, by: [f64; 2]) -> Vec<Command> {
        self.move_by(by).into_iter().take(1).collect()
    }

    pub fn move_by(&self, by: [f64; 2]) -> Vec<Command> {
        let (wf, hf) = (self.width as f64, self.height as f64);
        let r = (0.02 * self.size()).max(MIN_WIDEN);
        let points: Vec<[f64; 2]> = cover(&self.mask, self.width, self.height, r)
            .iter()
            .map(|&(x, y)| [(x + 0.5) / wf + by[0], (y + 0.5) / hf + by[1]])
            .collect();
        let c = self.centre();
        let (w, h) = (self.width, self.height);
        let (dx, dy) = ((by[0] * wf).round() as isize, (by[1] * hf).round() as isize);
        // Where the clone lands, in pixels.
        let moved = |x: isize, y: isize| {
            let (sx, sy) = (x - dx, y - dy);
            sx >= 0
                && sy >= 0
                && (sx as usize) < w
                && (sy as usize) < h
                && self.mask[sy as usize * w + sx as usize]
        };
        let rest: Vec<bool> = (0..w * h)
            .map(|i| self.mask[i] && !moved((i % w) as isize, (i / w) as isize))
            .collect();
        let rr = (WIDEN * self.size()).max(MIN_WIDEN);
        // The Remove spot's dabs, none reaching into the clone.
        let remove: Vec<[f64; 2]> = cover(&rest, w, h, rr)
            .into_iter()
            .filter(|&(x, y)| {
                let reach = 1.6 * rr;
                (0..12).all(|k| {
                    let a = k as f64 * std::f64::consts::TAU / 12.0;
                    !moved(
                        (x + reach * a.cos()) as isize,
                        (y + reach * a.sin()) as isize,
                    )
                }) && !moved(x as isize, y as isize)
            })
            .map(|(x, y)| [(x + 0.5) / wf, (y + 0.5) / hf])
            .collect();
        let mut out = vec![Command::AddSpot {
            at: [c[0] + by[0], c[1] + by[1]],
            radius: r / wf,
            kind: Some("clone".into()),
            source: Some(c),
            source_auto: false,
            feather: 25.0,
            opacity: 100.0,
            points: Some(points),
            separate: true,
        }];
        if !remove.is_empty() {
            out.push(Command::AddSpot {
                at: c,
                radius: rr / wf,
                kind: Some("remove".into()),
                source: None,
                source_auto: true,
                feather: 30.0,
                opacity: 100.0,
                points: Some(remove),
                separate: true,
            });
        }
        out
    }

    /// A brush mask's dabs covering it, `[x, y, radius]` in fractions (the
    /// radius of the width): each dab as big as fits inside (its distance
    /// to the edge), so a few big ones fill it and small ones follow the
    /// outline.
    pub fn mask_dabs(&self) -> Vec<[f64; 3]> {
        dabs_inside(&self.mask, self.width, self.height)
    }

    /// Its shadow's dabs alone, as `mask_dabs` (empty when none was found).
    pub fn shadow_dabs(&self) -> Vec<[f64; 3]> {
        if self.shadow.is_empty() {
            return Vec::new();
        }
        dabs_inside(&self.shadow, self.width, self.height)
    }
}

/// A brush mask's dabs covering `mask` (see `Selection::mask_dabs`).
fn dabs_inside(mask: &[bool], w: usize, h: usize) -> Vec<[f64; 3]> {
    let dist = distance_inside(mask, w, h);
    let mut order: Vec<usize> = (0..w * h).filter(|&i| mask[i]).collect();
    order.sort_by(|&a, &b| dist[b].total_cmp(&dist[a]));
    let mut covered = vec![false; w * h];
    let mut out = Vec::new();
    const SMALLEST: f32 = 2.0;
    for i in order {
        if covered[i] {
            continue;
        }
        let r = (dist[i] + 0.5).max(SMALLEST);
        let (cx, cy) = ((i % w) as f32, (i / w) as f32);
        // Marked within a little less than the dab, so neighbours overlap.
        let mark = r * 0.85;
        let m = mark.ceil() as isize;
        for dy in -m..=m {
            for dx in -m..=m {
                if (dx * dx + dy * dy) as f32 > mark * mark {
                    continue;
                }
                let (x, y) = (cx as isize + dx, cy as isize + dy);
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    covered[y as usize * w + x as usize] = true;
                }
            }
        }
        covered[i] = true;
        out.push([
            (f64::from(cx) + 0.5) / w as f64,
            (f64::from(cy) + 0.5) / h as f64,
            f64::from(r) / w as f64,
        ]);
    }
    out
}
