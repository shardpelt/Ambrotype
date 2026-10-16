// SPDX-License-Identifier: GPL-3.0-or-later
// Colour noise reduction (amb-engine's detail/ is the reference). The
// low-resolution guided filter and the noise statistics run on the CPU;
// here: the downscaled copy they start from, the chroma residual against the
// smoothed chroma, the per-tile sums the noise is measured from, and the
// garrote that keeps residuals that look like detail.

struct Cnr {
    w: u32,
    h: u32,
    factor: u32,
    sw: u32,
    sh: u32,
    // 16-pixel tiles across and down.
    tw: u32,
    th: u32,
    mix: f32,
    // Detail on (1): the garrote with threshold table `tau`.
    detail: u32,
    // Garrote: 2 → q, 4 → q², else q^half.
    garrote: u32,
    half: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read_write> small: array<f32>;
// Six sw × sh grids: a for R, G, B, then b for R, G, B.
@group(0) @binding(2) var<storage, read> coefs: array<f32>;
@group(0) @binding(3) var<storage, read_write> energy: array<f32>;
// Per pixel: encoded luminance, its chroma (three), and whether it has a residual.
@group(0) @binding(4) var<storage, read_write> lc: array<f32>;
// Per tile: residual-energy sums then directional chroma sums, 7 values each.
@group(0) @binding(5) var<storage, read_write> tiles: array<f32>;
@group(0) @binding(6) var<storage, read> tau: array<f32>;
@group(0) @binding(7) var<storage, read> cp: Cnr;

const CENTRE: f32 = 0.5;
const MIN_SCALE_Y: f32 = 1e-6;
const MAX_LINEAR: f32 = 1e3;
const TILE: u32 = 16u;

fn finite(v: f32) -> bool {
    return v == v && abs(v) <= 3.4028235e38;
}

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
    return cp.lum[0] * p.x + cp.lum[1] * p.y + cp.lum[2] * p.z;
}

fn pixel(i: u32) -> vec3<f32> {
    return vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
}

// Box-average f × f blocks, leaving out non-finite pixels, channels clamped.
@compute @workgroup_size(256)
fn cnr_downscale(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= cp.sw * cp.sh) { return; }
    let bx = i % cp.sw;
    let by = i / cp.sw;
    let f = cp.factor;
    var acc = vec3<f32>(0.0);
    var n = 0u;
    for (var y = by * f; y < min((by + 1u) * f, cp.h); y++) {
        for (var x = bx * f; x < min((bx + 1u) * f, cp.w); x++) {
            let p = pixel(y * cp.w + x);
            if (finite(p.x) && finite(p.y) && finite(p.z)) {
                acc += clamp(p, vec3<f32>(-MAX_LINEAR), vec3<f32>(MAX_LINEAR));
                n += 1u;
            }
        }
    }
    if (n > 0u) {
        acc /= f32(n);
    }
    small[i * 3u] = acc.x;
    small[i * 3u + 1u] = acc.y;
    small[i * 3u + 2u] = acc.z;
}

fn lerp_pos(i: u32, n: u32) -> vec3<f32> {
    let s = clamp((f32(i) + 0.5) / f32(cp.factor) - 0.5, 0.0, f32(n - 1u));
    let lo = min(u32(floor(s)), n - 1u);
    return vec3<f32>(f32(lo), f32(min(lo + 1u, n - 1u)), s - f32(lo));
}

fn grid_at(grid: u32, cx: vec3<f32>, cy: vec3<f32>) -> f32 {
    let base = grid * cp.sw * cp.sh;
    let r0 = base + u32(cy.x) * cp.sw;
    let r1 = base + u32(cy.y) * cp.sw;
    // Vertically first, then horizontally (CoefGrids).
    let c0 = coefs[r0 + u32(cx.x)] + (coefs[r1 + u32(cx.x)] - coefs[r0 + u32(cx.x)]) * cy.z;
    let c1 = coefs[r0 + u32(cx.y)] + (coefs[r1 + u32(cx.y)] - coefs[r0 + u32(cx.y)]) * cy.z;
    return c0 + (c1 - c0) * cx.z;
}

