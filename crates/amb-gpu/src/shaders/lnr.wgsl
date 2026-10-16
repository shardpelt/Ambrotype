// SPDX-License-Identifier: GPL-3.0-or-later
// Luminance noise reduction (amb-engine's detail/ is the reference):
// self-guided filters on the centred encoded luminance with eps from the
// measured noise, Detail returning removed signal where its energy clears
// the noise, Contrast adding a low-pass of what is still removed, applied to
// RGB by luminance ratio. The box and Gaussian blurs are planes.wgsl's.

struct Lnr {
    w: u32,
    h: u32,
    // Noise tiles across and down (16 × 16 pixels: 8 × 8 blocks of 2 × 2).
    tw: u32,
    th: u32,
    eps_scale: f32,
    restore: f32,
    threshold: f32,
    contrast_gain: f32,
    max_ratio: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read> p0: array<f32>;
@group(0) @binding(2) var<storage, read> p1: array<f32>;
@group(0) @binding(3) var<storage, read> p2: array<f32>;
@group(0) @binding(4) var<storage, read_write> out0: array<f32>;
@group(0) @binding(5) var<storage, read_write> out1: array<f32>;
// Noise sigma, then eps, by encoded luminance: 257 values each.
@group(0) @binding(6) var<storage, read> luts: array<f32>;
@group(0) @binding(7) var<storage, read> lp: Lnr;

const CENTRE: f32 = 0.5;
const MAX_LINEAR: f32 = 1e3;
const MIN_SCALE_Y: f32 = 1e-6;
const LUT_STEPS: u32 = 256u;

fn lut(offset: u32, x: f32) -> f32 {
    let u = select(clamp(x, 0.0, 1.0), 0.0, x != x);
    let pos = u * f32(LUT_STEPS);
    let i = min(u32(pos), LUT_STEPS - 1u);
    let a = luts[offset + i];
    return a + (luts[offset + i + 1u] - a) * (pos - f32(i));
}

fn sigma_at(x: f32) -> f32 {
    return lut(0u, x);
}

fn eps_at(x: f32) -> f32 {
    return lut(LUT_STEPS + 1u, x);
}

// The sRGB transfer, odd below 0 and 0 for NaN (detail/util.rs `Transfer`).
fn encode(v: f32) -> f32 {
    if (v != v) { return 0.0; }
    if (v < 0.0) { return -srgb_encode(-v); }
    return srgb_encode(v);
}

fn decode(v: f32) -> f32 {
    if (v != v) { return 0.0; }
    if (v < 0.0) { return -srgb_decode(-v); }
    return srgb_decode(v);
}

fn dot_lum(p: vec3<f32>) -> f32 {
    return lp.lum[0] * p.x + lp.lum[1] * p.y + lp.lum[2] * p.z;
}

// out0: the centred encoded luminance.
@compute @workgroup_size(256)
fn lnr_level(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    var y = dot_lum(vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]));
    if (y != y) {
        y = 0.0;
    }
    out0[i] = encode(clamp(y, -MAX_LINEAR, MAX_LINEAR)) - CENTRE;
}

// One thread per noise tile of the level in p0: the Haar diagonal power of
// its 2 × 2 blocks and their encoded luminance, added in the CPU's order.
@compute @workgroup_size(256)
fn lnr_tiles(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let t = pixel_index(id, g);
    if (t >= lp.tw * lp.th) { return; }
    let cw = lp.w / 2u;
    let ch = lp.h / 2u;
    let bx0 = (t % lp.tw) * 8u;
    let by0 = (t / lp.tw) * 8u;
    var power = 0.0;
    var lum = 0.0;
    var lo = 3.4028235e38;
    var hi = -3.4028235e38;
    var n = 0u;
    for (var cy = by0; cy < min(by0 + 8u, ch); cy++) {
        let r0 = 2u * cy * lp.w;
        let r1 = r0 + lp.w;
        for (var cx = bx0; cx < min(bx0 + 8u, cw); cx++) {
            let a = p0[r0 + 2u * cx];
            let b = p0[r0 + 2u * cx + 1u];
            let c = p0[r1 + 2u * cx];
            let d = p0[r1 + 2u * cx + 1u];
            let hv = 0.5 * (a - b - c + d);
            power += hv * hv;
            lum += 0.25 * (a + b + c + d) + CENTRE;
            lo = min(lo, min(min(a, b), min(c, d)) + CENTRE);
            hi = max(hi, max(max(a, b), max(c, d)) + CENTRE);
            n += 1u;
        }
    }
    let o = t * 7u;
    out0[o] = power;
    out0[o + 1u] = power;
    out0[o + 2u] = power;
    out0[o + 3u] = lum;
    out0[o + 4u] = lo;
    out0[o + 5u] = hi;
    out0[o + 6u] = f32(n);
}

// The self-guided filter's coefficients from the box means of the source
// (p0) and of its square (p1): a into out0, b into out1.
@compute @workgroup_size(256)
fn lnr_coef(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let m = p0[i];
    let v = max(p1[i] - m * m, 0.0);
    let e = lp.eps_scale * eps_at(m + CENTRE);
    var a = 1.0;
    if (v + e > 0.0) {
        a = v / (v + e);
    }
    out0[i] = a;
    out1[i] = m * (1.0 - a);
}

// The filtered plane: mean a (p1) times the source (p0) plus mean b (p2).
@compute @workgroup_size(256)
fn lnr_guided(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    out0[i] = p1[i] * p0[i] + p2[i];
}

// What smoothing (p1) removed from the level (p0), and its square.
@compute @workgroup_size(256)
fn lnr_removed(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let r = p0[i] - p1[i];
    out0[i] = r;
    out1[i] = r * r;
}

// Detail: from the 3 × 3 energy (p0), the smooth level (p1) and what was
// removed (p2), the level with the kept share returned (out0) and what is
// still removed (out1).
@compute @workgroup_size(256)
fn lnr_kept(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let m = p1[i];
    let r = p2[i];
    let tau = lp.threshold * sigma_at(m + CENTRE);
    var kept = lp.restore;
    if (tau > 0.0) {
        let q = p0[i] / (tau * tau);
        kept = lp.restore * q * q / (1.0 + q * q);
    }
    out0[i] = m + r * kept;
    out1[i] = r * (1.0 - kept);
}

// The new level (p1, plus Contrast's low-pass in p2) against the old (p0),
// applied to RGB by luminance ratio.
@compute @workgroup_size(256)
fn lnr_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    var v = p1[i];
    if (lp.contrast_gain > 0.0) {
        v += lp.contrast_gain * p2[i];
    }
    let lv = p0[i];
    if (v == lv) { return; }
    var px = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let y_old = dot_lum(px);
    if (y_old != y_old || abs(y_old) > 3.4028235e38) { return; }
    let y_new = max(y_old + decode(v + CENTRE) - decode(lv + CENTRE), min(y_old, 0.0));
    if (y_old > MIN_SCALE_Y) {
        let ratio = y_new / y_old;
        if (ratio <= lp.max_ratio) {
            px *= ratio;
        } else {
            px = px * lp.max_ratio + vec3<f32>(y_new - y_old * lp.max_ratio);
        }
    } else {
        px += vec3<f32>(y_new - y_old);
    }
    img[i * 3u] = px.x;
    img[i * 3u + 1u] = px.y;
    img[i * 3u + 2u] = px.z;
}
