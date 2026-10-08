//! Export geometry: orientation, resizing and output sharpening, and the
//! conversion between a crop as stored (sensor orientation) and as seen
//! upright on screen.

use amb_engine::buffer::Image;
use amb_engine::crop::{ANGLE_SIGN, Crop, MAX_ANGLE};
use rayon::prelude::*;

/// Size of a `w × h` stored image once shown upright with EXIF `orientation`.
pub fn oriented_size(w: usize, h: usize, orientation: u16) -> (usize, usize) {
    if (5..=8).contains(&orientation) {
        (h, w)
    } else {
        (w, h)
    }
}

/// Stored-pixel position of upright pixel (x, y) for a `w × h` stored image.
pub fn stored_position(orientation: u16, w: usize, h: usize, x: usize, y: usize) -> (usize, usize) {
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (y, h - 1 - x),
        7 => (w - 1 - y, h - 1 - x),
        8 => (w - 1 - y, x),
        _ => (x, y),
    }
}

/// Upright-pixel position of stored pixel (x, y) for a `w × h` stored image:
/// the inverse of [`stored_position`].
pub fn upright_position(
    orientation: u16,
    w: usize,
    h: usize,
    x: usize,
    y: usize,
) -> (usize, usize) {
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (h - 1 - y, x),
        7 => (h - 1 - y, w - 1 - x),
        8 => (y, w - 1 - x),
        _ => (x, y),
    }
}

/// [`stored_position`] for continuous positions given as fractions (0..1) of
/// the image: upright (u, v) → stored (x, y).
pub fn stored_fraction(orientation: u16, u: f64, v: f64) -> (f64, f64) {
    match orientation {
        2 => (1.0 - u, v),
        3 => (1.0 - u, 1.0 - v),
        4 => (u, 1.0 - v),
        5 => (v, u),
        6 => (v, 1.0 - u),
        7 => (1.0 - v, 1.0 - u),
        8 => (1.0 - v, u),
        _ => (u, v),
    }
}

/// [`upright_position`] for fractions: stored (x, y) → upright (u, v).
pub fn upright_fraction(orientation: u16, x: f64, y: f64) -> (f64, f64) {
    match orientation {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (1.0 - y, x),
        7 => (1.0 - y, 1.0 - x),
        8 => (y, 1.0 - x),
        _ => (x, y),
    }
}

/// Orientations that mirror the image (a flip, possibly with a turn).
fn mirrored(orientation: u16) -> bool {
    matches!(orientation, 2 | 4 | 5 | 7)
}

/// A crop rectangle as the user sees it: on the upright (EXIF-oriented),
/// uncropped photo. The GUI's crop tool works in this frame; [`stored_crop`]
/// and [`upright_crop`] convert to and from Camera Raw's tags, which are in
/// the stored orientation (docs/design/crop.md).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UprightCrop {
    /// Centre, in upright pixels of the uncropped image (edges at 0 and its size).
    pub cx: f64,
    pub cy: f64,
    /// Size along the rectangle's own axes, in upright pixels.
    pub width: f64,
    pub height: f64,
    /// Straighten angle in degrees as seen on screen: the rectangle's x axis
    /// runs at `ANGLE_SIGN × angle` degrees (y down) over the photo, so a
    /// positive angle turns the photo counter-clockwise when `ANGLE_SIGN` is
    /// +1. It is `CropAngle` itself, negated for mirrored orientations.
    pub angle: f64,
}

/// The upright view of `crop` on a `w × h` stored image. A crop that is off
/// is the whole frame.
pub fn upright_crop(crop: &Crop, w: usize, h: usize, orientation: u16) -> UprightCrop {
    let (ow, oh) = oriented_size(w, h, orientation);
    let (ow, oh) = (ow as f64, oh as f64);
    let values = [crop.left, crop.top, crop.right, crop.bottom, crop.angle];
    if !crop.enabled || values.iter().any(|v| !v.is_finite()) {
        return UprightCrop {
            cx: ow / 2.0,
            cy: oh / 2.0,
            width: ow,
            height: oh,
            angle: 0.0,
        };
    }
    let (wf, hf) = (w as f64, h as f64);
    let angle = crop.angle.clamp(-MAX_ANGLE, MAX_ANGLE);
    let (sin, cos) = (ANGLE_SIGN * angle).to_radians().sin_cos();
    // Same axes as `Crop::geometry`: the stored-orientation output runs along
    // u = (cos, sin) and v = (−sin, cos).
    let d = ((crop.right - crop.left) * wf, (crop.bottom - crop.top) * hf);
    let cw = (d.0 * cos + d.1 * sin).max(0.0);
    let ch = (-d.0 * sin + d.1 * cos).max(0.0);
    let (u, v) = upright_fraction(
        orientation,
        (crop.left + crop.right) / 2.0,
        (crop.top + crop.bottom) / 2.0,
    );
    // The output is oriented after the crop: a quarter turn swaps its sides,
    // a mirror reverses the sense of rotation.
    let swap = (5..=8).contains(&orientation);
    UprightCrop {
        cx: u * ow,
        cy: v * oh,
        width: if swap { ch } else { cw },
        height: if swap { cw } else { ch },
        angle: if mirrored(orientation) { -angle } else { angle },
    }
}