struct Residual {
    ok: bool,
    y: f32,
    le: f32,
    e: vec3<f32>,
    r: vec3<f32>,
}

fn residual(i: u32) -> Residual {
    var out: Residual;
    let px = pixel(i);
    let y = dot_lum(px);
    out.ok = y > MIN_SCALE_Y && finite(y) && finite(px.x) && finite(px.y) && finite(px.z);
    if (!out.ok) { return out; }
    let cx = lerp_pos(i % cp.w, cp.sw);
    let cy = lerp_pos(i / cp.w, cp.sh);
    out.y = y;
    out.le = encode(y);
    let l = out.le - CENTRE;
    out.e = vec3<f32>(encode(px.x), encode(px.y), encode(px.z));
    for (var c = 0u; c < 3u; c++) {
        out.r[c] = out.e[c] - out.le - (grid_at(c, cx, cy) * l + grid_at(3u + c, cx, cy));
    }
    return out;
}

// The residual energy |r|² per pixel, and what the tile sums need.
@compute @workgroup_size(256)
fn cnr_residual(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= cp.w * cp.h) { return; }
    let c = residual(i);
    if (c.ok) {
        energy[i] = dot(c.r, c.r);
        lc[i * 5u] = c.le;
        lc[i * 5u + 1u] = c.e.x - c.le;
        lc[i * 5u + 2u] = c.e.y - c.le;
        lc[i * 5u + 3u] = c.e.z - c.le;
        lc[i * 5u + 4u] = 1.0;
    } else {
        energy[i] = 0.0;
        var y = dot_lum(pixel(i));
        if (y != y) { y = 0.0; }
        lc[i * 5u] = encode(clamp(y, -MAX_LINEAR, MAX_LINEAR));
        lc[i * 5u + 1u] = 0.0;
        lc[i * 5u + 2u] = 0.0;
        lc[i * 5u + 3u] = 0.0;
        lc[i * 5u + 4u] = 0.0;
    }
}

fn haar(v: vec4<f32>) -> vec3<f32> {
    let hv = 0.5 * (v.x - v.y - v.z + v.w);
    let d1 = v.y - v.z;
    let d2 = v.x - v.w;
    return vec3<f32>(hv * hv, 0.5 * d1 * d1, 0.5 * d2 * d2);
}

fn lc_block(k: u32, y: u32, x: u32) -> vec4<f32> {
    let a = y * cp.w + x;
    let b = (y + 1u) * cp.w + x;
    return vec4<f32>(lc[a * 5u + k], lc[(a + 1u) * 5u + k], lc[b * 5u + k], lc[(b + 1u) * 5u + k]);
}

