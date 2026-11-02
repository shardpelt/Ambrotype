// SPDX-License-Identifier: GPL-3.0-or-later
//! Full-size renders for export (the editor's Export, Convert raws, `amb
//! render`): on the GPU when there is one and the photo fits, else on the
//! CPU pipeline as before. Any GPU failure falls back to the CPU too.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use amb_engine::buffer::Image;
use amb_engine::pipeline::{self, DevelopSettings};
use amb_engine::raw::CameraImage;

use crate::Gpu;

/// A full-size render holds several copies of the photo on the GPU at once
/// (the image, the camera data, a stage's planes): at most this many (11.5 measured
/// with Clarity, Shadows, Dehaze and the raw defaults).
const COPIES: usize = 12;

/// One full-size render at a time: two would need twice the memory.
static TURN: Mutex<()> = Mutex::new(());

/// Set once a full-size render failed on this GPU: later ones go straight
/// to the CPU.
static FAILED: AtomicBool = AtomicBool::new(false);

/// Where the last full-size render was made.
static LAST: Mutex<Option<Result<String, String>>> = Mutex::new(None);

/// Where the last full-size render was made: the GPU's name, or why on the
/// CPU; `None` before the first.
pub fn last() -> Option<Result<String, String>> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// [`last`] as a phrase: "on the GPU (name)" or "on the CPU (why)".
pub fn last_where() -> Option<String> {
    last().map(|r| match r {
        Ok(name) => format!("on the GPU ({name})"),
        Err(why) => format!("on the CPU ({why})"),
    })
}

/// The GPU for programs that render without a preview (the CLI, the
/// library's batch jobs), found on first use; `None` without one. A
/// software GPU is not used (it is slower than the CPU).
pub fn shared() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| Gpu::new(false)).as_ref()
}

/// `img` developed with `develop` at full size: on `gpu` when the photo
/// fits and nothing fails, else on the CPU. Also says where: the GPU's name,
/// or why the CPU.
pub fn render(
    gpu: Option<&Gpu>,
    img: &CameraImage,
    develop: &DevelopSettings,
) -> (Image, Result<String, String>) {
    let (out, how) = match on_gpu(gpu, img, develop) {
        Ok((out, name)) => (out, Ok(name)),
        Err(why) => (pipeline::render(img, develop), Err(why)),
    };
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(how.clone());
    (out, how)
}

fn on_gpu(
    gpu: Option<&Gpu>,
    img: &CameraImage,
    develop: &DevelopSettings,
) -> Result<(Image, String), String> {
    let gpu = gpu.ok_or("no GPU")?;
    if FAILED.load(Ordering::Relaxed) {
        return Err("the GPU failed an earlier full-size render".into());
    }
    // A whole photo at once when it fits; else in tiles that do.
    let fits = (gpu.memory_budget() / (12 * COPIES)).min(gpu.max_pixels());
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let out = if img.width * img.height <= fits {
        let plan = amb_engine::plan::plan(img, develop)?;
        safely(gpu, &plan)?
    } else {
        tiled(gpu, img, develop, fits)?
    };
    Ok((out, gpu.name()))
}

/// `plan` rendered on `gpu`, or why not. wgpu reports running out of
/// memory, or a lost device, by panicking; either turns the GPU off for
/// full-size renders.
fn safely(gpu: &Gpu, plan: &amb_engine::plan::Plan) -> Result<Image, String> {
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gpu.render(plan)));
    match out {
        Ok(out) if out.rgb.iter().all(|v| v.is_finite()) => Ok(out),
        Ok(_) => {
            FAILED.store(true, Ordering::Relaxed);
            Err("the GPU's render had invalid pixels".into())
        }
        Err(_) => {
            FAILED.store(true, Ordering::Relaxed);
            Err("the GPU failed (out of memory?)".into())
        }
    }
}

/// The smallest tile side tried before giving up on the GPU.
const MIN_TILE: usize = 256;

