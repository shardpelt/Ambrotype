// SPDX-License-Identifier: GPL-3.0-or-later
// Kernels for running AI models (neural networks) on the GPU: the layers
// the models we offer are made of, on contiguous f32 tensors. Every entry
// point shares one layout: the output at binding 0, up to three inputs,
// and the parameters (64 integers; floats are bit-cast) as a uniform.

@group(0) @binding(0) var<storage, read_write> out: array<f32>;
@group(0) @binding(1) var<storage, read> a: array<f32>;
@group(0) @binding(2) var<storage, read> b: array<f32>;
@group(0) @binding(3) var<storage, read> c: array<f32>;
struct Params { v: array<vec4<i32>, 16> }
@group(0) @binding(4) var<uniform> params: Params;

fn P(k: u32) -> i32 {
    return params.v[k / 4u][k % 4u];
}

fn F(k: u32) -> f32 {
    return bitcast<f32>(P(k));
}

// The thread's flat index: large dispatches wrap into a second dimension.
fn flat(id: vec3<u32>, n: vec3<u32>) -> i32 {
    return i32(id.x + id.y * n.x * 256u);
}

// out[dst + Σ i·dst_stride] = a[src + Σ ((i / div) % mod)·src_stride] over
// six axes: a slice, a transpose, a broadcast, a tile or a nearest upsample.
// P: 0 count, 1 src offset, 2 dst offset, 4..10 shape, 10..16 src strides,
// 16..22 dst strides, 22..28 div, 28..34 mod.
@compute @workgroup_size(256)
fn copy(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    var r = i;
    var s = P(1u);
    var d = P(2u);
    for (var k = 5; k >= 0; k--) {
        let u = u32(k);
        let dim = P(4u + u);
        let x = r % dim;
        r = r / dim;
        s += ((x / P(22u + u)) % P(28u + u)) * P(10u + u);
        d += x * P(16u + u);
    }
    out[d] = a[s];
}

// P: 0 count, 1 value.
@compute @workgroup_size(256)
fn fill(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    out[i] = F(1u);
}

// out = a ∘ b, broadcast. P: 0 count, 1 op, 4..10 shape, 10..16 a strides,
// 16..22 b strides.
@compute @workgroup_size(256)
fn binary(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    var r = i;
    var ia = 0;
    var ib = 0;
    for (var k = 5; k >= 0; k--) {
        let u = u32(k);
        let dim = P(4u + u);
        let x = r % dim;
        r = r / dim;
        ia += x * P(10u + u);
        ib += x * P(16u + u);
    }
    let x = a[ia];
    let y = b[ib];
    var v = 0.0;
    switch P(1u) {
        case 0: { v = x + y; }
        case 1: { v = x - y; }
        case 2: { v = x * y; }
        case 3: { v = x / y; }
        case 4: { v = max(x, y); }
        case 5: { v = min(x, y); }
        case 6: { v = pow(x, y); }
        case 7: { v = y - x; }
        default: { v = x; }
    }
    out[i] = v;
}

// Abramowitz and Stegun 7.1.26: within 1.5e-7.
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.3275911 * abs(x));
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-x * x);
    return select(-y, y, x >= 0.0);
}

// out = f(a). P: 0 count, 1 op, 2 alpha.
@compute @workgroup_size(256)
fn unary(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    let x = a[i];
    let alpha = F(2u);
    var v = x;
    switch P(1u) {
        case 0: { v = -x; }
        case 1: { v = 1.0 / (1.0 + exp(-x)); }
        case 2: { v = exp(x); }
        case 3: { v = log(x); }
        case 4: { v = sqrt(x); }
        case 5: { v = inverseSqrt(x); }
        case 6: { v = 1.0 / x; }
        case 7: { v = abs(x); }
        case 8: { v = tanh(x); }
        case 9: { v = erf(x); }
        case 10: { v = 0.5 * x * (1.0 + erf(x * 0.70710678)); }
        case 11: { v = select(alpha * x, x, x >= 0.0); }
        case 12: { v = x * x; }
        case 13: { v = round(x); }
        case 14: { v = pow(x, alpha); }
        case 15: { v = floor(x); }
        case 16: { v = ceil(x); }
        case 17: { v = sin(x); }
        case 18: { v = cos(x); }
        default: { v = x; }
    }
    out[i] = v;
}

