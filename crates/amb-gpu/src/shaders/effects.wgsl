// SPDX-License-Identifier: GPL-3.0-or-later
// The post-crop vignette (amb-engine's effects.rs is the reference): a
// separable superellipse mask, then exposure with highlight protection
// (Highlight or Color Priority) or a paint overlay.

struct Vignette {
    w: u32,
    h: u32,
    // Where this image sits in the full frame the mask is laid out on.
    x0: u32,
    y0: u32,
    p: f32,
    inner: f32,
    outer: f32,
    mirrored: u32,
    base: f32,
    norm: f32,
    q_skip: f32,
    // 0 Highlight Priority, 1 Color Priority, 2 Paint Overlay.
    style: u32,
    stops: f32,
    protect: f32,
    opacity: f32,
    target_white: f32,
    gamma: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read> col: array<f32>;
@group(0) @binding(2) var<storage, read> row: array<f32>;
@group(0) @binding(3) var<storage, read> vp: Vignette;

fn mask_at(x: u32, y: u32) -> f32 {
    let q = col[x] + row[y];
    if (q <= vp.q_skip) {
        return 0.0;
    }
    var d: f32;
    if (vp.p == 2.0) {
        d = sqrt(q);
    } else {
        d = pow(q, 1.0 / vp.p);
    }
    var s = smoothstep_(vp.inner, vp.outer, d);
    if (vp.mirrored == 1u) {
        s += smoothstep_(vp.inner, vp.outer, -d);
    }
    return clamp((s - vp.base) * vp.norm, 0.0, 1.0);
}

fn signed_pow(v: f32, e: f32) -> f32 {
    let m = pow(abs(v), e);
    return select(m, -m, v < 0.0);
}

fn protected_exposure(v: f32, g: f32, h: f32) -> f32 {
    let c = clamp(v, 0.0, 1.0);
    let den = 1.0 + (g - 1.0) * h * c;
    let out = c * g / den;
    if (v < 0.0) {
        return out + v * g;
    } else if (v > 1.0) {
        var slope = g / (den * den);
        if (g < 1.0) {
            slope = min(slope, 1.0);
        } else {
            slope = max(slope, 1.0);
        }
        return out + (v - 1.0) * slope;
    }
    return out;
}

@compute @workgroup_size(256)
fn vignette(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= vp.w * vp.h) { return; }
    let m = mask_at(vp.x0 + i % vp.w, vp.y0 + i / vp.w);
    if (m <= 0.0) { return; }
    var c = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    if (vp.style == 2u) {
        let a = vp.opacity * m;
        if (vp.target_white == 0.0) {
            c *= pow(1.0 - a, vp.gamma);
        } else {
            let inv = 1.0 / vp.gamma;
            for (var k = 0u; k < 3u; k++) {
                let e = signed_pow(c[k], inv);
                c[k] = signed_pow(e + (1.0 - e) * a, vp.gamma);
            }
        }
    } else if (vp.style == 0u) {
        let gain = exp2(vp.stops * m);
        for (var k = 0u; k < 3u; k++) {
            c[k] = protected_exposure(c[k], gain, vp.protect);
        }
    } else {
        let gain = exp2(vp.stops * m);
        let y = vp.lum[0] * c.x + vp.lum[1] * c.y + vp.lum[2] * c.z;
        var scale = gain;
        if (y > 1e-9) {
            scale = protected_exposure(y, gain, vp.protect) / y;
        }
        if (scale > 1.0) {
            // Never push a channel past 1: that would shift the hue.
            let mx = max(c.x, max(c.y, c.z));
            if (mx > 0.0) {
                scale = min(scale, max(1.0 / mx, 1.0));
            }
        }
        c *= scale;
    }
    img[i * 3u] = c.x;
    img[i * 3u + 1u] = c.y;
    img[i * 3u + 2u] = c.z;
}
