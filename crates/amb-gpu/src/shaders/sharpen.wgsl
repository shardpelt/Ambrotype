// SPDX-License-Identifier: GPL-3.0-or-later
// Sharpening (amb-engine's detail.rs is the reference): an unsharp mask on
// encoded luminance, overshoot limited to the local min/max plus a margin,
// optionally weighted by an edge mask, applied to RGB by luminance ratio.

struct Sharpen {
    w: u32,
    h: u32,
    gain: f32,
    // Halo window radius, and the blend towards radius + 1.
    win: u32,
    win_frac: f32,
    margin: f32,
    max_ratio: f32,
    // Edge mask: on (1) or off, and its smoothstep edges.
    masked: u32,
    e0: f32,
    e1: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read_write> lvl: array<f32>;
@group(0) @binding(2) var<storage, read> blur: array<f32>;
@group(0) @binding(3) var<storage, read> mask: array<f32>;
@group(0) @binding(4) var<storage, read_write> raw: array<f32>;
@group(0) @binding(5) var<storage, read> sp: Sharpen;

const MAX_LINEAR: f32 = 1e3;
const MIN_SCALE_Y: f32 = 1e-6;

// The sRGB transfer, odd below 0 and 0 for NaN (detail.rs `Transfer`).
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
    return sp.lum[0] * p.x + sp.lum[1] * p.y + sp.lum[2] * p.z;
}

@compute @workgroup_size(256)
fn sharpen_level(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= sp.w * sp.h) { return; }
    var y = dot_lum(vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]));
    if (y != y) {
        y = 0.0;
    }
    lvl[i] = encode(clamp(y, -MAX_LINEAR, MAX_LINEAR));
}

// detail.rs `smoothstep`: a step when the edges meet.
fn step_edges(e0: f32, e1: f32, x: f32) -> f32 {
    if (e1 <= e0) {
        return select(0.0, 1.0, x >= e0);
    }
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// The edge mask before its final blur, from the blurred level in `blur`.
@compute @workgroup_size(256)
fn edge_mask(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= sp.w * sp.h) { return; }
    let x = i % sp.w;
    let y = i / sp.w;
    let row = y * sp.w;
    let gx = 0.5 * (blur[row + min(x + 1u, sp.w - 1u)] - blur[row + select(x - 1u, 0u, x == 0u)]);
    let up = select(y - 1u, 0u, y == 0u) * sp.w;
    let down = min(y + 1u, sp.h - 1u) * sp.w;
    let gy = 0.5 * (blur[down + x] - blur[up + x]);
    raw[i] = step_edges(sp.e0, sp.e1, sqrt(gx * gx + gy * gy));
}

fn soft_limit(v: f32, lo: f32, hi: f32, margin: f32) -> f32 {
    if (v > hi) {
        let e = v - hi;
        return hi + e * margin / (e + margin);
    } else if (v < lo) {
        let e = lo - v;
        return lo - e * margin / (e + margin);
    }
    return v;
}

@compute @workgroup_size(256)
fn sharpen_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= sp.w * sp.h) { return; }
    let lv = lvl[i];
    var gain = sp.gain;
    if (sp.masked == 1u) {
        gain *= mask[i];
    }
    var v = lv + gain * (lv - blur[i]);
    if (v == lv) { return; }
    var px = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let y_old = dot_lum(px);
    if (y_old != y_old || abs(y_old) > 3.4028235e38) { return; }
    // The window's min and max (and the next ring's, for the fraction).
    let x = i32(i % sp.w);
    let y = i32(i / sp.w);
    let r = i32(sp.win);
    var lo = 3.4028235e38;
    var hi = -3.4028235e38;
    for (var yy = max(y - r, 0); yy <= min(y + r, i32(sp.h) - 1); yy++) {
        for (var xx = max(x - r, 0); xx <= min(x + r, i32(sp.w) - 1); xx++) {
            let s = lvl[u32(yy) * sp.w + u32(xx)];
            lo = min(lo, s);
            hi = max(hi, s);
        }
    }
    if (sp.win_frac > 0.0) {
        var olo = lo;
        var ohi = hi;
        let ro = r + 1;
        for (var yy = max(y - ro, 0); yy <= min(y + ro, i32(sp.h) - 1); yy++) {
            for (var xx = max(x - ro, 0); xx <= min(x + ro, i32(sp.w) - 1); xx++) {
                let s = lvl[u32(yy) * sp.w + u32(xx)];
                olo = min(olo, s);
                ohi = max(ohi, s);
            }
        }
        lo += (olo - lo) * sp.win_frac;
        hi += (ohi - hi) * sp.win_frac;
    }
    v = soft_limit(v, lo, hi, sp.margin);
    let y_new = max(y_old + decode(v) - decode(lv), min(y_old, 0.0));
    if (y_old > MIN_SCALE_Y) {
        let ratio = y_new / y_old;
        if (ratio <= sp.max_ratio) {
            px *= ratio;
        } else {
            px = px * sp.max_ratio + vec3<f32>(y_new - y_old * sp.max_ratio);
        }
    } else {
        px += vec3<f32>(y_new - y_old);
    }
    img[i * 3u] = px.x;
    img[i * 3u + 1u] = px.y;
    img[i * 3u + 2u] = px.z;
}