// Tiled matrix products: 64 × 64 outputs per workgroup, 4 × 4 per thread,
// 16 of the shared axis at a time.
var<workgroup> ta: array<f32, 1024>;
var<workgroup> tb: array<f32, 1024>;

// C[m, n] = Σk A[m, k] B[k, n] for each of up to three batch axes, each
// operand addressed by strides. P: 0 M, 1 N, 2 K, 4..7 batch shape,
// 8 a offset, 9 a m, 10 a k, 12..15 a batch strides, 16 b offset, 17 b k,
// 18 b n, 20..23 b batch, 24 c offset, 25 c m, 26 c n, 28..31 c batch.
@compute @workgroup_size(256)
fn matmul(@builtin(local_invocation_index) li: u32, @builtin(workgroup_id) wg: vec3<u32>) {
    let M = P(0u);
    let N = P(1u);
    let K = P(2u);
    let z = i32(wg.z);
    let b2 = z % P(6u);
    let b1 = (z / P(6u)) % P(5u);
    let b0 = z / (P(6u) * P(5u));
    let oa = P(8u) + b0 * P(12u) + b1 * P(13u) + b2 * P(14u);
    let ob = P(16u) + b0 * P(20u) + b1 * P(21u) + b2 * P(22u);
    let oc = P(24u) + b0 * P(28u) + b1 * P(29u) + b2 * P(30u);
    let tx = i32(li % 16u);
    let ty = i32(li / 16u);
    let m0 = i32(wg.y) * 64;
    let n0 = i32(wg.x) * 64;
    // Four rows of four outputs (no arrays: some shader compilers unroll
    // indexed ones into far too much code).
    var acc0 = vec4<f32>(0.0);
    var acc1 = vec4<f32>(0.0);
    var acc2 = vec4<f32>(0.0);
    var acc3 = vec4<f32>(0.0);
    for (var k0 = 0; k0 < K; k0 += 16) {
        for (var e = 0; e < 4; e++) {
            let idx = i32(li) + e * 256;
            let j = idx % 64;
            let k = idx / 64;
            let m = m0 + j;
            let n = n0 + j;
            let kk = k0 + k;
            var va = 0.0;
            if (m < M && kk < K) { va = a[oa + m * P(9u) + kk * P(10u)]; }
            ta[k * 64 + j] = va;
            var vb = 0.0;
            if (n < N && kk < K) { vb = b[ob + kk * P(17u) + n * P(18u)]; }
            tb[k * 64 + j] = vb;
        }
        workgroupBarrier();
        for (var k = 0; k < 16; k++) {
            let i = k * 64 + ty * 4;
            let j = k * 64 + tx * 4;
            let rb = vec4<f32>(tb[j], tb[j + 1], tb[j + 2], tb[j + 3]);
            acc0 += ta[i] * rb;
            acc1 += ta[i + 1] * rb;
            acc2 += ta[i + 2] * rb;
            acc3 += ta[i + 3] * rb;
        }
        workgroupBarrier();
    }
    mm_store(m0 + ty * 4, n0 + tx * 4, oc, acc0);
    mm_store(m0 + ty * 4 + 1, n0 + tx * 4, oc, acc1);
    mm_store(m0 + ty * 4 + 2, n0 + tx * 4, oc, acc2);
    mm_store(m0 + ty * 4 + 3, n0 + tx * 4, oc, acc3);
}

// Four outputs of row `m` from column `n`, those inside the matrix.
fn mm_store(m: i32, n: i32, oc: i32, v: vec4<f32>) {
    if (m >= P(0u)) { return; }
    let at = oc + m * P(25u);
    let s = P(26u);
    let N = P(1u);
    if (n < N) { out[at + n * s] = v.x; }
    if (n + 1 < N) { out[at + (n + 1) * s] = v.y; }
    if (n + 2 < N) { out[at + (n + 2) * s] = v.z; }
    if (n + 3 < N) { out[at + (n + 3) * s] = v.w; }
}

