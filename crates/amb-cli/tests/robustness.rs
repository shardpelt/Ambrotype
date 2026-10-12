// SPDX-License-Identifier: GPL-3.0-or-later
//! Every parser of files people download (presets, sidecars, lens profiles,
//! LUTs, plugin manifests, DNG opcodes, the AI's compact edits) on thousands
//! of damaged copies of a good file: they may refuse them, never panic.
//! A cheap stand-in for a fuzzer that runs with the other tests (camera
//! profiles have their own, in `amb_engine::profile`).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// xorshift64*: deterministic, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Bytes that turn up at format boundaries.
const INTERESTING: &[&[u8]] = &[
    b"<",
    b">",
    b"\"",
    b"=",
    b"{",
    b"}",
    b",",
    b"-1",
    b"1e308",
    b"NaN",
    b"0",
    b"\0",
    b"\xff\xff",
    b"\x00\x00\x00\x09",
    b"\x7f\xff\xff\xff",
];

fn mutate(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut v = seed.to_vec();
    for _ in 0..1 + rng.below(6) {
        let at = rng.below(v.len() + 1);
        match rng.below(6) {
            0 if !v.is_empty() => {
                let i = rng.below(v.len());
                v[i] ^= 1 << rng.below(8);
            }
            1 => {
                let end = (at + rng.below(32)).min(v.len());
                v.drain(at..end);
            }
            2 => {
                let piece = INTERESTING[rng.below(INTERESTING.len())];
                v.splice(at..at, piece.iter().copied());
            }
            3 => v.truncate(at),
            4 if !v.is_empty() => {
                // Repeat a chunk (nesting, long lists).
                let start = rng.below(v.len());
                let end = (start + rng.below(64)).min(v.len());
                let chunk = v[start..end].to_vec();
                v.splice(at..at, chunk);
            }
            _ => {
                let byte = rng.next() as u8;
                v.insert(at, byte);
            }
        }
    }
    v
}

