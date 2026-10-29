// SPDX-License-Identifier: GPL-3.0-or-later
//! Dust and hairs on scanned film. **Ours.**
//!
//! On a scan, dust and hairs block light, so they are darker than what is
//! around them, on a negative and a slide alike (after a negative is
//! inverted they show white). They are small and sharp, and they show on
//! smooth areas such as skies, where they matter most. So:
//!
//! 1. Log luminance, signed so a defect is a peak.
//! 2. Its difference from the mean of a ring around each pixel (a square
//!    minus the middle square that a speck fits in): zero on average over
//!    grain, a speck's contrast on a speck.
//! 3. Pixels above a threshold over that difference's own spread (the
//!    grain), grown from those well above it (hysteresis), joined into
//!    parts. A part must stand out from the grain as a whole: the sum of
//!    its contrast, against the grain's spread times the square root of its
//!    size, must be high (grain makes small, weak clumps).
//! 4. A part is kept when it is small (a speck) or thin and short (a hair),
//!    but not a long straight line (a wire, a mast, a straight scratch:
//!    those are left to the hand), and when it stands out from its
//!    surroundings more than they vary themselves (snow among grass,
//!    lettering and leaves sit among others like them).
//!
//! Each defect also gets a donor: the nearby place of the same size with the
//! most similar surroundings and no defect in it, which a heal spot copies
//! from (docs/design/dust.md).

/// The constants of the search, in pixels of a 4,000-pixel long side
/// (scaled to the scan's size, down to half) or in natural-log units of
/// luminance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DustStrengths {
    /// The ring's outer and inner sides: the inner one is the widest
    /// speck; the outer one sets how far the surroundings reach.
    pub window: f64,
    pub inner: f64,
    /// A part's seed is this many noise deviations above its surroundings,
    /// and at least `seed_floor`; it grows over `grow` deviations. As a
    /// whole it must stand `significance` deviations above the grain.
    pub seed: f64,
    pub seed_floor: f64,
    pub grow: f64,
    pub significance: f64,
    /// The square the grain's spread is measured over.
    pub noise_window: f64,
    /// Longest speck or hair.
    pub longest: f64,
    /// Widest hair (a part longer than `window` must be this thin).
    pub hair_width: f64,
    /// A part longer than this whose pixels lie within `straightness` of
    /// one line is a line, not a hair.
    pub line_length: f64,
    pub straightness: f64,
    /// A part's contrast must be this many times how busy its surroundings
    /// are ([`surroundings`]), and at least `faintest` (fainter specks are
    /// hard to see, and experts leave them).
    pub stand_out: f64,
    pub faintest: f64,
    /// The most of a part's rim (the pixels just around it) that may be as
    /// dark as half its contrast: a twig's tip joins its branch, a hair
    /// lies on its own.
    pub attached: f64,
    /// At most this many defects, the strongest first.
    pub most: usize,
}

pub const DEFAULT_DUST: DustStrengths = DustStrengths {
    window: 31.0,
    inner: 11.0,
    seed: 4.0,
    seed_floor: 0.05,
    grow: 2.5,
    significance: 12.0,
    noise_window: 64.0,
    longest: 120.0,
    hair_width: 6.0,
    line_length: 40.0,
    straightness: 0.8,
    stand_out: 2.0,
    faintest: 0.1,
    attached: 0.15,
    most: 500,
};

/// One defect, in pixels of the image searched.
#[derive(Debug, Clone, PartialEq)]
pub struct Defect {
    /// Its pixels' bounding centre.
    pub centre: (f64, f64),
    /// The distance from `path` within which every pixel of it lies (for a
    /// speck, `path` is the centre alone).
    pub radius: f64,
    /// A hair's line through its pixels, end to end; a speck's centre.
    pub path: Vec<(f64, f64)>,
    /// How far the donor is: the defect copies from `centre + donor`.
    pub donor: (f64, f64),
    /// Its contrast against the surroundings (log units).
    pub contrast: f64,
    /// Its pixels, as `y * width + x`.
    pub pixels: Vec<usize>,
}

impl Defect {
    pub fn is_hair(&self) -> bool {
        self.path.len() > 1
    }
}

