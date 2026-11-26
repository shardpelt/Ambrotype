// SPDX-License-Identifier: GPL-3.0-or-later
//! The two-ink styles: Dither (random dots) and the line screens Rings and
//! Lines, where the line gets thicker the darker the photo is.

use super::{Common, PERCENT, contrasted, inks, luma, spacing};
use amb_plugin_sdk::{Effect, Pixels};

/// A dither cell is at least one pixel wide.
const CELL_MIN_PX: f32 = 1.0;
/// Spacing floor for a line screen, in pixels (below it nothing resolves).
const SCREEN_MIN_PX: f32 = 2.0;
/// The thinnest a line gets, as a share of the spacing: a hairline in the
/// highlights, so a white photo still shows its screen.
const HAIRLINE: f32 = 0.04;
/// The edge of a line is smoothed over this many output pixels.
const EDGE_PX: f32 = 1.0;
/// Each channel's screen moves by fringe/100 × spacing × this.
const FRINGE_SHIFT: f32 = 0.5;
/// How far each colour channel's screen moves: red one way, green not, blue
/// the other.
const FRINGE_SIGN: [f32; 3] = [1.0, 0.0, -1.0];

/// Random dots in two inks: a pixel is light where its brightness beats a
/// random threshold, one per cell, so mid-tones are a dense mix.
pub(crate) fn dither(e: &Effect, px: &mut Pixels, common: &Common) {
    let cell = spacing(e, CELL_MIN_PX).round().max(CELL_MIN_PX);
    let ink = e.choice("ink");
    for y in 0..px.height {
        for x in 0..px.width {
            let (fx, fy) = (e.x + x as u32, e.y + y as u32);
            let c = px.get(x, y);
            let v = contrasted(luma(c), common.contrast);
            let t = e.noise((fx as f32 / cell) as u32, (fy as f32 / cell) as u32);
            let (dark, light) = inks(ink, c);
            px.set(x, y, common.mix(c, if v > t { light } else { dark }));
        }
    }
}

/// Rings round a centre.
pub(crate) fn rings(e: &Effect, px: &mut Pixels, common: &Common) {
    let sp = spacing(e, SCREEN_MIN_PX);
    let (fw, fh) = (e.full_width.max(1) as f32, e.full_height.max(1) as f32);
    let centre = [
        fw * e.number("x") as f32 / PERCENT,
        fh * e.number("y") as f32 / PERCENT,
    ];
    // A channel's shift moves the centre along x.
    screen(e, px, common, sp, |fx, fy, shift| {
        (fx - shift - centre[0]).hypot(fy - centre[1]) / sp
    });
}

/// Straight lines at an angle.
pub(crate) fn lines(e: &Effect, px: &mut Pixels, common: &Common) {
    let sp = spacing(e, SCREEN_MIN_PX);
    let (s, c) = (e.number("angle") as f32).to_radians().sin_cos();
    // The lines run along (c, s); the screen is measured across them. A
    // channel's shift moves it along that normal.
    screen(e, px, common, sp, |fx, fy, shift| {
        (fx * -s + fy * c - shift) / sp
    });
}

/// Draw a line screen: `coord(x, y, shift)` is the position across the
/// screen in line spacings; a line sits at each whole number, as thick as
/// the photo is dark.
fn screen(
    e: &Effect,
    px: &mut Pixels,
    common: &Common,
    sp: f32,
    coord: impl Fn(f32, f32, f32) -> f32,
) {
    let ink = e.choice("ink");
    let fringe = (e.number("fringe") as f32 / PERCENT).clamp(0.0, 1.0);
    let shift = fringe * sp * FRINGE_SHIFT;
    // Half an output pixel, in spacings: the smoothing reaches that far
    // each side of a line's edge.
    let aa = EDGE_PX / sp / 2.0;
    for y in 0..px.height {
        for x in 0..px.width {
            let (fx, fy) = ((e.x + x as u32) as f32 + 0.5, (e.y + y as u32) as f32 + 0.5);
            let c = px.get(x, y);
            let v = contrasted(luma(c), common.contrast);
            let darkness = (1.0 - v).max(HAIRLINE);
            let (dark, light) = inks(ink, c);
            let out = [0, 1, 2].map(|k| {
                let r = coord(fx, fy, FRINGE_SIGN[k] * shift);
                // How far inside the line's edge this pixel is.
                let m = darkness / 2.0 - (r - r.round()).abs();
                let on = smooth(m / aa);
                light[k] + (dark[k] - light[k]) * on
            });
            px.set(x, y, common.mix(c, out));
        }
    }
}

/// 0 below -1, 1 above 1, smooth between.
fn smooth(t: f32) -> f32 {
    let u = (t * 0.5 + 0.5).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{effect, ramp, run};
    use serde_json::json;

    const GAME_BOY: [[f32; 3]; 2] = [[0.25, 0.32, 0.24], [0.77, 0.94, 0.85]];

    #[test]
    fn dither_holds_only_the_two_inks() {
        let (w, h) = (60, 40);
        let mut data = ramp(w, h);
        let e = effect(json!({ "size": 0.5 }), w as u32, h as u32);
        run(&e, &mut data, w, h);
        assert!(
            data.chunks_exact(3)
                .all(|c| GAME_BOY.iter().any(|i| i[..] == *c))
        );
        let light = data.chunks_exact(3).filter(|c| c[0] > 0.5).count();
        assert!(light > 0 && light < w * h, "a ramp gives a mix");
    }

    #[test]
    fn dither_black_is_dark_and_white_is_light() {
        let (w, h) = (30, 30);
        for (v, ink) in [(0.0, GAME_BOY[0]), (1.0, GAME_BOY[1])] {
            let mut data = vec![v; w * h * 3];
            run(&effect(json!({}), w as u32, h as u32), &mut data, w, h);
            assert!(data.chunks_exact(3).all(|c| c[..] == ink), "{v}");
        }
    }

    fn mean(data: &[f32]) -> f32 {
        data.iter().sum::<f32>() / data.len() as f32
    }

    #[test]
    fn rings_follow_the_darkness() {
        let (w, h) = (100, 100);
        let e = effect(
            json!({ "style": "Rings", "ink": "Black and white", "size": 4 }),
            100,
            100,
        );
        let mut black = vec![0.0; w * h * 3];
        run(&e, &mut black, w, h);
        let mut white = vec![1.0; w * h * 3];
        run(&e, &mut white, w, h);
        assert!(mean(&black) < 0.3, "{}", mean(&black));
        assert!(mean(&white) > 0.7, "{}", mean(&white));
    }

    fn coloured(data: &[f32]) -> usize {
        data.chunks_exact(3)
            .filter(|c| (c[0] - c[1]).abs() > 0.1 || (c[2] - c[1]).abs() > 0.1)
            .count()
    }

    #[test]
    fn fringe_gives_colour_only_when_asked() {
        let (w, h) = (100, 60);
        for style in ["Rings", "Lines"] {
            let plain = json!({ "style": style, "ink": "Black and white", "size": 3 });
            let mut flat = ramp(w, h);
            run(&effect(plain, 100, 60), &mut flat, w, h);
            assert_eq!(coloured(&flat), 0, "{style} without fringe");
            let fringed =
                json!({ "style": style, "ink": "Black and white", "size": 3, "fringe": 80 });
            let mut data = ramp(w, h);
            run(&effect(fringed, 100, 60), &mut data, w, h);
            assert!(coloured(&data) > 0, "{style} with fringe");
        }
    }
}
