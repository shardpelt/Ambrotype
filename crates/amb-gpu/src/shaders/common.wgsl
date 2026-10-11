// SPDX-License-Identifier: GPL-3.0-or-later
// Helpers shared by every kernel, written to match the CPU code exactly.

// The thread's pixel (1-D dispatch, wrapped into rows of 65535 groups).
fn pixel_index(id: vec3<u32>, groups: vec3<u32>) -> u32 {
    return id.x + id.y * groups.x * 256u;
}

fn smoothstep_(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn srgb_encode(v: f32) -> f32 {
    if (v <= 0.0031308) {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn srgb_decode(v: f32) -> f32 {
    if (v <= 0.04045) {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn cbrt(x: f32) -> f32 {
    return sign(x) * pow(abs(x), 1.0 / 3.0);
}

struct Size {
    width: u32,
    height: u32,
    pixels: u32,
    lut_size: u32,
}

// Linear interpolation in a table over 0..1 (pipeline.rs `lookup`).
fn lut_at(lut_offset: u32, v: f32) -> f32 {
    let n = size.lut_size;
    let pos = clamp(v, 0.0, 1.0) * f32(n - 1u);
    let i = min(u32(pos), n - 2u);
    let f = pos - f32(i);
    return luts[lut_offset + i] * (1.0 - f) + luts[lut_offset + i + 1u] * f;
}