/// Defects on `rgb` (interleaved linear RGB, `w`×`h`). `dark`: they are
/// darker than their surroundings (a scan); false looks for lighter ones
/// (a negative already turned positive).
pub fn find(rgb: &[f32], w: usize, h: usize, dark: bool, k: &DustStrengths) -> Vec<Defect> {
    if w < 16 || h < 16 {
        return Vec::new();
    }
    // Smaller scans keep the sizes of a 2,000-pixel one: a speck is never
    // less than a pixel or two.
    let scale = (w.max(h) as f64 / 4000.0).max(0.5);
    let px = |v: f64| (v * scale).max(1.0);
    let sign = if dark { -1.0 } else { 1.0 };
    let f: Vec<f32> = rgb
        .chunks_exact(3)
        .map(|p| sign * ((p[0] + 2.0 * p[1] + p[2]) / 4.0).max(1e-4).ln())
        .collect();
    let sums = Integral::new(&f, w, h);
    let (outer, inner) = (
        (px(k.window) as usize / 2).max(3),
        (px(k.inner) as usize / 2).max(1),
    );
    // The surroundings: the mean of the ring.
    let ring: Vec<f32> = (0..w * h)
        .map(|i| sums.ring_mean(i % w, i / w, outer, inner) as f32)
        .collect();
    let top: Vec<f32> = f.iter().zip(&ring).map(|(a, b)| a - b).collect();
    // The grain's spread around each pixel: the mean size of the contrast
    // over a wide square (scaled from a mean absolute deviation to a
    // standard one). Dark and busy parts are noisier in log units than a
    // clear sky, so one figure for the frame would miss the sky's dust.
    let size: Vec<f32> = top.iter().map(|v| v.abs()).collect();
    let spread = Integral::new(&size, w, h);
    let reach = (px(k.noise_window) as usize / 2).max(4);
    let noise: Vec<f32> = (0..w * h)
        .map(|i| (1.2533 * spread.ring_mean(i % w, i / w, reach, 0)).max(1e-3) as f32)
        .collect();
    let seed = |i: usize| (k.seed as f32 * noise[i]).max(k.seed_floor as f32);
    let grow = |i: usize| (k.grow as f32 * noise[i]).max(k.seed_floor as f32 * 0.5);

    // Parts: grown from seeds over the lower threshold.
    let mut part = vec![u32::MAX; w * h];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if part[start] != u32::MAX || top[start] < seed(start) {
            continue;
        }
        let id = found.len() as u32;
        let mut pixels = vec![start];
        let mut count = 1usize;
        part[start] = id;
        stack.push(start);
        let limit = (px(k.longest) * px(k.window)) as usize;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            for (dx, dy) in [
                (-1i64, 0i64),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
            ] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if part[j] == u32::MAX && top[j] >= grow(j) {
                    part[j] = id;
                    count += 1;
                    if pixels.len() <= limit {
                        pixels.push(j);
                    }
                    stack.push(j);
                }
            }
        }
        // Too big to be dust: an edge of the picture, a bright window.
        if count <= limit {
            found.push(pixels);
        }
    }

    let mut defects: Vec<Defect> = found
        .into_iter()
        .filter(|pixels| {
            let sum: f64 = pixels.iter().map(|&i| top[i] as f64).sum();
            let grain = noise[pixels[0]] as f64;
            sum >= k.significance * grain * (pixels.len() as f64).sqrt()
        })
        .filter_map(|pixels| shape(&pixels, w, k, scale))
        .filter_map(|mut d| {
            let pixels = std::mem::take(&mut d.pixels);
            let mut c: Vec<f32> = pixels.iter().map(|&i| top[i]).collect();
            let contrast = percentile(&mut c, 0.5);
            let spread = surroundings(&top, w, h, &d, &pixels, px(k.window));
            let attached = attached(&f, w, h, &d, &pixels, contrast, px(k.window));
            let keep =
                contrast as f64 >= k.faintest.max(k.stand_out * spread) && attached <= k.attached;
            keep.then_some(Defect {
                contrast: contrast as f64,
                pixels,
                ..d
            })
        })
        .collect();
    defects.sort_by(|a, b| b.contrast.total_cmp(&a.contrast));
    defects.truncate(k.most);
    let taken: std::collections::HashSet<usize> = defects
        .iter()
        .flat_map(|d| d.pixels.iter().copied())
        .collect();
    for d in &mut defects {
        d.donor = donor(&ring, w, h, d, &taken);
    }
    defects
}