// Convolution as a matrix product, per group (the z of the dispatch): rows
// are output channels, columns output pixels (n, y, x), the shared axis the
// kernel's taps (c, ky, kx), read straight from the input. a: input NCHW,
// b: kernel OIHW, c: bias.
// P: 0 batch, 1 C, 2 H, 3 W, 4 O, 5 OH, 6 OW, 7 KH, 8 KW, 9 stride y,
// 10 stride x, 11 pad y, 12 pad x, 13 dilation y, 14 dilation x,
// 16 Cg (input channels per group), 17 Og, 18 has bias.
@compute @workgroup_size(256)
fn conv(@builtin(local_invocation_index) li: u32, @builtin(workgroup_id) wg: vec3<u32>) {
    let C = P(1u);
    let H = P(2u);
    let W = P(3u);
    let O = P(4u);
    let OH = P(5u);
    let OW = P(6u);
    let KH = P(7u);
    let KW = P(8u);
    let Cg = P(16u);
    let Og = P(17u);
    let g = i32(wg.z);
    let M = Og;
    let N = P(0u) * OH * OW;
    let K = Cg * KH * KW;
    let tx = i32(li % 16u);
    let ty = i32(li / 16u);
    let m0 = i32(wg.y) * 64;
    let n0 = i32(wg.x) * 64;
    // Four rows of four outputs (no arrays: some shader compilers unroll
    // indexed ones into far too much code).
    var acc0 = vec4<f32>(0.0);
    var acc1 = vec4<f32>(0.0);
    var acc2 = vec4<f32>(0.0);
    var acc3 = vec4<f32>(0.0);
    for (var k0 = 0; k0 < K; k0 += 16) {
        for (var e = 0; e < 4; e++) {
            let idx = i32(li) + e * 256;
            let j = idx % 64;
            let k = idx / 64;
            let m = m0 + j;
            let col = n0 + j;
            let kk = k0 + k;
            var va = 0.0;
            if (m < M && kk < K) { va = b[(g * Og + m) * K + kk]; }
            ta[k * 64 + j] = va;
            var vb = 0.0;
            if (col < N && kk < K) {
                let kx = kk % KW;
                let ky = (kk / KW) % KH;
                let ci = kk / (KW * KH);
                let ox = col % OW;
                let oy = (col / OW) % OH;
                let n = col / (OW * OH);
                let iy = oy * P(9u) - P(11u) + ky * P(13u);
                let ix = ox * P(10u) - P(12u) + kx * P(14u);
                if (iy >= 0 && iy < H && ix >= 0 && ix < W) {
                    vb = a[((n * C + g * Cg + ci) * H + iy) * W + ix];
                }
            }
            tb[k * 64 + j] = vb;
        }
        workgroupBarrier();
        for (var k = 0; k < 16; k++) {
            let i = k * 64 + ty * 4;
            let j = k * 64 + tx * 4;
            let rb = vec4<f32>(tb[j], tb[j + 1], tb[j + 2], tb[j + 3]);
            acc0 += ta[i] * rb;
            acc1 += ta[i + 1] * rb;
            acc2 += ta[i + 2] * rb;
            acc3 += ta[i + 3] * rb;
        }
        workgroupBarrier();
    }
    conv_store(m0 + ty * 4, n0 + tx * 4, g, acc0);
    conv_store(m0 + ty * 4 + 1, n0 + tx * 4, g, acc1);
    conv_store(m0 + ty * 4 + 2, n0 + tx * 4, g, acc2);
    conv_store(m0 + ty * 4 + 3, n0 + tx * 4, g, acc3);
}

// One output pixel of channel `m` (in group `g`), with its bias.
fn conv_one(m: i32, col: i32, g: i32, v: f32) {
    let OH = P(5u);
    let OW = P(6u);
    if (col >= P(0u) * OH * OW) { return; }
    let o = g * P(17u) + m;
    var bias = 0.0;
    if (P(18u) != 0) { bias = c[o]; }
    let ox = col % OW;
    let oy = (col / OW) % OH;
    let n = col / (OW * OH);
    out[((n * P(4u) + o) * OH + oy) * OW + ox] = v + bias;
}