/// Run `parse` on the seed and on `rounds` damaged copies of it.
/// `AMB_FUZZ=<times>[,<seed>]` runs longer, or other copies, locally.
fn survives(name: &str, seed: &[u8], rounds: usize, parse: impl Fn(&[u8])) {
    let env = std::env::var("AMB_FUZZ").unwrap_or_default();
    let mut env = env.split(',').map(|v| v.trim().parse::<u64>().ok());
    let times = env.next().flatten().unwrap_or(1).max(1) as usize;
    let start = env.next().flatten().unwrap_or(0x9e37_79b9_7f4a_7c15);
    let rounds = rounds * times;
    let mut rng = Rng(start ^ name.len() as u64);
    parse(seed);
    for i in 0..rounds {
        let input = mutate(seed, &mut rng);
        if catch_unwind(AssertUnwindSafe(|| parse(&input))).is_err() {
            let shown = String::from_utf8_lossy(&input);
            panic!(
                "{name}: round {i} panicked on {} bytes:\n{shown}",
                input.len()
            );
        }
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn fixture(path: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A preset or sidecar is parsed, then read as develop settings and masks.
fn preset(b: &[u8]) {
    if let Ok(p) = amb_presets::Preset::parse(&text(b)) {
        let _ = amb_engine::pipeline::DevelopSettings::from_settings(&p.settings);
    }
}

#[test]
fn presets_and_sidecars() {
    for f in [
        "crates/amb-presets/tests/fixtures/punchy-teal-orange.xmp",
        "crates/amb-presets/tests/fixtures/sidecar.xmp",
        "crates/amb-presets/tests/fixtures/element-form.xmp",
        "crates/amb-presets/tests/fixtures/warm-matte.lrtemplate",
    ] {
        survives(f, &fixture(f), 4000, preset);
    }
}

#[test]
fn lens_profiles() {
    let shot = amb_lens::ShotInfo {
        camera_make: "Acme".into(),
        camera_model: "FF-1".into(),
        lens_model: Some("Acme Zoomar 24-70mm f2.8 Pro A".into()),
        focal_mm: Some(35.0),
        aperture: Some(4.0),
        ..Default::default()
    };
    let lcp =
        "crates/amb-lens/tests/fixtures/lcp/ACME FF-1 (Acme Zoomar 24-70mm f2.8 Pro A) - RAW.lcp";
    survives("lcp", &fixture(lcp), 4000, |b| {
        if let Ok(f) = amb_lens::LcpFile::parse_str(&text(b), "x.lcp") {
            let _ = f.correction(&shot);
        }
    });
    let xml = "crates/amb-lens/tests/fixtures/lensfun/acme.xml";
    survives("lensfun", &fixture(xml), 4000, |b| {
        let mut db = amb_lens::LensfunDb::default();
        if db.load_str(&text(b), Path::new("x.xml")).is_ok()
            && let Some(m) = db.find_lens(&shot)
        {
            let _ = db.correction(&m, &shot);
        }
    });
}

#[test]
fn cube_luts() {
    let mut seed = String::from("TITLE \"x\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n");
    for i in 0..8 {
        let c = |b: usize| f32::from((i >> b) & 1 != 0) * 0.9 + 0.05;
        seed += &format!("{} {} {}\n", c(0), c(1), c(2));
    }
    survives("cube", seed.as_bytes(), 8000, |b| {
        if let Ok(l) = amb_engine::lut::CubeLut::parse(&text(b)) {
            let _ = l.apply([0.3, 0.5, 0.9]);
        }
    });
}

#[test]
fn compact_edits() {
    let seed = b"exp+0.3 contrast=20 wb=5600/+5 hsl.orange.sat-10 sky:exp=-0.5,sat=10 grade.shadows=200/20 crop=0.1,0.1,0.9,0.9";
    survives("compact", seed, 10000, |b| {
        let _ = amb_protocol::compact::parse(&text(b));
    });
}

#[test]
fn plugin_manifests() {
    let seed = br#"{"format":1,"id":"example","name":"Example","version":"1.0.0","permissions":["read_settings","edit"],"params":[{"id":"amount","label":"Amount","kind":"number","min":0,"max":100,"default":50}]}"#;
    survives("manifest", seed, 8000, |b| {
        let _ = amb_plugin::Manifest::from_json(&text(b));
    });
}

#[test]
fn dng_opcode_lists() {
    // A WarpRectilinear and a 2 × 2 GainMap.
    let mut seed = 2u32.to_be_bytes().to_vec();
    let mut warp = 1u32.to_be_bytes().to_vec();
    for v in [0.98, -0.01, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5] {
        warp.extend_from_slice(&f64::to_be_bytes(v));
    }
    let mut gain = Vec::new();
    for v in [0u32, 0, 4, 4, 0, 1, 2, 2, 2, 2] {
        gain.extend_from_slice(&v.to_be_bytes());
    }
    for v in [1.0f64, 1.0, 0.0, 0.0] {
        gain.extend_from_slice(&v.to_be_bytes());
    }
    gain.extend_from_slice(&1u32.to_be_bytes());
    for g in [1.0f32, 1.5, 2.0, 2.5] {
        gain.extend_from_slice(&g.to_be_bytes());
    }
    for (id, body) in [(1u32, warp), (9, gain)] {
        for v in [id, 0x0103_0000, 0, body.len() as u32] {
            seed.extend_from_slice(&v.to_be_bytes());
        }
        seed.extend_from_slice(&body);
    }
    use amb_engine::dng_opcodes::{self, Opcode, Placement};
    survives("opcodes", &seed, 10000, |b| {
        let ops = dng_opcodes::parse(b);
        let at = Placement {
            stage: (8, 6),
            origin: (1, 1),
        };
        let _ = dng_opcodes::correction(&ops, at, 6, 4);
        let maps: Vec<_> = ops
            .iter()
            .filter_map(|o| match o {
                Opcode::GainMap(m) => Some(m),
                _ => None,
            })
            .collect();
        let mut rgb = vec![0.5f32; 6 * 4 * 3];
        let rggb = |r: usize, c: usize| [[0, 1], [1, 2]][r % 2][c % 2];
        dng_opcodes::apply_gain_maps(&mut rgb, 6, 4, &maps, at, Some(&rggb));
    });
}