/// A part's shape, if it is a speck or a hair: its centre and either its
/// centre alone or its line, and how far its pixels are from that.
fn shape(pixels: &[usize], w: usize, k: &DustStrengths, scale: f64) -> Option<Defect> {
    let px = |v: f64| (v * scale).max(1.0);
    let n = pixels.len() as f64;
    let pts: Vec<(f64, f64)> = pixels
        .iter()
        .map(|&i| ((i % w) as f64, (i / w) as f64))
        .collect();
    let (mx, my) = pts
        .iter()
        .fold((0.0, 0.0), |(a, b), p| (a + p.0 / n, b + p.1 / n));
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in &pts {
        sxx += (p.0 - mx).powi(2) / n;
        syy += (p.1 - my).powi(2) / n;
        sxy += (p.0 - mx) * (p.1 - my) / n;
    }
    // The main axis, and where each pixel lies along and across it.
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (ca, sa) = (angle.cos(), angle.sin());
    let along: Vec<f64> = pts
        .iter()
        .map(|p| (p.0 - mx) * ca + (p.1 - my) * sa)
        .collect();
    let across: Vec<f64> = pts
        .iter()
        .map(|p| -(p.0 - mx) * sa + (p.1 - my) * ca)
        .collect();
    let (lo, hi) = along
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let length = hi - lo + 1.0;
    if length > px(k.longest) {
        return None;
    }
    let width = n / length;
    let spread = (across.iter().map(|v| v * v).sum::<f64>() / n).sqrt();
    let speck = length <= px(k.window);
    if !speck && width > px(k.hair_width) {
        return None;
    }
    if length > px(k.line_length) && spread <= k.straightness * scale.max(1.0) {
        return None;
    }
    let path: Vec<(f64, f64)> = if speck {
        vec![(mx, my)]
    } else {
        // The middle of the pixels in each stretch along the axis.
        let step = (px(k.hair_width) * 1.5).max(2.0);
        let bins = ((length / step).ceil() as usize).max(2);
        let mut sums = vec![(0.0, 0.0, 0.0); bins];
        for (p, &a) in pts.iter().zip(&along) {
            let b = (((a - lo) / length) * bins as f64)
                .floor()
                .min(bins as f64 - 1.0) as usize;
            sums[b].0 += p.0;
            sums[b].1 += p.1;
            sums[b].2 += 1.0;
        }
        sums.into_iter()
            .filter(|s| s.2 > 0.0)
            .map(|s| (s.0 / s.2, s.1 / s.2))
            .collect()
    };
    let radius = pts
        .iter()
        .map(|p| distance_to_path(*p, &path))
        .fold(0.0, f64::max)
        + 1.0;
    Some(Defect {
        centre: (mx, my),
        radius,
        path,
        donor: (0.0, 0.0),
        contrast: 0.0,
        pixels: pixels.to_vec(),
    })
}

fn distance_to_path(p: (f64, f64), path: &[(f64, f64)]) -> f64 {
    if path.len() == 1 {
        return (p.0 - path[0].0).hypot(p.1 - path[0].1);
    }
    path.windows(2)
        .map(|s| {
            let (a, b) = (s[0], s[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l2 = (dx * dx + dy * dy).max(1e-9);
            let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0);
            (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
        })
        .fold(f64::MAX, f64::min)
}

/// How busy a defect's surroundings are: the 90th percentile of the
/// contrast's size in a frame around its box, leaving out its own pixels.
/// Grain alone gives about 1.6 deviations; leaves, grass and lettering
/// much more.
fn surroundings(top: &[f32], w: usize, h: usize, d: &Defect, own: &[usize], margin: f64) -> f64 {
    let (x0, y0, x1, y1) = bounds(d, margin, w, h);
    let own: std::collections::HashSet<usize> = own.iter().copied().collect();
    let mut v: Vec<f32> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| y * w + x))
        .filter(|i| !own.contains(i))
        .map(|i| top[i].abs())
        .collect();
    if v.len() < 16 {
        return f64::MAX;
    }
    percentile(&mut v, 0.9) as f64
}

