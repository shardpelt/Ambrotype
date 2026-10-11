// SPDX-License-Identifier: GPL-3.0-or-later
// Highlights/Shadows and Texture/Clarity on the luminance level, as
// amb-engine's local_tone.rs and local.rs do them: guided bases (A and B
// upsampled bilinearly), each band's change, and the change applied to RGB
// by a luminance ratio within the headroom.

struct Local {
    w: u32,
    h: u32,
    // The guided base's subsampling and low-resolution size; B starts at wl · hl in `ab`.
    s: u32,
    wl: u32,
    hl: u32,
    amount: f32,
    limit: f32,
    combined_limit: f32,
    lum: array<f32, 3>,
    knee: f32,
    level_bound: f32,
    // 1: add to `acc1` (removal) instead of `acc0` (boost).
    remove: u32,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read_write> lvl: array<f32>;
@group(0) @binding(2) var<storage, read> p0: array<f32>;
@group(0) @binding(3) var<storage, read> p1: array<f32>;
@group(0) @binding(4) var<storage, read> p2: array<f32>;
@group(0) @binding(5) var<storage, read_write> acc0: array<f32>;
@group(0) @binding(6) var<storage, read_write> acc1: array<f32>;
@group(0) @binding(7) var<storage, read> lp: Local;

fn is_finite(v: f32) -> bool {
    return v == v && abs(v) <= 3.4028235e38;
}

// Encoded luminance for filtering: finite and within ±level_bound.
@compute @workgroup_size(256)
fn luma_level(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let y = lp.lum[0] * img[i * 3u] + lp.lum[1] * img[i * 3u + 1u] + lp.lum[2] * img[i * 3u + 2u];
    let v = srgb_encode(y);
    if (v != v) {
        lvl[i] = 0.0;
    } else {
        lvl[i] = clamp(v, -lp.level_bound, lp.level_bound);
    }
}

// Low-resolution sample j sits at full-resolution (j + 0.5) · s − 0.5.
fn coord(x: u32, n: u32) -> vec3<f32> {
    let f = clamp((f32(x) + 0.5) / f32(lp.s) - 0.5, 0.0, f32(n - 1u));
    let i0 = min(u32(f), n - 1u);
    return vec3<f32>(f32(i0), f32(min(i0 + 1u, n - 1u)), f - f32(i0));
}

fn bilerp(buf: u32, off: u32, cx: vec3<f32>, cy: vec3<f32>) -> f32 {
    let x0 = u32(cx.x);
    let x1 = u32(cx.y);
    let r0 = off + u32(cy.x) * lp.wl;
    let r1 = off + u32(cy.y) * lp.wl;
    var a: f32; var b: f32; var c: f32; var d: f32;
    if (buf == 1u) {
        a = p1[r0 + x0]; b = p1[r0 + x1]; c = p1[r1 + x0]; d = p1[r1 + x1];
    } else {
        a = p0[r0 + x0]; b = p0[r0 + x1]; c = p0[r1 + x0]; d = p0[r1 + x1];
    }
    let top = a + (b - a) * cx.z;
    let bottom = c + (d - c) * cx.z;
    return top + (bottom - top) * cy.z;
}

// The guided base at pixel i of plane value v (A and B in `buf`).
fn base_at(buf: u32, i: u32, v: f32) -> f32 {
    let cx = coord(i % lp.w, lp.wl);
    let cy = coord(i / lp.w, lp.hl);
    let n = lp.wl * lp.hl;
    return bilerp(buf, 0u, cx, cy) * v + bilerp(buf, n, cx, cy);
}

fn lut_lookup(v: f32) -> f32 {
    // `p2` holds the table (local.rs `lookup`).
    let n = arrayLength(&p2);
    let pos = clamp(v, 0.0, 1.0) * f32(n - 1u);
    let i = min(u32(pos), n - 2u);
    return p2[i] + (p2[i + 1u] - p2[i]) * (pos - f32(i));
}

fn sat(x: f32) -> f32 {
    return x / sqrt(1.0 + x * x);
}

fn soft_limit(v: f32, c: f32) -> f32 {
    if (c > 0.0) {
        return c * sat(v / c);
    }
    return v;
}

fn hard_knee_limit(v: f32, c: f32) -> f32 {
    if (c > 0.0) {
        let x = v / c;
        let x2 = x * x;
        return v / sqrt(sqrt(1.0 + x2 * x2));
    }
    return v;
}

fn headroom_limit(l: f32, d: f32) -> f32 {
    var room = l;
    if (d > 0.0) {
        room = 1.0 - l;
    }
    if (room <= 0.0) {
        return 0.0;
    }
    return room * sat(d / room);
}

// One band's change: p0 the band's source plane, p1 its A then B, p2 the
// midtone weight table.
@compute @workgroup_size(256)
fn band(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let s = p0[i];
    let b = base_at(1u, i, s);
    let d = lp.amount * lut_lookup(b) * soft_limit(s - b, lp.limit);
    if (lp.remove == 1u) {
        acc1[i] += d;
    } else {
        acc0[i] += d;
    }
}

fn dot_lum(p: vec3<f32>) -> f32 {
    return lp.lum[0] * p.x + lp.lum[1] * p.y + lp.lum[2] * p.z;
}

// Change pixel i's encoded luminance level `lv` by `d` (local.rs).
fn change_luminance(i: u32, lv: f32, d: f32) {
    var px = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    if (d == 0.0 || abs(lv) >= lp.level_bound || !is_finite(px.x) || !is_finite(px.y) || !is_finite(px.z)) {
        return;
    }
    var y_new = srgb_decode(lv + headroom_limit(lv, d));
    let y = dot_lum(px);
    if (d > 0.0) {
        // Keep the brightest channel within the same headroom.
        let m = max(px.x, max(px.y, px.z));
        if (!(y < lp.knee || m <= y)) {
            let mc = srgb_encode(m);
            y_new = min(y_new, y * (srgb_decode(mc + headroom_limit(mc, d)) / m));
        }
    }
    if (y >= lp.knee) {
        px *= y_new / y;
    } else {
        let t = max(y / lp.knee, 0.0);
        var ratio = 1.0;
        if (y > 0.0) {
            ratio = y_new / y;
        }
        let shifted = px + vec3<f32>(y_new - y);
        px = shifted + t * (px * ratio - shifted);
    }
    img[i * 3u] = px.x;
    img[i * 3u + 1u] = px.y;
    img[i * 3u + 2u] = px.z;
}

// Texture and Clarity: the summed band changes (positive ones limited together).
@compute @workgroup_size(256)
fn local_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let d = hard_knee_limit(acc0[i], lp.combined_limit) + acc1[i];
    change_luminance(i, lvl[i], d);
}

// Highlights and Shadows: the change at the base level (p0: A then B; p2: the change table).
@compute @workgroup_size(256)
fn tone_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= lp.w * lp.h) { return; }
    let lv = lvl[i];
    let b = base_at(0u, i, lv);
    change_luminance(i, lv, lut_lookup(b));
}
