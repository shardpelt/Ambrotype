// SPDX-License-Identifier: GPL-3.0-or-later
// Single-channel plane filters, as amb-engine's filters.rs and local.rs do
// them: Gaussian kernels and box blurs (edges clamped), fractional box
// blurs, block downsampling and the guided filter's coefficients. Each output
// sums its own window, where the CPU keeps running sums; the results differ
// only in rounding.

struct Plane {
    // Input size (output size for downsample: wl × hl).
    w: u32,
    h: u32,
    // Box radius (integer part), fractional part and 1 / window width.
    n: u32,
    frac: f32,
    norm: f32,
    // Kernel length (conv), subsampling factor (downsample), eps (coef).
    klen: u32,
    s: u32,
    eps: f32,
    // Where the output starts in `out0` (lets two planes share a buffer).
    out_offset: u32,
    in_offset: u32,
}

@group(0) @binding(0) var<storage, read> in0: array<f32>;
@group(0) @binding(1) var<storage, read> in1: array<f32>;
@group(0) @binding(2) var<storage, read_write> out0: array<f32>;
@group(0) @binding(3) var<storage, read_write> out1: array<f32>;
@group(0) @binding(4) var<storage, read> pp: Plane;

fn at(x: i32, y: i32) -> f32 {
    let cx = clamp(x, 0, i32(pp.w) - 1);
    let cy = clamp(y, 0, i32(pp.h) - 1);
    return in0[pp.in_offset + u32(cy) * pp.w + u32(cx)];
}

fn xy(i: u32) -> vec2<i32> {
    return vec2<i32>(i32(i % pp.w), i32(i / pp.w));
}

@compute @workgroup_size(256)
fn conv_h(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let p = xy(i);
    let r = i32(pp.klen / 2u);
    var acc = 0.0;
    for (var j = 0; j < i32(pp.klen); j++) {
        acc += in1[j] * at(p.x + j - r, p.y);
    }
    out0[pp.out_offset + i] = acc;
}

@compute @workgroup_size(256)
fn conv_v(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let p = xy(i);
    let r = i32(pp.klen / 2u);
    var acc = 0.0;
    for (var j = 0; j < i32(pp.klen); j++) {
        acc += in1[j] * at(p.x, p.y + j - r);
    }
    out0[pp.out_offset + i] = acc;
}

// A box of radius n plus end taps weighted by `frac` (0 for an integer box).
@compute @workgroup_size(256)
fn box_h(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let p = xy(i);
    let n = i32(pp.n);
    var sum = 0.0;
    for (var j = -n; j <= n; j++) {
        sum += at(p.x + j, p.y);
    }
    if (pp.frac > 0.0) {
        sum += pp.frac * (at(p.x - n - 1, p.y) + at(p.x + n + 1, p.y));
    }
    out0[pp.out_offset + i] = sum * pp.norm;
}

@compute @workgroup_size(256)
fn box_v(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let p = xy(i);
    let n = i32(pp.n);
    var sum = 0.0;
    for (var j = -n; j <= n; j++) {
        sum += at(p.x, p.y + j);
    }
    if (pp.frac > 0.0) {
        sum += pp.frac * (at(p.x, p.y - n - 1) + at(p.x, p.y + n + 1));
    }
    out0[pp.out_offset + i] = sum * pp.norm;
}

// Box-average s × s blocks of the w × h input into wl × hl (partial blocks
// at the right and bottom average what they have).
@compute @workgroup_size(256)
fn downsample(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    let wl = (pp.w + pp.s - 1u) / pp.s;
    let hl = (pp.h + pp.s - 1u) / pp.s;
    if (i >= wl * hl) { return; }
    let bx = i % wl;
    let by = i / wl;
    let x0 = bx * pp.s;
    let x1 = min((bx + 1u) * pp.s, pp.w);
    let y0 = by * pp.s;
    let y1 = min((by + 1u) * pp.s, pp.h);
    var sum = 0.0;
    for (var y = y0; y < y1; y++) {
        var row = 0.0;
        for (var x = x0; x < x1; x++) {
            row += in0[pp.in_offset + y * pp.w + x];
        }
        sum += row;
    }
    out0[pp.out_offset + i] = sum / f32((x1 - x0) * (y1 - y0));
}

@compute @workgroup_size(256)
fn square(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let v = in0[pp.in_offset + i];
    out0[pp.out_offset + i] = v * v;
}

// The guided filter's coefficients from the box means of x and x².
@compute @workgroup_size(256)
fn guided_coef(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= pp.w * pp.h) { return; }
    let m = in0[i];
    let c = in1[i];
    let v = max(c - m * m, 0.0);
    let a = v / (v + pp.eps);
    out0[i] = a;
    out1[i] = m - a * m;
}