/// The share of a part's rim (within two pixels of it) that is as far from
/// the background as half the part's contrast: what it is attached to.
fn attached(
    f: &[f32],
    w: usize,
    h: usize,
    d: &Defect,
    own: &[usize],
    contrast: f32,
    margin: f64,
) -> f64 {
    let mine: std::collections::HashSet<usize> = own.iter().copied().collect();
    let (x0, y0, x1, y1) = bounds(d, margin, w, h);
    let mut frame: Vec<f32> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| y * w + x))
        .filter(|i| !mine.contains(i))
        .map(|i| f[i])
        .collect();
    if frame.is_empty() {
        return 0.0;
    }
    let background = percentile(&mut frame, 0.5);
    let mut rim = std::collections::HashSet::new();
    for &i in own {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 {
                    let j = ny as usize * w + nx as usize;
                    if !mine.contains(&j) {
                        rim.insert(j);
                    }
                }
            }
        }
    }
    let dark = rim
        .iter()
        .filter(|&&j| f[j] - background > 0.5 * contrast)
        .count();
    dark as f64 / rim.len().max(1) as f64
}

fn bounds(d: &Defect, margin: f64, w: usize, h: usize) -> (usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &d.path {
        x0 = x0.min(p.0 - d.radius - margin);
        y0 = y0.min(p.1 - d.radius - margin);
        x1 = x1.max(p.0 + d.radius + margin);
        y1 = y1.max(p.1 + d.radius + margin);
    }
    (
        x0.max(0.0) as usize,
        y0.max(0.0) as usize,
        (x1.ceil().max(0.0) as usize).min(w),
        (y1.ceil().max(0.0) as usize).min(h),
    )
}

/// The donor offset: of eight directions (across a hair first), the one
/// whose footprint holds no defect and whose background (the ring means)
/// is most like the defect's own.
fn donor(
    ring: &[f32],
    w: usize,
    h: usize,
    d: &Defect,
    taken: &std::collections::HashSet<usize>,
) -> (f64, f64) {
    let reach = 2.5 * d.radius + 2.0;
    let sample = |dx: f64, dy: f64| -> Option<f32> {
        let mut sum = 0.0f32;
        let mut n = 0.0f32;
        for &i in &d.pixels {
            let x = (i % w) as f64 + dx;
            let y = (i / w) as f64 + dy;
            if x < 0.0 || y < 0.0 || x >= w as f64 || y >= h as f64 {
                return None;
            }
            let j = y as usize * w + x as usize;
            if taken.contains(&j) {
                return None;
            }
            sum += ring[j];
            n += 1.0;
        }
        Some(sum / n.max(1.0))
    };
    let own = sample(0.0, 0.0).unwrap_or(0.0);
    let base = if d.is_hair() {
        let (a, b) = (d.path[0], d.path[d.path.len() - 1]);
        (b.1 - a.1).atan2(b.0 - a.0) + std::f64::consts::FRAC_PI_2
    } else {
        0.0
    };
    let mut best = (f32::MAX, (reach, 0.0));
    for step in 0..8 {
        let a = base + step as f64 * std::f64::consts::FRAC_PI_4;
        for far in [1.0, 1.6] {
            let (dx, dy) = (
                (reach * far * a.cos()).round(),
                (reach * far * a.sin()).round(),
            );
            if let Some(v) = sample(dx, dy) {
                // Across a hair is preferred a little; further a little less.
                let cost = (v - own).abs()
                    + if step % 4 == 0 { 0.0 } else { 0.01 }
                    + (far as f32 - 1.0) * 0.02;
                if cost < best.0 {
                    best = (cost, (dx, dy));
                }
            }
        }
    }
    best.1
}

/// Sums over rectangles, for box means.
struct Integral {
    w: usize,
    h: usize,
    s: Vec<f64>,
}

impl Integral {
    fn new(f: &[f32], w: usize, h: usize) -> Integral {
        let mut s = vec![0.0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0.0;
            for x in 0..w {
                row += f[y * w + x] as f64;
                s[(y + 1) * (w + 1) + x + 1] = s[y * (w + 1) + x + 1] + row;
            }
        }
        Integral { w, h, s }
    }

