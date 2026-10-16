// SPDX-License-Identifier: GPL-3.0-or-later
// Stage 3's per-pixel parts: the base look (scene-linear to display-linear)
// and the global tone curve on luminance; stage 5's curves. pipeline/mod.rs is
// the reference.

struct Tone {
    // The base look: 0 linear, 1 the default filmic curve.
    base: u32,
    lum: array<f32, 3>,
    // Where each table starts in `luts`, or 0xffffffff when unused: the tone
    // curve, then the parametric, RGB, red, green and blue curves.
    tone_lut: u32,
    curves: array<u32, 5>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read> size: Size;
@group(0) @binding(2) var<storage, read> tp: Tone;
@group(0) @binding(3) var<storage, read> luts: array<f32>;

const NONE: u32 = 0xffffffffu;

// Linear interpolation in a table over 0..1 (pipeline/mod.rs `lookup`).
fn lut_at(lut_offset: u32, v: f32) -> f32 {
    let n = size.lut_size;
    let pos = clamp(v, 0.0, 1.0) * f32(n - 1u);
    let i = min(u32(pos), n - 2u);
    let f = pos - f32(i);
    return luts[lut_offset + i] * (1.0 - f) + luts[lut_offset + i + 1u] * f;
}


fn filmic(x0: f32) -> f32 {
    // Narkowicz's fit of the ACES filmic curve (BaseLook::Default).
    let x = max(x0, 0.0);
    return min((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 1.0);
}

@compute @workgroup_size(256)
fn base_look(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels || tp.base == 0u) {
        return;
    }
    for (var c = 0u; c < 3u; c++) {
        img[i * 3u + c] = filmic(img[i * 3u + c]);
    }
}

@compute @workgroup_size(256)
fn tone_curve(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels || tp.tone_lut == NONE) {
        return;
    }
    // Hue-preserving: scale RGB by the luminance curve's ratio.
    let p = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let y = tp.lum[0] * p.x + tp.lum[1] * p.y + tp.lum[2] * p.z;
    if (y > 1e-6) {
        let target_y = srgb_decode(lut_at(tp.tone_lut, srgb_encode(min(y, 1.0))));
        let s = target_y / min(y, 1.0);
        for (var c = 0u; c < 3u; c++) {
            img[i * 3u + c] = p[c] * s;
        }
    }
}

@compute @workgroup_size(256)
fn curves(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels) {
        return;
    }
    // Curves act on encoded values.
    for (var c = 0u; c < 3u; c++) {
        var e = srgb_encode(clamp(img[i * 3u + c], 0.0, 1.0));
        if (tp.curves[0] != NONE) {
            e = lut_at(tp.curves[0], e);
        }
        if (tp.curves[1] != NONE) {
            e = lut_at(tp.curves[1], e);
        }
        if (tp.curves[2u + c] != NONE) {
            e = lut_at(tp.curves[2u + c], e);
        }
        img[i * 3u + c] = srgb_decode(e);
    }
}
