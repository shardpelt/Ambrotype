// SPDX-License-Identifier: GPL-3.0-or-later
//! The editor's scopes: the histogram and the vectorscope of the displayed
//! (sRGB, 8-bit) preview. Pure functions on the pixels that are shown.

/// Bins on each side of the vectorscope's square plot.
pub const VECTORSCOPE_SIZE: usize = 128;

/// The usual skin-tone line, in degrees from the +Cb axis counter-clockwise.
/// The editor's script draws it at the same angle (`SCOPE_SKIN_DEG`).
pub const SKIN_LINE_DEG: f64 = 123.0;

/// BT.709 luma weights, applied to gamma-encoded R'G'B'.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// BT.709: `Cb = (B' - Y') / 1.8556`, `Cr = (R' - Y') / 1.5748`, each in -0.5..0.5.
const CB_DIVISOR: f32 = 1.8556;
const CR_DIVISOR: f32 = 1.5748;

/// Per-channel 256-bin histogram of packed RGB8 pixels.
pub fn histogram(rgb: &[u8]) -> [Vec<u32>; 3] {
    let mut h = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
    for p in rgb.chunks_exact(3) {
        for c in 0..3 {
            h[c][p[c] as usize] += 1;
        }
    }
    h
}

/// Cb and Cr of a pixel, each in -0.5..0.5.
fn chroma(rgb: [u8; 3]) -> (f32, f32) {
    let [r, g, b] = rgb.map(|v| v as f32 / 255.0);
    let y = LUMA[0] * r + LUMA[1] * g + LUMA[2] * b;
    ((b - y) / CB_DIVISOR, (r - y) / CR_DIVISOR)
}

/// Where a colour lands on the vectorscope, as 0..1 across and down the plot
/// (y down). Grey is the centre. This is the broadcast convention (Cb across,
/// Cr up), so red sits at the upper left, blue at the right and skin near 123
/// degrees.
pub fn vectorscope_point(rgb: [u8; 3]) -> [f32; 2] {
    let (cb, cr) = chroma(rgb);
    [cb + 0.5, 0.5 - cr]
}

/// The vectorscope of packed RGB8 pixels: `VECTORSCOPE_SIZE` squared counts,
/// row by row, y down. Hue is the angle from the centre, saturation the
/// distance.
pub fn vectorscope(rgb: &[u8]) -> Vec<u32> {
    let n = VECTORSCOPE_SIZE;
    let mut bins = vec![0u32; n * n];
    let last = (n - 1) as f32;
    for p in rgb.chunks_exact(3) {
        let [x, y] = vectorscope_point([p[0], p[1], p[2]]);
        let x = (x * n as f32).clamp(0.0, last) as usize;
        let y = (y * n as f32).clamp(0.0, last) as usize;
        bins[y * n + x] += 1;
    }
    bins
}

#[cfg(test)]
mod tests {
    use super::*;

    fn angle(rgb: [u8; 3]) -> f64 {
        let [x, y] = vectorscope_point(rgb);
        let (cb, cr) = ((x - 0.5) as f64, (0.5 - y) as f64);
        cr.atan2(cb).to_degrees().rem_euclid(360.0)
    }

    #[test]
    fn histogram_counts_every_pixel_per_channel() {
        let h = histogram(&[0, 10, 255, 0, 10, 255]);
        assert_eq!((h[0][0], h[1][10], h[2][255]), (2, 2, 2));
        assert!(h.iter().all(|c| c.iter().sum::<u32>() == 2));
    }

    #[test]
    fn grey_is_the_centre() {
        for v in [0u8, 90, 255] {
            let [x, y] = vectorscope_point([v, v, v]);
            assert!((x - 0.5).abs() < 1e-6 && (y - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn red_and_blue_land_where_the_broadcast_scope_has_them() {
        let red = angle([255, 0, 0]);
        let blue = angle([0, 0, 255]);
        assert!((red - 103.0).abs() < 2.0, "red {red}");
        assert!((blue - 355.0).abs() < 2.0, "blue {blue}");
        assert_eq!(vectorscope_point([0, 0, 255])[0], 1.0);
    }

    #[test]
    fn skin_lands_near_the_skin_line() {
        let a = angle([200, 160, 140]);
        assert!((a - SKIN_LINE_DEG).abs() < 5.0, "skin at {a}");
        assert_eq!(SKIN_LINE_DEG, 123.0);
    }

    #[test]
    fn vectorscope_bins_every_pixel_once() {
        let px = [255, 0, 0, 255, 0, 0, 128, 128, 128, 0, 0, 255];
        let v = vectorscope(&px);
        assert_eq!(v.len(), VECTORSCOPE_SIZE * VECTORSCOPE_SIZE);
        assert_eq!(v.iter().sum::<u32>(), 4);
        let mid = VECTORSCOPE_SIZE / 2;
        assert_eq!(v[mid * VECTORSCOPE_SIZE + mid], 1);
    }
}
