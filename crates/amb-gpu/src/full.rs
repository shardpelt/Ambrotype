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
    let pixels = img.width * img.height;
    if pixels > gpu.max_pixels() || pixels.saturating_mul(12 * COPIES) > gpu.memory_budget() {
        return Err(format!(
            "{} MP is too large for this GPU",
            pixels.div_ceil(1_000_000)
        ));
    }
    let plan = amb_engine::plan::plan(img, develop)?;
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    // wgpu reports running out of memory, or a lost device, by panicking.
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gpu.render(&plan)));
    match out {
        Ok(out) if out.rgb.iter().all(|v| v.is_finite()) => Ok((out, gpu.name())),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::tests::{compare, photo, settings};
    use crate::device::tests::gpu;

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