    /// The sum and count over the square of half-side `r` around (x, y),
    /// cut at the edges.
    fn square(&self, x: usize, y: usize, r: usize) -> (f64, f64) {
        let (x0, y0) = (x.saturating_sub(r), y.saturating_sub(r));
        let (x1, y1) = ((x + r + 1).min(self.w), (y + r + 1).min(self.h));
        let at = |x: usize, y: usize| self.s[y * (self.w + 1) + x];
        (
            at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0),
            ((x1 - x0) * (y1 - y0)) as f64,
        )
    }

    fn ring_mean(&self, x: usize, y: usize, outer: usize, inner: usize) -> f64 {
        let (so, no) = self.square(x, y, outer);
        let (si, ni) = self.square(x, y, inner);
        (so - si) / (no - ni).max(1.0)
    }
}

fn percentile(v: &mut [f32], q: f64) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let i = ((v.len() - 1) as f64 * q).round() as usize;
    *v.select_nth_unstable_by(i, f32::total_cmp).1
}

/// A straight scratch along a line drawn from `a` to `b` (pixels), as
/// pieces of at most 160 pixels (at 4,000 on the long side), each moved
/// onto the scratch on its own, so a scratch that drifts a little is
/// followed, and each healed from its own side. **Ours.** Where two pieces
/// meet, their ends are joined halfway.
pub fn line(rgb: &[f32], w: usize, h: usize, a: (f64, f64), b: (f64, f64)) -> Vec<Defect> {
    let f: Vec<f32> = rgb
        .chunks_exact(3)
        .map(|p| ((p[0] + 2.0 * p[1] + p[2]) / 4.0).max(1e-4).ln())
        .collect();
    let scale = (w.max(h) as f64 / 4000.0).max(0.5);
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    let pieces = ((length / (160.0 * scale)).ceil() as usize).max(1);
    let mut out: Vec<Defect> = (0..pieces)
        .map(|i| {
            let t0 = i as f64 / pieces as f64;
            let t1 = (i + 1) as f64 / pieces as f64;
            let p = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            piece(&f, w, h, p(t0), p(t1))
        })
        .collect();
    for i in 1..out.len() {
        let (end, start) = (out[i - 1].path[1], out[i].path[0]);
        let mid = ((end.0 + start.0) / 2.0, (end.1 + start.1) / 2.0);
        out[i - 1].path[1] = mid;
        out[i].path[0] = mid;
    }
    for d in &mut out {
        d.centre = (
            (d.path[0].0 + d.path[1].0) / 2.0,
            (d.path[0].1 + d.path[1].1) / 2.0,
        );
    }
    out
}