fn conv_store(m: i32, col: i32, g: i32, v: vec4<f32>) {
    if (m >= P(17u)) { return; }
    conv_one(m, col, g, v.x);
    conv_one(m, col + 1, g, v.y);
    conv_one(m, col + 2, g, v.z);
    conv_one(m, col + 3, g, v.w);
}

// Convolution one output at a time, for few output channels per group
// (depthwise convolutions). Parameters as `conv`; P(0) here is the count.
@compute @workgroup_size(256)
fn conv_direct(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    let C = P(1u);
    let H = P(2u);
    let W = P(3u);
    let O = P(4u);
    let OH = P(5u);
    let OW = P(6u);
    let KH = P(7u);
    let KW = P(8u);
    let Cg = P(16u);
    let Og = P(17u);
    if (i >= P(19u)) { return; }
    let ox = i % OW;
    let oy = (i / OW) % OH;
    let o = (i / (OW * OH)) % O;
    let n = i / (OW * OH * O);
    let g = o / Og;
    var sum = 0.0;
    if (P(18u) != 0) { sum = c[o]; }
    for (var ci = 0; ci < Cg; ci++) {
        for (var ky = 0; ky < KH; ky++) {
            let iy = oy * P(9u) - P(11u) + ky * P(13u);
            if (iy < 0 || iy >= H) { continue; }
            for (var kx = 0; kx < KW; kx++) {
                let ix = ox * P(10u) - P(12u) + kx * P(14u);
                if (ix < 0 || ix >= W) { continue; }
                sum += a[((n * C + g * Cg + ci) * H + iy) * W + ix] * b[((o * Cg + ci) * KH + ky) * KW + kx];
            }
        }
    }
    out[i] = sum;
}

// Transposed convolution, one output at a time: each output gathers the
// inputs whose taps land on it. a: input NCHW, b: kernel addressed by
// strides, c: bias. P as `conv`, with 19 count, 20..24 kernel strides
// (input channel, output channel in its group, ky, kx).
@compute @workgroup_size(256)
fn deconv(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(19u)) { return; }
    let C = P(1u);
    let H = P(2u);
    let W = P(3u);
    let O = P(4u);
    let OH = P(5u);
    let OW = P(6u);
    let KH = P(7u);
    let KW = P(8u);
    let sy = P(9u);
    let sx = P(10u);
    let Cg = P(16u);
    let Og = P(17u);
    let ox = i % OW;
    let oy = (i / OW) % OH;
    let o = (i / (OW * OH)) % O;
    let n = i / (OW * OH * O);
    let g = o / Og;
    let og = o % Og;
    var sum = 0.0;
    if (P(18u) != 0) { sum = c[o]; }
    for (var ky = 0; ky < KH; ky++) {
        let ty = oy + P(11u) - ky * P(13u);
        if (ty < 0 || ty % sy != 0) { continue; }
        let iy = ty / sy;
        if (iy >= H) { continue; }
        for (var kx = 0; kx < KW; kx++) {
            let tx = ox + P(12u) - kx * P(14u);
            if (tx < 0 || tx % sx != 0) { continue; }
            let ix = tx / sx;
            if (ix >= W) { continue; }
            for (var ci = 0; ci < Cg; ci++) {
                let ch = g * Cg + ci;
                sum += a[((n * C + ch) * H + iy) * W + ix] * b[ch * P(20u) + og * P(21u) + ky * P(22u) + kx * P(23u)];
            }
        }
    }
    out[i] = sum;
}

// Padding over six axes. P: 0 count, 1 mode (0 constant, 1 reflect,
// 2 edge), 2 value, 4..10 output shape, 10..16 input shape, 16..22 before.
@compute @workgroup_size(256)
fn pad(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    var r = i;
    var s = 0;
    var stride = 1;
    var outside = false;
    for (var k = 5; k >= 0; k--) {
        let u = u32(k);
        let dim = P(4u + u);
        let len = P(10u + u);
        var x = r % dim - P(16u + u);
        r = r / dim;
        if (x < 0 || x >= len) {
            switch P(1u) {
                case 1: {
                    let period = max(2 * (len - 1), 1);
                    x = abs(x) % period;
                    if (x >= len) { x = period - x; }
                }
                case 2: { x = clamp(x, 0, len - 1); }
                default: { outside = true; }
            }
        }
        s += x * stride;
        stride *= len;
    }
    if (outside) { out[i] = F(2u); } else { out[i] = a[s]; }
}