/// The stored crop (enabled) for an upright rectangle on a `w × h` stored
/// image: the inverse of [`upright_crop`].
pub fn stored_crop(r: &UprightCrop, w: usize, h: usize, orientation: u16) -> Crop {
    let (ow, oh) = oriented_size(w, h, orientation);
    let (wf, hf) = (w as f64, h as f64);
    let screen = r.angle.clamp(-MAX_ANGLE, MAX_ANGLE);
    let angle = if mirrored(orientation) {
        -screen
    } else {
        screen
    };
    let (sin, cos) = (ANGLE_SIGN * angle).to_radians().sin_cos();
    let (cw, ch) = if (5..=8).contains(&orientation) {
        (r.height, r.width)
    } else {
        (r.width, r.height)
    };
    let (fx, fy) = stored_fraction(orientation, r.cx / ow as f64, r.cy / oh as f64);
    let (cx, cy) = (fx * wf, fy * hf);
    // Half the diagonal from the top-left to the bottom-right corner:
    // (cw·u + ch·v) / 2.
    let half = (0.5 * (cw * cos - ch * sin), 0.5 * (cw * sin + ch * cos));
    Crop {
        enabled: true,
        left: (cx - half.0) / wf,
        top: (cy - half.1) / hf,
        right: (cx + half.0) / wf,
        bottom: (cy + half.1) / hf,
        angle,
    }
}

/// Rotate/flip pixels upright according to an EXIF orientation (1..8).
pub fn orient(img: &Image, orientation: u16) -> Image {
    let (w, h) = (img.width, img.height);
    if orientation <= 1 || orientation > 8 {
        return img.clone();
    }
    let (ow, oh) = oriented_size(w, h, orientation);
    Image::from_fn(ow, oh, |x, y| {
        let (sx, sy) = stored_position(orientation, w, h, x, y);
        img.get(sx, sy)
    })
}

/// How to size the export (Lightroom's "Resize to Fit" choices).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resize {
    None,
    LongEdge(u32),
    ShortEdge(u32),
    Width(u32),
    Height(u32),
    /// Fit inside width × height.
    Fit(u32, u32),
    Megapixels(f64),
    Percent(f64),
    /// Exactly width × height (a size preset such as Instagram's 1080×1350).
    /// When the aspect differs, the image is scaled to cover that size and
    /// centre-cropped ([`Resize::cover_window`]). Enlarges when needed, since
    /// the size is what the destination asks for.
    Exact(u32, u32),
}

impl Resize {
    /// Target size for a `w × h` image; never enlarges unless `enlarge`.
    pub fn target(self, w: usize, h: usize, enlarge: bool) -> (usize, usize) {
        let (wf, hf) = (w as f64, h as f64);
        let scale = match self {
            Resize::None => 1.0,
            Resize::LongEdge(n) => f64::from(n) / wf.max(hf),
            Resize::ShortEdge(n) => f64::from(n) / wf.min(hf),
            Resize::Width(n) => f64::from(n) / wf,
            Resize::Height(n) => f64::from(n) / hf,
            Resize::Fit(mw, mh) => (f64::from(mw) / wf).min(f64::from(mh) / hf),
            Resize::Megapixels(mp) => (mp * 1e6 / (wf * hf)).sqrt(),
            Resize::Percent(p) => p / 100.0,
            Resize::Exact(tw, th) if tw > 0 && th > 0 => return (tw as usize, th as usize),
            Resize::Exact(..) => 1.0,
        };
        let scale = if enlarge { scale } else { scale.min(1.0) };
        if !(scale.is_finite() && scale > 0.0) || (scale - 1.0).abs() < 1e-9 {
            return (w, h);
        }
        (
            ((wf * scale).round() as usize).max(1),
            ((hf * scale).round() as usize).max(1),
        )
    }
}

