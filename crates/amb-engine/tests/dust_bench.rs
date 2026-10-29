// SPDX-License-Identifier: GPL-3.0-or-later
//! The dust bench: how many of the defects an expert repaired on real
//! damaged scans the search finds, and how many of its finds are real.
//!
//! It needs the FILM-R dataset (Ivanova, Williamson and Henderson,
//! "Simulating analogue film damage to analyse and improve artefact
//! restoration on high-resolution scans", Eurographics 2023; CC BY 4.0),
//! which is never committed: `tools/film-r.sh <DIR>` downloads it. Then
//!
//!   AMB_FILMR=<DIR> cargo test --release -p amb-engine --test dust_bench -- --ignored --nocapture
//!
//! The expert's repairs are where the damaged and restored scans differ.
//! The scans are positives (JPEG), so on negative film the dust is light
//! and on slides dark.

use amb_engine::dust::{self, DEFAULT_DUST};

const SLIDES: &[&str] = &["provia", "velvia", "ektachrome"];

fn linear(path: &std::path::Path) -> (Vec<f32>, Vec<u8>, usize, usize) {
    let img = image::open(path).expect("open").to_rgb8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let raw = img.into_raw();
    let lin = raw.iter().map(|&v| (v as f32 / 255.0).powf(2.2)).collect();
    (lin, raw, w, h)
}

/// 8-connected parts of a mask, as pixel lists.
fn parts(mask: &[bool], w: usize, h: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    for s in 0..mask.len() {
        if !mask[s] || seen[s] {
            continue;
        }
        let mut px = vec![s];
        let mut stack = vec![s];
        seen[s] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        px.push(j);
                        stack.push(j);
                    }
                }
            }
        }
        out.push(px);
    }
    out
}

/// The constants, with any of them changed by `AMB_DUST`
/// (`seed=4,stand_out=0`) for trying values.
fn strengths() -> dust::DustStrengths {
    let mut k = DEFAULT_DUST;
    for pair in std::env::var("AMB_DUST").unwrap_or_default().split(',') {
        let Some((name, v)) = pair.split_once('=') else {
            continue;
        };
        let v: f64 = v.parse().expect("a number");
        match name {
            "window" => k.window = v,
            "inner" => k.inner = v,
            "seed" => k.seed = v,
            "seed_floor" => k.seed_floor = v,
            "grow" => k.grow = v,
            "significance" => k.significance = v,
            "noise_window" => k.noise_window = v,
            "longest" => k.longest = v,
            "hair_width" => k.hair_width = v,
            "line_length" => k.line_length = v,
            "straightness" => k.straightness = v,
            "stand_out" => k.stand_out = v,
            "attached" => k.attached = v,
            "faintest" => k.faintest = v,
            "most" => k.most = v as usize,
            _ => panic!("no constant {name}"),
        }
    }
    k
}