/// `img` developed in tiles of at most `fits` pixels each (with their
/// margins), as `pipeline::render_region` does on the CPU: the whole-photo
/// measurements (haze, a negative's levels, colour ranges) come from a small
/// copy, and each tile is developed with the margin its filters reach.
fn tiled(
    gpu: &Gpu,
    img: &CameraImage,
    develop: &DevelopSettings,
    fits: usize,
) -> Result<Image, String> {
    use amb_engine::buffer::Rect;
    let (w, h) = pipeline::output_size(img, develop);
    let small = amb_engine::raw::downscale(img, pipeline::GLOBALS_LONG_SIDE);
    let globals = pipeline::measure_globals(img, &small, develop);
    let mut out = Image {
        width: w,
        height: h,
        rgb: vec![0.0; w * h * 3],
    };
    // A square window whose work area (with margins) fits; smaller if not.
    let mut side = ((fits as f64).sqrt() * 0.8) as usize;
    let mut y = 0;
    while y < h {
        let mut x = 0;
        let mut row_height = 0;
        while x < w {
            let window = Rect {
                x,
                y,
                width: side.min(w - x),
                height: side.min(h - y),
            };
            let r = amb_engine::plan::plan_region(img, develop, window, &globals)?;
            if r.plan.width * r.plan.height > fits && side > MIN_TILE {
                // The margins are wider than guessed: smaller tiles, and
                // this row again.
                side = (side * 7 / 10).max(MIN_TILE);
                x = 0;
                row_height = 0;
                continue;
            }
            let part = safely(gpu, &r.plan)?;
            let (win, inside) = (r.window, r.inside);
            for row in 0..win.height {
                let from = ((inside.y + row) * part.width + inside.x) * 3;
                let to = ((win.y + row) * w + win.x) * 3;
                out.rgb[to..to + win.width * 3]
                    .copy_from_slice(&part.rgb[from..from + win.width * 3]);
            }
            row_height = win.height;
            x += win.width;
        }
        y += row_height;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::tests::{compare, photo, settings};
    use crate::device::tests::gpu;

    #[test]
    fn a_large_photo_renders_in_tiles_like_the_cpu() {
        let Some(gpu) = gpu() else { return };
        let img = photo(900, 600);
        // Filters with reach, position-dependent effects and a mask, no
        // noise reduction (it measures the noise on each tile, as the CPU's
        // regions do).
        let n = amb_presets::Value::Number;
        let s = settings(&[
            ("ColorNoiseReduction", n(0.0)),
            ("Clarity2012", n(30.0)),
            ("Shadows2012", n(40.0)),
            ("Sharpness", n(40.0)),
            ("PostCropVignetteAmount", n(-40.0)),
            ("GrainAmount", n(30.0)),
            ("amb:Halation", n(60.0)),
        ]);
        // About 40 tiles' worth.
        let out = tiled(&gpu, &img, &s, 900 * 600 / 30).unwrap();
        let cpu = pipeline::render(&img, &s);
        assert_eq!((out.width, out.height), (cpu.width, cpu.height));
        let (max, mean) = compare(&out.rgb, &cpu.rgb);
        eprintln!("tiled: max {max:.2e}, mean {mean:.2e}");
        assert!(max < 5e-3 && mean < 2e-4, "max {max}, mean {mean}");
    }

    #[test]
    fn tiles_share_the_whole_photo_s_haze() {
        let Some(gpu) = gpu() else { return };
        let img = photo(900, 600);
        let n = amb_presets::Value::Number;
        let s = settings(&[("ColorNoiseReduction", n(0.0)), ("Dehaze", n(40.0))]);
        let out = tiled(&gpu, &img, &s, 900 * 600 / 12).unwrap();
        let cpu = pipeline::render(&img, &s);
        let (max, mean) = compare(&out.rgb, &cpu.rgb);
        eprintln!("tiled dehaze: max {max:.2e}, mean {mean:.2e}");
        // The haze is measured on a small copy, as the CPU's regions do.
        assert!(max < 3e-2 && mean < 3e-3, "max {max}, mean {mean}");
    }

    #[test]
    fn falls_back_to_the_cpu_without_a_gpu() {
        let img = photo(64, 48);
        let s = settings(&[]);
        let (out, how) = render(None, &img, &s);
        assert_eq!(how, Err("no GPU".into()));
        let cpu = pipeline::render(&img, &s);
        assert_eq!(out.rgb, cpu.rgb);
    }

    #[test]
    fn a_full_size_render_matches_the_cpu() {
        let Some(gpu) = gpu() else { return };
        // The defaults a raw gets (sharpening, colour noise reduction) and
        // local stages, on a photo larger than the other tests'.
        let img = photo(1200, 800);
        let s = settings(&[
            ("Clarity2012", amb_presets::Value::Number(30.0)),
            ("Shadows2012", amb_presets::Value::Number(40.0)),
            ("Dehaze", amb_presets::Value::Number(20.0)),
        ]);
        let before = gpu.allocated();
        let (out, how) = render(Some(&gpu), &img, &s);
        assert!(how.is_ok(), "{how:?}");
        let copies = (gpu.allocated() - before) as f64 / (img.width * img.height * 12) as f64;
        eprintln!("GPU memory: {copies:.1} copies of the photo");
        assert!(copies <= COPIES as f64, "{copies:.1} copies");
        let cpu = pipeline::render(&img, &s);
        assert_eq!((out.width, out.height), (cpu.width, cpu.height));
        let (max, mean) = compare(&out.rgb, &cpu.rgb);
        assert!(max < 2e-3 && mean < 1e-4, "max {max}, mean {mean}");
    }
}