impl Resize {
    /// For [`Resize::Exact`] with an aspect that differs from `w × h`: the
    /// centred part of the image to keep, `(x, y, width, height)`, so that
    /// scaling it gives exactly the target size (cover, then centre-crop).
    /// `None` when nothing needs cutting (the same aspect to within a pixel).
    pub fn cover_window(self, w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
        let Resize::Exact(tw, th) = self else {
            return None;
        };
        if tw == 0 || th == 0 || w == 0 || h == 0 {
            return None;
        }
        let target = f64::from(tw) / f64::from(th);
        let (wf, hf) = (w as f64, h as f64);
        let (cw, ch) = if wf / hf > target {
            ((hf * target).round() as usize, h)
        } else {
            (w, (wf / target).round() as usize)
        };
        let (cw, ch) = (cw.clamp(1, w), ch.clamp(1, h));
        ((cw, ch) != (w, h)).then_some(((w - cw) / 2, (h - ch) / 2, cw, ch))
    }
}

/// The `width × height` part of `img` whose top-left pixel is (`x`, `y`).
pub fn cut(img: &Image, x: usize, y: usize, width: usize, height: usize) -> Image {
    Image::from_fn(width, height, |i, j| img.get(x + i, y + j))
}

fn lanczos3(x: f64) -> f64 {
    let x = x.abs();
    if x < 1e-9 {
        1.0
    } else if x < 3.0 {
        let px = std::f64::consts::PI * x;
        3.0 * px.sin() * (px / 3.0).sin() / (px * px)
    } else {
        0.0
    }
}

/// Normalised Lanczos-3 taps for resampling `src_len` → `dst_len`:
/// (first source index, weights) per output index.
fn taps(src_len: usize, dst_len: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = src_len as f64 / dst_len as f64;
    let support = 3.0 * scale.max(1.0);
    let stretch = scale.max(1.0);
    (0..dst_len)
        .map(|i| {
            let centre = (i as f64 + 0.5) * scale - 0.5;
            let lo = (centre - support).floor().max(0.0) as usize;
            let hi = ((centre + support).ceil() as usize).min(src_len - 1);
            let mut w: Vec<f64> = (lo..=hi)
                .map(|j| lanczos3((j as f64 - centre) / stretch))
                .collect();
            let sum: f64 = w.iter().sum();
            w.iter_mut().for_each(|v| *v /= sum);
            (lo, w.into_iter().map(|v| v as f32).collect())
        })
        .collect()
}

/// Lanczos-3 resize of an interleaved RGB image (separable, in the image's own
/// encoding; call it on linear values).
pub fn resize(img: &Image, ow: usize, oh: usize) -> Image {
    let (w, h) = (img.width, img.height);
    if (ow, oh) == (w, h) {
        return img.clone();
    }
    let tx = taps(w, ow);
    let ty = taps(h, oh);
    // Horizontal pass: h rows of width ow.
    let mut mid = vec![0.0f32; ow * h * 3];
    mid.par_chunks_mut(ow * 3)
        .zip(img.rgb.par_chunks(w * 3))
        .for_each(|(out, row)| {
            for (x, (lo, weights)) in tx.iter().enumerate() {
                let mut acc = [0.0f32; 3];
                for (k, wt) in weights.iter().enumerate() {
                    let p = &row[(lo + k) * 3..(lo + k) * 3 + 3];
                    acc[0] += wt * p[0];
                    acc[1] += wt * p[1];
                    acc[2] += wt * p[2];
                }
                out[x * 3..x * 3 + 3].copy_from_slice(&acc);
            }
        });
    // Vertical pass, row by row (contiguous rows).
    let mut out = Image::new(ow, oh);
    out.rgb
        .par_chunks_mut(ow * 3)
        .enumerate()
        .for_each(|(y, orow)| {
            let (lo, weights) = &ty[y];
            for (k, wt) in weights.iter().enumerate() {
                let src = &mid[(lo + k) * ow * 3..(lo + k + 1) * ow * 3];
                for (o, s) in orow.iter_mut().zip(src) {
                    *o += wt * s;
                }
            }
        });
    out
}

/// Output sharpening target (Lightroom's "Sharpen For").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharpenFor {
    Screen,
    MattePaper,
    GlossyPaper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharpenAmount {
    Low,
    Standard,
    High,
}

