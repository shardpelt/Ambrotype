// SPDX-License-Identifier: GPL-3.0-or-later
// Film grain (amb-engine's effects.rs is the reference): white noise from
// an integer hash of (x, y, seed), blurred at a fine and a coarse scale,
// then a midtone-weighted log-normal luminance ratio. The hash is 64-bit,
// done here on pairs of u32 (lo, hi), so the noise is the CPU's bit for bit.

struct Grain {
    w: u32,
    h: u32,
    // The blurs' reach; the noise plane is (w + 2m) × (h + 2m).
    margin: u32,
    fine_len: u32,
    coarse_len: u32,
    with_coarse: u32,
    seed_lo: u32,
    seed_hi: u32,
    coarse_weight: f32,
    inv_std: f32,
    strength: f32,
    variance: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read_write> white: array<f32>;
@group(0) @binding(2) var<storage, read_write> hfine: array<f32>;
@group(0) @binding(3) var<storage, read_write> hcoarse: array<f32>;
// The fine half kernel, then the coarse one.
@group(0) @binding(4) var<storage, read> taps: array<f32>;
@group(0) @binding(5) var<storage, read> weight: array<f32>;
@group(0) @binding(6) var<storage, read> gp: Grain;

// The full 64-bit product of two u32.
fn mul32(a: u32, b: u32) -> vec2<u32> {
    let a0 = a & 0xffffu;
    let a1 = a >> 16u;
    let b0 = b & 0xffffu;
    let b1 = b >> 16u;
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 16u) + (p01 & 0xffffu) + (p10 & 0xffffu);
    return vec2<u32>((p00 & 0xffffu) | (mid << 16u), p11 + (p01 >> 16u) + (p10 >> 16u) + (mid >> 16u));
}

// Wrapping 64-bit multiply and add.
fn mul64(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let r = mul32(a.x, b.x);
    return vec2<u32>(r.x, r.y + a.x * b.y + a.y * b.x);
}

fn add64(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let lo = a.x + b.x;
    return vec2<u32>(lo, a.y + b.y + select(0u, 1u, lo < a.x));
}

// z ^ (z >> s) for 0 < s < 32.
fn xor_shr(z: vec2<u32>, s: u32) -> vec2<u32> {
    return z ^ vec2<u32>((z.x >> s) | (z.y << (32u - s)), z.y >> s);
}

// SplitMix64's finaliser.
fn mix64(z0: vec2<u32>) -> vec2<u32> {
    var z = mul64(xor_shr(z0, 30u), vec2<u32>(0x1CE4E5B9u, 0xBF58476Du));
    z = mul64(xor_shr(z, 27u), vec2<u32>(0x133111EBu, 0x94D049BBu));
    return xor_shr(z, 31u);
}

// Uniform noise in (−1, 1) with mean 0 (effects.rs's hash_noise).
fn hash_noise(x: i32, y: i32) -> f32 {
    let key = vec2<u32>(bitcast<u32>(x), bitcast<u32>(y));
    let z = mix64(add64(mul64(key, vec2<u32>(0x7F4A7C15u, 0x9E3779B9u)), vec2<u32>(gp.seed_lo, gp.seed_hi)));
    let k = i32(z.y >> 8u);
    return f32(2 * k + 1 - (1 << 24)) * (1.0 / 16777216.0);
}

@compute @workgroup_size(256)
fn grain_white(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    let pw = gp.w + 2u * gp.margin;
    if (i >= pw * (gp.h + 2u * gp.margin)) { return; }
    let m = i32(gp.margin);
    white[i] = hash_noise(i32(i % pw) - m, i32(i / pw) - m);
}

// Horizontal blur of every padded row at the image's columns, taps summed
// in the CPU's order: the centre, then each mirrored pair.
fn blur_row(row: u32, x: u32, k0: u32, len: u32) -> f32 {
    let c = row * (gp.w + 2u * gp.margin) + gp.margin + x;
    var d = taps[k0] * white[c];
    for (var t = 1u; t < len; t++) {
        d += taps[k0 + t] * (white[c - t] + white[c + t]);
    }
    return d;
}

@compute @workgroup_size(256)
fn grain_horizontal(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= gp.w * (gp.h + 2u * gp.margin)) { return; }
    let row = i / gp.w;
    let x = i % gp.w;
    hfine[i] = blur_row(row, x, 0u, gp.fine_len);
    if (gp.with_coarse == 1u) {
        hcoarse[i] = blur_row(row, x, gp.fine_len, gp.coarse_len);
    }
}

@compute @workgroup_size(256)
fn grain_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= gp.w * gp.h) { return; }
    let w = gp.w;
    // Vertical passes: inv_std·(fine − c·coarse), scales folded into the taps.
    let c = (i / w + gp.margin) * w + i % w;
    var n = (taps[0] * gp.inv_std) * hfine[c];
    for (var t = 1u; t < gp.fine_len; t++) {
        n += (taps[t] * gp.inv_std) * (hfine[c - t * w] + hfine[c + t * w]);
    }
    if (gp.with_coarse == 1u) {
        let s = -gp.coarse_weight * gp.inv_std;
        let k0 = gp.fine_len;
        n += (taps[k0] * s) * hcoarse[c];
        for (var t = 1u; t < gp.coarse_len; t++) {
            n += (taps[k0 + t] * s) * (hcoarse[c - t * w] + hcoarse[c + t * w]);
        }
    }
    var px = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let y = gp.lum[0] * px.x + gp.lum[1] * px.y + gp.lum[2] * px.z;
    // Not above 0 also skips NaN.
    if (!(y > 0.0)) { return; }
    let tt = min(y, 1.0) * 4096.0;
    let j = u32(tt);
    let wt = weight[j] + (weight[j + 1u] - weight[j]) * (tt - f32(j));
    let a = gp.strength * wt;
    // Log-normal: the mean ratio is 1, and it is never negative.
    px *= exp(a * n - 0.5 * a * a * gp.variance);
    img[i * 3u] = px.x;
    img[i * 3u + 1u] = px.y;
    img[i * 3u + 2u] = px.z;
}
