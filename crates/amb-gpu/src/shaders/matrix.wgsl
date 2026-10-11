// SPDX-License-Identifier: GPL-3.0-or-later
// Stage 1: camera RGB to scene-linear ProPhoto with white balance, Exposure,
// highlight recovery and the Calibration panel's shadow tint
// (`develop` and `Unclip` in amb-engine's pipeline.rs).

struct Matrix {
    m: array<f32, 9>,
    gain: f32,
    // Highlight recovery: on (1) or off, clip levels, white-balance gains, reach.
    unclip: u32,
    clip: array<f32, 3>,
    gains: array<f32, 3>,
    reach: u32,
    // The Calibration panel's shadow tint: on (1) or off, stops, end.
    tint: u32,
    tint_stops: f32,
    tint_end: f32,
}

@group(0) @binding(0) var<storage, read> camera: array<f32>;
@group(0) @binding(1) var<storage, read_write> img: array<f32>;
@group(0) @binding(2) var<storage, read_write> weights: array<f32>;
@group(0) @binding(3) var<storage, read> size: Size;
@group(0) @binding(4) var<storage, read> mp: Matrix;
// Unused here (the layout is shared).
@group(0) @binding(5) var<storage, read> luts: array<f32>;

// How clipped each channel is: 0 below 90% of its clip level, 1 from 98%.
fn clip_weight(v: f32, k: f32) -> f32 {
    if (k != k || abs(k) > 3.0e38) {
        return 0.0;
    }
    let t = clamp((v - 0.9 * k) / (0.08 * k), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@compute @workgroup_size(256)
fn unclip_weights(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels) {
        return;
    }
    for (var c = 0u; c < 3u; c++) {
        weights[i * 3u + c] = clip_weight(camera[i * 3u + c], mp.clip[c]);
    }
}

@compute @workgroup_size(256)
fn matrix(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels) {
        return;
    }
    var c = vec3<f32>(camera[i * 3u], camera[i * 3u + 1u], camera[i * 3u + 2u]);
    if (mp.unclip == 1u) {
        // Each channel: the most clipped within `reach` pixels.
        let x = i32(i % size.width);
        let y = i32(i / size.width);
        let r = i32(mp.reach);
        var w = vec3<f32>(0.0);
        for (var sy = max(y - r, 0); sy <= min(y + r, i32(size.height) - 1); sy++) {
            for (var sx = max(x - r, 0); sx <= min(x + r, i32(size.width) - 1); sx++) {
                let j = u32(sy) * size.width + u32(sx);
                w = max(w, vec3<f32>(weights[j * 3u], weights[j * 3u + 1u], weights[j * 3u + 2u]));
            }
        }
        if (any(w != vec3<f32>(0.0))) {
            let gains = vec3<f32>(mp.gains[0], mp.gains[1], mp.gains[2]);
            let b = c * gains;
            let top = max(b.x, max(b.y, b.z));
            c = (b + w * (top - b)) / gains;
        }
    }
    var o: array<f32, 3>;
    for (var r = 0u; r < 3u; r++) {
        o[r] = (mp.m[r * 3u] * c.x + mp.m[r * 3u + 1u] * c.y + mp.m[r * 3u + 2u] * c.z) * mp.gain;
    }
    if (mp.tint == 1u) {
        let y = max(0.288 * o[0] + 0.712 * o[1], 0.0);
        let t = min(y / mp.tint_end, 1.0);
        let w = 1.0 - t * t * (3.0 - 2.0 * t);
        if (w > 0.0) {
            let gg = exp2(-mp.tint_stops * w);
            let rb = 1.0 / sqrt(gg);
            o[0] *= rb;
            o[1] *= gg;
            o[2] *= rb;
        }
    }
    for (var k = 0u; k < 3u; k++) {
        img[i * 3u + k] = o[k];
    }
}