/// One piece of a scratch from `a` to `b`: the line moved onto it (each end
/// up to `reach` pixels across), how wide it is, and a donor beside it.
/// Each end is moved to where the line differs most from the bands on both
/// sides of it, dark or light (a scratch through the emulsion shows light
/// on a negative's scan, one in the base dark). Where nothing stands out,
/// the line stays as drawn, a few pixels wide.
fn piece(f: &[f32], w: usize, h: usize, a: (f64, f64), b: (f64, f64)) -> Defect {
    let at = |x: f64, y: f64| -> Option<f32> {
        let (xi, yi) = (x.round(), y.round());
        (xi >= 0.0 && yi >= 0.0 && xi < w as f64 && yi < h as f64)
            .then(|| f[yi as usize * w + xi as usize])
    };
    let scale = (w.max(h) as f64 / 4000.0).max(0.5);
    let reach = (12.0 * scale).round().max(8.0) as i64;
    let length = (b.0 - a.0).hypot(b.1 - a.1).max(1.0);
    let (ux, uy) = ((b.0 - a.0) / length, (b.1 - a.1) / length);
    let (nx, ny) = (-uy, ux);
    let steps = length.ceil() as usize;
    // The mean along the line from a+oa·n to b+ob·n, offset across by `by`.
    let mean = |oa: f64, ob: f64, by: f64| -> f64 {
        let (mut sum, mut n) = (0.0, 0.0);
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let o = oa + (ob - oa) * t + by;
            if let Some(v) = at(
                a.0 + (b.0 - a.0) * t + o * nx,
                a.1 + (b.1 - a.1) * t + o * ny,
            ) {
                sum += v as f64;
                n += 1.0;
            }
        }
        if n > 0.0 { sum / n } else { 0.0 }
    };
    let side = 4.0 * scale.max(1.0);
    // A ridge: darker (or lighter) than both bands, by the smaller step;
    // a picture's edge is a step to one side only.
    let contrast = |oa: f64, ob: f64| {
        let on = mean(oa, ob, 0.0);
        let (c1, c2) = (on - mean(oa, ob, side), on - mean(oa, ob, -side));
        if c1 * c2 > 0.0 {
            c1.signum() * c1.abs().min(c2.abs())
        } else {
            0.0
        }
    };
    // In half pixels. A scratch wider than a pixel gives a plateau of
    // nearly as good lines; its middle is the line.
    let mut tried = Vec::new();
    for oa in -2 * reach..=2 * reach {
        for ob in -2 * reach..=2 * reach {
            let (oa, ob) = (oa as f64 / 2.0, ob as f64 / 2.0);
            tried.push((contrast(oa, ob), oa, ob));
        }
    }
    let top = tried.iter().copied().fold((0.0f64, 0.0, 0.0), |b, t| {
        if t.0.abs() > b.0.abs() { t } else { b }
    });
    let near: Vec<_> = tried
        .iter()
        .filter(|t| t.0 * top.0 > 0.0 && t.0.abs() >= 0.9 * top.0.abs())
        .collect();
    let n = near.len().max(1) as f64;
    let (c, oa, ob) = (
        top.0,
        near.iter().map(|t| t.1).sum::<f64>() / n,
        near.iter().map(|t| t.2).sum::<f64>() / n,
    );
    let snapped = c.abs() >= 0.04;
    let (oa, ob) = if snapped { (oa, ob) } else { (0.0, 0.0) };
    // Its width: the offsets across where the line still differs by half.
    let half_width = if snapped {
        let base = (mean(oa, ob, side * 2.0) + mean(oa, ob, -side * 2.0)) / 2.0;
        let mut r = 0.0;
        for k in 1..=(reach as usize * 2) {
            let o = k as f64 * 0.5;
            let inside = |by: f64| ((mean(oa, ob, by) - base) / c).max(0.0);
            if inside(o).max(inside(-o)) >= 0.5 {
                r = o;
            } else {
                break;
            }
        }
        // No wider than a hair: past that the bands on either side are
        // more likely a second scratch or the picture.
        r.min(3.0 * scale.max(1.0))
    } else {
        1.5 * scale.max(1.0)
    };
    let pa = (a.0 + oa * nx, a.1 + oa * ny);
    let pb = (b.0 + ob * nx, b.1 + ob * ny);
    let radius = half_width + 1.0;
    // The donor: across, on the side whose band is evener.
    let gap = 2.5 * radius + 2.0;
    let evenness = |by: f64| {
        let v: Vec<f64> = (0..=steps)
            .filter_map(|k| {
                let t = k as f64 / steps as f64;
                let o = oa + (ob - oa) * t + by;
                at(
                    a.0 + (b.0 - a.0) * t + o * nx,
                    a.1 + (b.1 - a.1) * t + o * ny,
                )
                .map(f64::from)
            })
            .collect();
        if v.len() < 2 {
            return f64::MAX;
        }
        let m = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64
    };
    let by = if evenness(gap) <= evenness(-gap) {
        gap
    } else {
        -gap
    };
    Defect {
        centre: ((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0),
        radius,
        path: vec![pa, pb],
        donor: (by * nx, by * ny),
        contrast: c.abs(),
        pixels: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grainy sky with a soft gradient, made repeatably.
    fn sky(w: usize, h: usize) -> Vec<f32> {
        let mut seed = 12345u32;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 - 0.5
        };
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let base = 0.3 + 0.1 * x as f32 / w as f32;
                let g = 1.0 + 0.06 * rand();
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[base * g; 3]);
            }
        }
        rgb
    }

    fn darken(rgb: &mut [f32], w: usize, pts: impl IntoIterator<Item = (usize, usize)>) {
        for (x, y) in pts {
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] *= 0.25;
            }
        }
    }

    fn near(d: &[Defect], x: f64, y: f64, r: f64) -> bool {
        d.iter().any(|d| {
            d.path
                .iter()
                .any(|p| (p.0 - x).hypot(p.1 - y) <= r + d.radius)
        })
    }

    #[test]
    fn finds_specks_and_hairs_but_not_lines_or_grain() {
        let (w, h) = (800, 600);
        let mut rgb = sky(w, h);
        // Specks of 1 to 5 pixels across.
        let specks = [(100, 100, 1), (300, 120, 2), (500, 400, 3), (650, 200, 5)];
        for &(cx, cy, r) in &specks {
            darken(
                &mut rgb,
                w,
                (cx - r..=cx + r)
                    .flat_map(|x| (cy - r..=cy + r).map(move |y| (x, y)))
                    .filter(|&(x, y)| {
                        (x as f64 - cx as f64).hypot(y as f64 - cy as f64) <= r as f64
                    }),
            );
        }
        // A curved hair, 2 pixels wide and about 30 long.
        let hair: Vec<(usize, usize)> = (0..30)
            .flat_map(|t| {
                let x = 200 + t;
                let y = 400 + ((t as f64 / 6.0).sin() * 6.0) as usize;
                [(x, y), (x, y + 1)]
            })
            .collect();
        darken(&mut rgb, w, hair);
        // A straight wire right across, which must stay.
        darken(&mut rgb, w, (0..w).map(|x| (x, 520)));
        let found = find(&rgb, w, h, true, &DEFAULT_DUST);
        for &(cx, cy, _) in &specks {
            assert!(
                near(&found, cx as f64, cy as f64, 2.0),
                "speck at {cx},{cy}: {found:?}"
            );
        }
        assert!(
            found
                .iter()
                .any(|d| d.is_hair() && near(std::slice::from_ref(d), 215.0, 403.0, 4.0)),
            "the hair"
        );
        assert!(
            !found
                .iter()
                .any(|d| d.path.iter().any(|p| (p.1 - 520.0).abs() < 3.0)),
            "the wire stays"
        );
        assert!(
            found.len() <= specks.len() + 2,
            "only the defects: {}",
            found.len()
        );
        // Every defect's pixels are covered by its radius around its path,
        // and its donor is clear of it.
        for d in &found {
            for &i in &d.pixels {
                let p = ((i % w) as f64, (i / w) as f64);
                assert!(distance_to_path(p, &d.path) <= d.radius);
            }
            assert!(d.donor.0.hypot(d.donor.1) > d.radius, "{d:?}");
        }
    }

    #[test]
    fn a_clean_sky_has_none() {
        let (w, h) = (600, 400);
        assert!(find(&sky(w, h), w, h, true, &DEFAULT_DUST).is_empty());
    }

    #[test]
    fn a_drawn_line_moves_onto_the_scratch() {
        let (w, h) = (600, 400);
        let mut rgb = sky(w, h);
        // A light scratch, 2 pixels wide, a little slanted.
        for x in 50..550 {
            let y = 200 + (x - 50) / 50;
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] *= 3.0;
                rgb[((y + 1) * w + x) * 3 + c] *= 3.0;
            }
        }
        // Drawn a few pixels off.
        let pieces = line(&rgb, w, h, (60.0, 195.0), (540.0, 213.0));
        assert_eq!(pieces.len(), 7, "pieces of 80 pixels at this size");
        for p in pieces.windows(2) {
            assert_eq!(p[0].path[1], p[1].path[0], "joined");
        }
        let (pa, pb) = (pieces[0].path[0], pieces[6].path[1]);
        let d = &pieces[0];
        assert!(
            (pa.1 - 200.5).abs() <= 1.0 && (pb.1 - 209.5).abs() <= 1.0,
            "{d:?}"
        );
        assert!(d.radius >= 1.5 && d.radius <= 4.0, "{d:?}");
        assert!(d.donor.1.abs() > d.radius, "{d:?}");
        // Over nothing, the line stays as drawn.
        let d = line(&sky(w, h), w, h, (60.0, 100.0), (540.0, 100.0));
        assert_eq!(
            (d[0].path[0], d[d.len() - 1].path[1]),
            ((60.0, 100.0), (540.0, 100.0))
        );
    }
}