#[test]
#[ignore]
fn film_r() {
    let Ok(dir) = std::env::var("AMB_FILMR") else {
        eprintln!("set AMB_FILMR to the FILM-R folder");
        return;
    };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("AMB_FILMR")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".jpg") && !n.ends_with("_restored.jpg"))
        .collect();
    names.sort();
    let (mut found_all, mut real_all, mut gt_all, mut det_all) = (0, 0, 0, 0);
    let (mut smooth_all, mut found_smooth_all) = (0, 0);
    for name in names {
        let stem = name.trim_end_matches(".jpg");
        let restored = std::path::Path::new(&dir).join(format!("{stem}_restored.jpg"));
        if !restored.exists() {
            continue;
        }
        let (lin, damaged, w, h) = linear(&std::path::Path::new(&dir).join(&name));
        let (_, fixed, _, _) = linear(&restored);
        let changed: Vec<bool> = damaged
            .chunks_exact(3)
            .zip(fixed.chunks_exact(3))
            .map(|(a, b)| (0..3).any(|c| (a[c] as i32 - b[c] as i32).abs() > 20))
            .collect();
        let truth: Vec<Vec<usize>> = parts(&changed, w, h)
            .into_iter()
            .filter(|p| p.len() >= 3)
            .collect();
        let dark = SLIDES.iter().any(|s| stem.starts_with(s));
        let t = std::time::Instant::now();
        let defects = dust::find(&lin, w, h, dark, &strengths());
        let took = t.elapsed();
        // A defect is real when the expert changed a pixel within its reach;
        // a repair is found when a defect reaches one of its pixels.
        let mut reached = vec![false; w * h];
        let mut real = 0;
        for d in &defects {
            let mut hit = false;
            let r = d.radius + 2.0;
            for p in &d.path {
                let (x0, x1) = ((p.0 - r).max(0.0) as usize, ((p.0 + r) as usize).min(w - 1));
                let (y0, y1) = ((p.1 - r).max(0.0) as usize, ((p.1 + r) as usize).min(h - 1));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if (x as f64 - p.0).hypot(y as f64 - p.1) <= r {
                            reached[y * w + x] = true;
                            hit |= changed[y * w + x];
                        }
                    }
                }
            }
            real += hit as usize;
            if let Ok(out) = std::env::var("AMB_FILMR_OUT") {
                use std::io::Write;
                let path = std::path::Path::new(&out).join(format!("{stem}.csv"));
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .expect("csv");
                let _ = writeln!(
                    f,
                    "{:.1},{:.1},{:.1},{},{:.3},{}",
                    d.centre.0,
                    d.centre.1,
                    d.radius,
                    d.path.len(),
                    d.contrast,
                    hit as u8
                );
            }
        }
        let found = truth
            .iter()
            .filter(|p| p.iter().any(|&i| reached[i]))
            .count();
        // Repairs on smooth ground (sky, walls), where dust shows most: the
        // restored scan's luminance varies little around them.
        let smooth: Vec<&Vec<usize>> = truth
            .iter()
            .filter(|p| {
                let (cx, cy) = (p[0] % w, p[0] / w);
                let mut v = Vec::new();
                for y in cy.saturating_sub(15)..(cy + 16).min(h) {
                    for x in cx.saturating_sub(15)..(cx + 16).min(w) {
                        let i = (y * w + x) * 3;
                        v.push(
                            (fixed[i] as f64 + 2.0 * fixed[i + 1] as f64 + fixed[i + 2] as f64)
                                / 4.0,
                        );
                    }
                }
                let m = v.iter().sum::<f64>() / v.len() as f64;
                (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt() < 8.0
            })
            .collect();
        let found_smooth = smooth
            .iter()
            .filter(|p| p.iter().any(|&i| reached[i]))
            .count();
        eprintln!(
            "{stem:22} {}: repairs {:4}, found {:4} ({:3.0} %), on smooth ground {:3.0} % of {:3}; defects {:4}, real {:4} ({:3.0} %); {:.1} s",
            if dark { "slide" } else { "neg  " },
            truth.len(),
            found,
            100.0 * found as f64 / truth.len().max(1) as f64,
            100.0 * found_smooth as f64 / smooth.len().max(1) as f64,
            smooth.len(),
            defects.len(),
            real,
            100.0 * real as f64 / defects.len().max(1) as f64,
            took.as_secs_f64()
        );
        found_all += found;
        smooth_all += smooth.len();
        found_smooth_all += found_smooth;
        real_all += real;
        gt_all += truth.len();
        det_all += defects.len();
    }
    eprintln!(
        "all: found {found_all} of {gt_all} repairs ({:.0} %), {found_smooth_all} of {smooth_all} on smooth ground ({:.0} %); {real_all} of {det_all} defects real ({:.0} %)",
        100.0 * found_all as f64 / gt_all.max(1) as f64,
        100.0 * found_smooth_all as f64 / smooth_all.max(1) as f64,
        100.0 * real_all as f64 / det_all.max(1) as f64
    );
}