// Reduce the middle of (outer, len, inner). P: 0 count (outer · inner),
// 1 op (0 sum, 1 mean, 2 max, 3 min, 4 product, 5 mean of squares),
// 2 outer, 3 len, 4 inner.
@compute @workgroup_size(256)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    let len = P(3u);
    let inner = P(4u);
    let base = (i / inner) * len * inner + i % inner;
    let op = P(1u);
    var v = a[base];
    if (op == 5) { v = v * v; }
    for (var j = 1; j < len; j++) {
        let x = a[base + j * inner];
        switch op {
            case 2: { v = max(v, x); }
            case 3: { v = min(v, x); }
            case 4: { v = v * x; }
            case 5: { v += x * x; }
            default: { v += x; }
        }
    }
    if (op == 1 || op == 5) { v = v / f32(len); }
    out[i] = v;
}

// Softmax (or its log) along the middle of (outer, len, inner). P as
// `reduce`, op 1 for the log.
@compute @workgroup_size(256)
fn softmax(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    let len = P(3u);
    let inner = P(4u);
    let base = (i / inner) * len * inner + i % inner;
    var top = a[base];
    for (var j = 1; j < len; j++) { top = max(top, a[base + j * inner]); }
    var sum = 0.0;
    for (var j = 0; j < len; j++) { sum += exp(a[base + j * inner] - top); }
    for (var j = 0; j < len; j++) {
        let x = a[base + j * inner] - top;
        if (P(1u) == 1) { out[base + j * inner] = x - log(sum); } else { out[base + j * inner] = exp(x) / sum; }
    }
}

// x / √(mean(x²) + eps) along the middle of (outer, len, inner). P as
// `reduce`, 5 eps.
@compute @workgroup_size(256)
fn rms_norm(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    let len = P(3u);
    let inner = P(4u);
    let base = (i / inner) * len * inner + i % inner;
    var sum = 0.0;
    for (var j = 0; j < len; j++) {
        let x = a[base + j * inner];
        sum += x * x;
    }
    let s = inverseSqrt(sum / f32(len) + F(5u));
    for (var j = 0; j < len; j++) { out[base + j * inner] = a[base + j * inner] * s; }
}

// Max pooling, NCHW. P as `conv` (no kernel or bias), 19 count.
@compute @workgroup_size(256)
fn max_pool(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(19u)) { return; }
    let C = P(1u);
    let H = P(2u);
    let W = P(3u);
    let OH = P(5u);
    let OW = P(6u);
    let ox = i % OW;
    let oy = (i / OW) % OH;
    let ch = (i / (OW * OH)) % C;
    let n = i / (OW * OH * C);
    var v = -3.4e38;
    for (var ky = 0; ky < P(7u); ky++) {
        let iy = oy * P(9u) - P(11u) + ky * P(13u);
        if (iy < 0 || iy >= H) { continue; }
        for (var kx = 0; kx < P(8u); kx++) {
            let ix = ox * P(10u) - P(12u) + kx * P(14u);
            if (ix < 0 || ix >= W) { continue; }
            v = max(v, a[((n * C + ch) * H + iy) * W + ix]);
        }
    }
    out[i] = v;
}

// out[o, j, i] = a[o, b[j], i]: b holds the indices (bit-cast integers).
// P: 0 count, 2 outer, 3 indices, 4 inner, 5 the axis's length.
@compute @workgroup_size(256)
fn gather(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {
    let i = flat(id, nw);
    if (i >= P(0u)) { return; }
    let inner = P(4u);
    let count = P(3u);
    let len = P(5u);
    let x = i % inner;
    let j = (i / inner) % count;
    let o = i / (inner * count);
    var at = bitcast<i32>(b[j]);
    if (at < 0) { at += len; }
    out[i] = a[(o * len + at) * inner + x];
}