/// Parse "screen", "matte:high", "glossy:low", ...
pub fn parse_sharpen(s: &str) -> Option<(SharpenFor, SharpenAmount)> {
    let (media, amount) = s.split_once(':').unwrap_or((s, "standard"));
    let media = match media.to_ascii_lowercase().as_str() {
        "screen" => SharpenFor::Screen,
        "matte" | "matte-paper" => SharpenFor::MattePaper,
        "glossy" | "glossy-paper" => SharpenFor::GlossyPaper,
        _ => return None,
    };
    let amount = match amount.to_ascii_lowercase().as_str() {
        "low" => SharpenAmount::Low,
        "standard" | "std" => SharpenAmount::Standard,
        "high" => SharpenAmount::High,
        _ => return None,
    };
    Some((media, amount))
}

/// Unsharp mask on encoded (display) values, sized for the output medium.
/// Model v1 (ours): radius in output pixels and strength per medium/amount.
pub fn output_sharpen(img: &mut Image, media: SharpenFor, amount: SharpenAmount) {
    let (sigma, base) = match media {
        SharpenFor::Screen => (0.6f32, 0.45f32),
        SharpenFor::MattePaper => (1.0, 0.6),
        SharpenFor::GlossyPaper => (0.8, 0.5),
    };
    let k = base
        * match amount {
            SharpenAmount::Low => 0.55,
            SharpenAmount::Standard => 1.0,
            SharpenAmount::High => 1.6,
        };
    let (w, h) = (img.width, img.height);
    for c in 0..3 {
        let plane: Vec<f32> = img.rgb.iter().skip(c).step_by(3).copied().collect();
        let blurred = amb_engine::filters::gaussian_blur(&plane, w, h, sigma);
        img.rgb
            .par_chunks_mut(3)
            .zip(plane.par_iter().zip(blurred.par_iter()))
            .for_each(|(p, (v, b))| p[c] = v + k * (v - b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(w: usize, h: usize) -> Image {
        Image::from_fn(w, h, |x, y| [x as f32, y as f32, 0.0])
    }

    #[test]
    fn orientations_put_the_top_left_pixel_where_exif_says() {
        let img = numbered(3, 2);
        // Orientation 6: the stored image must be rotated 90° clockwise; the
        // stored bottom-left pixel becomes the visual top-left.
        let r = orient(&img, 6);
        assert_eq!((r.width, r.height), (2, 3));
        assert_eq!(r.get(0, 0), [0.0, 1.0, 0.0]);
        assert_eq!(r.get(1, 0), [0.0, 0.0, 0.0]);
        let r8 = orient(&img, 8);
        assert_eq!(r8.get(0, 0), [2.0, 0.0, 0.0]);
        assert_eq!(orient(&img, 3).get(0, 0), [2.0, 1.0, 0.0]);
        assert_eq!(orient(&img, 1).rgb, img.rgb);
        // 6 then 8 is the identity.
        assert_eq!(orient(&orient(&img, 6), 8).rgb, img.rgb);
    }

    /// Camera Raw stores the crop in the stored (sensor) orientation and the
    /// engine crops there; export orients afterwards. A crop of the stored
    /// left half of an orientation-6 photo (rotated 90° clockwise for display)
    /// is the upright photo's top half.
    #[test]
    fn crop_in_stored_orientation_then_orient() {
        use amb_engine::crop::Crop;
        use amb_engine::pipeline::{BaseLook, DevelopSettings, render};
        let (w, h) = (60, 40);
        let rgb = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                [0.1 + 0.01 * x, 0.1 + 0.01 * y, 0.3]
            })
            .collect();
        let img = amb_engine::raw::CameraImage {
            width: w,
            height: h,
            rgb,
            xyz_to_cam: amb_color::matrix::invert3(&amb_color::matrix::PROPHOTO_TO_XYZ_D50),
            as_shot_neutral: [1.0; 3],
            clip: [f32::INFINITY; 3],
            rendered: false,
            make: "Test".into(),
            model: "Synthetic".into(),
            shot: Default::default(),
            metadata: Default::default(),
        };
        let mut s = DevelopSettings {
            base_look: BaseLook::Linear,
            ..Default::default()
        };
        s.sharpening.amount = 0.0;
        s.noise_reduction.color = 0.0;
        let full = orient(&render(&img, &s), 6);
        s.crop = Crop {
            enabled: true,
            right: 0.5,
            ..Default::default()
        };
        let cropped = orient(&render(&img, &s), 6);
        assert_eq!((full.width, full.height), (40, 60));
        assert_eq!((cropped.width, cropped.height), (40, 30));
        for y in 0..30 {
            for x in 0..40 {
                let (a, b) = (cropped.get(x, y), full.get(x, y));
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 1e-5, "({x},{y}) {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn upright_and_stored_positions_are_inverse_in_every_orientation() {
        let (w, h) = (5, 3);
        let img = numbered(w, h);
        for o in 1..=8u16 {
            let (ow, oh) = oriented_size(w, h, o);
            let upright = orient(&img, o);
            for y in 0..h {
                for x in 0..w {
                    let (ux, uy) = upright_position(o, w, h, x, y);
                    assert!(ux < ow && uy < oh, "orientation {o}");
                    assert_eq!(stored_position(o, w, h, ux, uy), (x, y), "orientation {o}");
                    assert_eq!(upright.get(ux, uy), [x as f32, y as f32, 0.0]);
                    // The fraction maps agree at pixel centres, and invert.
                    let (fx, fy) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
                    let (fu, fv) = upright_fraction(o, fx, fy);
                    assert!((fu - (ux as f64 + 0.5) / ow as f64).abs() < 1e-12, "{o}");
                    assert!((fv - (uy as f64 + 0.5) / oh as f64).abs() < 1e-12, "{o}");
                    let (bx, by) = stored_fraction(o, fu, fv);
                    assert!((bx - fx).abs() < 1e-12 && (by - fy).abs() < 1e-12, "{o}");
                }
            }
        }
    }

    /// The crop tool's upright rectangle, turned into tags, makes the engine
    /// crop exactly that rectangle once the output is oriented: checked
    /// pixel by pixel through `Crop::geometry` for all 8 orientations, with
    /// and without a straighten angle.
    #[test]
    fn upright_crops_match_the_engine_crop_in_every_orientation() {
        let (w, h) = (600usize, 400usize);
        for o in 1..=8u16 {
            let (ow, oh) = oriented_size(w, h, o);
            let (ow, oh) = (ow as f64, oh as f64);
            for angle in [0.0, 7.5, -12.0] {
                // Whole-pixel sizes, so the engine does not stretch the crop.
                let r = UprightCrop {
                    cx: 0.45 * ow,
                    cy: 0.55 * oh,
                    width: 0.4 * ow,
                    height: 0.3 * oh,
                    angle,
                };
                let crop = stored_crop(&r, w, h, o);
                let back = upright_crop(&crop, w, h, o);
                for (a, b) in [
                    (back.cx, r.cx),
                    (back.cy, r.cy),
                    (back.width, r.width),
                    (back.height, r.height),
                    (back.angle, r.angle),
                ] {
                    assert!((a - b).abs() < 1e-9, "orientation {o}: {back:?} vs {r:?}");
                }
                let g = crop.geometry(w, h).expect("a crop");
                let (gw, gh) = oriented_size(g.width, g.height, o);
                assert_eq!((gw as f64, gh as f64), (r.width, r.height), "{o}");
                let (sin, cos) = (ANGLE_SIGN * r.angle).to_radians().sin_cos();
                for (x, y) in [(0, 0), (gw - 1, 0), (0, gh - 1), (17, 9), (gw - 1, gh - 1)] {
                    // Upright output pixel → stored output pixel → frame →
                    // upright photo.
                    let (sx, sy) = stored_position(o, g.width, g.height, x, y);
                    let (px, py) = g.source(sx as f64, sy as f64);
                    let (u, v) = upright_fraction(o, (px + 0.5) / w as f64, (py + 0.5) / h as f64);
                    let got = (u * ow, v * oh);
                    // Where the upright rectangle puts that pixel's centre.
                    let (i, j) = (
                        x as f64 + 0.5 - r.width / 2.0,
                        y as f64 + 0.5 - r.height / 2.0,
                    );
                    let want = (r.cx + i * cos - j * sin, r.cy + i * sin + j * cos);
                    assert!(
                        (got.0 - want.0).hypot(got.1 - want.1) < 1e-6,
                        "orientation {o}, angle {angle}, pixel ({x},{y}): {got:?} vs {want:?}"
                    );
                }
            }
        }
        // No crop is the whole frame; the stored left half of an
        // orientation-6 photo is its upright top half.
        let full = upright_crop(&Crop::default(), 60, 40, 6);
        assert_eq!(
            (full.cx, full.cy, full.width, full.height),
            (20.0, 30.0, 40.0, 60.0)
        );
        let half = Crop {
            enabled: true,
            right: 0.5,
            ..Default::default()
        };
        let top = upright_crop(&half, 60, 40, 6);
        assert_eq!(
            (top.cx, top.cy, top.width, top.height),
            (20.0, 15.0, 40.0, 30.0)
        );
    }

    #[test]
    fn exact_resize_covers_then_centre_crops() {
        // Same aspect: no cut, an exact size even when enlarging.
        assert_eq!(Resize::Exact(1500, 1000).cover_window(6000, 4000), None);
        assert_eq!(
            Resize::Exact(1500, 1000).target(6000, 4000, false),
            (1500, 1000)
        );
        assert_eq!(
            Resize::Exact(9000, 6000).target(6000, 4000, false),
            (9000, 6000)
        );
        // 3:2 → 4:5 keeps the middle 3200×4000, which scales to 1080×1350.
        assert_eq!(
            Resize::Exact(1080, 1350).cover_window(6000, 4000),
            Some((1400, 0, 3200, 4000))
        );
        // 3:2 → 16:9 keeps the full width.
        assert_eq!(
            Resize::Exact(1600, 900).cover_window(6000, 4000),
            Some((0, 312, 6000, 3375))
        );
        // Other modes never cut.
        assert_eq!(Resize::Fit(100, 100).cover_window(6000, 4000), None);
        let img = numbered(10, 6);
        let c = cut(&img, 2, 1, 4, 3);
        assert_eq!((c.width, c.height), (4, 3));
        assert_eq!(c.get(0, 0), [2.0, 1.0, 0.0]);
        assert_eq!(c.get(3, 2), [5.0, 3.0, 0.0]);
    }

    #[test]
    fn resize_targets() {
        assert_eq!(
            Resize::LongEdge(2048).target(6000, 4000, false),
            (2048, 1365)
        );
        assert_eq!(
            Resize::ShortEdge(1000).target(6000, 4000, false),
            (1500, 1000)
        );
        assert_eq!(
            Resize::Fit(1000, 1000).target(6000, 4000, false),
            (1000, 667)
        );
        assert_eq!(
            Resize::Megapixels(6.0).target(6000, 4000, false),
            (3000, 2000)
        );
        assert_eq!(
            Resize::Percent(50.0).target(6000, 4000, false),
            (3000, 2000)
        );
        // No enlarging unless asked.
        assert_eq!(
            Resize::LongEdge(8000).target(6000, 4000, false),
            (6000, 4000)
        );
        assert_eq!(
            Resize::LongEdge(8000).target(6000, 4000, true),
            (8000, 5333)
        );
    }

    #[test]
    fn resize_preserves_flat_fields_and_averages() {
        let flat = Image::from_fn(300, 200, |_, _| [0.25, 0.5, 0.75]);
        let small = resize(&flat, 97, 61);
        assert_eq!((small.width, small.height), (97, 61));
        assert!(
            small
                .rgb
                .chunks(3)
                .all(|p| (p[0] - 0.25).abs() < 1e-4 && (p[2] - 0.75).abs() < 1e-4)
        );
        // A fine checkerboard averages to its mean when downscaled a lot.
        let checker = Image::from_fn(400, 400, |x, y| [((x + y) % 2) as f32; 3]);
        let down = resize(&checker, 40, 40);
        assert!(down.rgb.iter().all(|v| (v - 0.5).abs() < 0.02));
    }

    #[test]
    fn output_sharpening_raises_edge_contrast_only() {
        let mut img = Image::from_fn(40, 10, |x, _| [if x < 20 { 0.3 } else { 0.7 }; 3]);
        output_sharpen(&mut img, SharpenFor::Screen, SharpenAmount::High);
        assert!(img.get(19, 5)[0] < 0.3 && img.get(20, 5)[0] > 0.7);
        assert!(
            (img.get(5, 5)[0] - 0.3).abs() < 1e-5,
            "flat areas untouched"
        );
        assert_eq!(
            parse_sharpen("matte:high"),
            Some((SharpenFor::MattePaper, SharpenAmount::High))
        );
        assert_eq!(
            parse_sharpen("screen"),
            Some((SharpenFor::Screen, SharpenAmount::Standard))
        );
        assert_eq!(parse_sharpen("canvas"), None);
    }
}