// One thread per 16 × 16 tile, adding in the CPU's order.
@compute @workgroup_size(256)
fn cnr_tiles(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let t = pixel_index(id, g);
    if (t >= cp.tw * cp.th) { return; }
    let tx = t % cp.tw;
    let ty = t / cp.tw;
    let y0 = ty * TILE;
    let y1 = min(y0 + TILE, cp.h);
    let x0 = tx * TILE;
    let x1 = min(x0 + TILE, cp.w);
    // Residual energy: [energy; 3] with the pixel's own level as mean, low and high.
    var power = 0.0;
    var lum = 0.0;
    var lo = 3.4028235e38;
    var hi = -3.4028235e38;
    var n = 0u;
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            let i = y * cp.w + x;
            if (lc[i * 5u + 4u] == 1.0) {
                let le = lc[i * 5u];
                power += energy[i];
                lum += le;
                lo = min(lo, le);
                hi = max(hi, le);
                n += 1u;
            }
        }
    }
    let o = t * 14u;
    tiles[o] = power;
    tiles[o + 1u] = power;
    tiles[o + 2u] = power;
    tiles[o + 3u] = lum;
    tiles[o + 4u] = lo;
    tiles[o + 5u] = hi;
    tiles[o + 6u] = f32(n);
    // Directional chroma detail of 2 × 2 blocks: the row pairs of the tile,
    // the column pairs of the image's even width.
    var hp = vec3<f32>(0.0);
    var hlum = 0.0;
    var hlo = 3.4028235e38;
    var hhi = -3.4028235e38;
    var hn = 0u;
    let pairs_x1 = min(x1, (cp.w / 2u) * 2u);
    for (var y = y0; y + 1u < y1; y += 2u) {
        for (var x = x0; x + 1u < pairs_x1 + 1u && x < pairs_x1; x += 2u) {
            var p = vec3<f32>(0.0);
            for (var k = 1u; k < 4u; k++) {
                p += haar(lc_block(k, y, x));
            }
            let l = lc_block(0u, y, x);
            hp += p;
            hlum += 0.25 * (l.x + l.y + l.z + l.w);
            hlo = min(hlo, min(min(l.x, l.y), min(l.z, l.w)));
            hhi = max(hhi, max(max(l.x, l.y), max(l.z, l.w)));
            hn += 1u;
        }
    }
    tiles[o + 7u] = hp.x;
    tiles[o + 8u] = hp.y;
    tiles[o + 9u] = hp.z;
    tiles[o + 10u] = hlum;
    tiles[o + 11u] = hlo;
    tiles[o + 12u] = hhi;
    tiles[o + 13u] = f32(hn);
}

fn tau_at(v0: f32) -> f32 {
    var v = v0;
    if (v != v) { v = 0.0; }
    v = clamp(v, 0.0, 1.0);
    let steps = arrayLength(&tau) - 1u;
    let pos = v * f32(steps);
    let i = min(u32(pos), steps - 1u);
    return tau[i] + (tau[i + 1u] - tau[i]) * (pos - f32(i));
}

fn keep(q: f32) -> f32 {
    if (q != q || q >= 1.0) { return 0.0; }
    if (cp.garrote == 2u) { return 1.0 - q; }
    if (cp.garrote == 4u) { return 1.0 - q * q; }
    return 1.0 - pow(q, cp.half);
}

@compute @workgroup_size(256)
fn cnr_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= cp.w * cp.h) { return; }
    let c = residual(i);
    if (!c.ok) { return; }
    let r2 = dot(c.r, c.r);
    var k = 0.0;
    if (cp.detail == 1u) {
        // The mean energy of the 8 neighbours (edges clamped).
        let x = i % cp.w;
        let y = i / cp.w;
        let xm = select(x - 1u, 0u, x == 0u);
        let xp = min(x + 1u, cp.w - 1u);
        let up = select(y - 1u, 0u, y == 0u) * cp.w;
        let mid = y * cp.w;
        let down = min(y + 1u, cp.h - 1u) * cp.w;
        let ne = (energy[up + xm] + energy[up + x] + energy[up + xp] + energy[mid + xm]
            + energy[mid + xp] + energy[down + xm] + energy[down + x] + energy[down + xp]) / 8.0;
        let tv = tau_at(c.le);
        k = keep(tv * tv / ne);
    }
    let f = cp.mix * (k - 1.0);
    if (f == 0.0 || r2 == 0.0) { return; }
    var out: vec3<f32>;
    for (var ch = 0u; ch < 3u; ch++) {
        // Never push a channel below zero that was not negative already.
        out[ch] = decode(max(c.e[ch] + f * c.r[ch], min(c.e[ch], 0.0)));
    }
    let y_new = dot_lum(out);
    if (y_new > MIN_SCALE_Y) {
        let s = c.y / y_new;
        img[i * 3u] = out.x * s;
        img[i * 3u + 1u] = out.y * s;
        img[i * 3u + 2u] = out.z * s;
    }
}
