// SPDX-License-Identifier: GPL-3.0-or-later
// Stage 5's colour controls: Vibrance, Saturation, the HSL / Color Mixer
// bands, black & white, and Color Grading, in OkLab (`ColorOp` in
// amb-engine's color.rs is the reference).

struct Color {
    to_lms: array<f32, 9>,
    from_lms: array<f32, 9>,
    m2: array<f32, 9>,
    m2_inv: array<f32, 9>,
    // Strengths: HSL hue (degrees), HSL luminance, vibrance, grade tint, grade lum.
    k: array<f32, 5>,
    vibrance: f32,
    saturation: f32,
    // Band centres (radians, ascending) and their hue, sat, lum, mix amounts.
    band_hue: array<f32, 8>,
    band_amount: array<f32, 32>,
    grayscale: u32,
    orange_index: u32,
    grade_dirs: array<f32, 8>,
    grade_amounts: array<f32, 8>,
    pivot: f32,
    width: f32,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read> size: Size;
@group(0) @binding(2) var<storage, read> cp: Color;
@group(0) @binding(3) var<storage, read> luts: array<f32>;

const TAU: f32 = 6.28318530717958647692;

fn mat(which: u32, v: vec3<f32>) -> vec3<f32> {
    var m: array<f32, 9>;
    switch which {
        case 0u: { m = cp.to_lms; }
        case 1u: { m = cp.from_lms; }
        case 2u: { m = cp.m2; }
        default: { m = cp.m2_inv; }
    }
    return vec3<f32>(
        m[0] * v.x + m[1] * v.y + m[2] * v.z,
        m[3] * v.x + m[4] * v.y + m[5] * v.z,
        m[6] * v.x + m[7] * v.y + m[8] * v.z,
    );
}

fn rem_tau(x: f32) -> f32 {
    return x - TAU * floor(x / TAU);
}

// The two bands around `hue`: (index, weight) pairs as (i0, i1, w0, w1).
fn band_weights(hue: f32) -> vec4<f32> {
    let h = rem_tau(hue);
    for (var i = 0u; i < 8u; i++) {
        let lo = cp.band_hue[i];
        var hi: f32;
        if (i + 1u < 8u) {
            hi = cp.band_hue[i + 1u];
        } else {
            hi = cp.band_hue[0] + TAU;
        }
        var hh = h;
        if (h < lo) {
            hh = h + TAU;
        }
        if (hh >= lo && hh < hi) {
            let t = smoothstep_(0.0, 1.0, (hh - lo) / (hi - lo));
            return vec4<f32>(f32(i), f32((i + 1u) % 8u), 1.0 - t, t);
        }
    }
    return vec4<f32>(0.0, 0.0, 1.0, 0.0);
}

fn amount(band: u32, which: u32) -> f32 {
    return cp.band_amount[band * 4u + which];
}

fn grade(l0: f32, a0: f32, b0: f32) -> vec3<f32> {
    var l = l0;
    var a = a0;
    var b = b0;
    let t = smoothstep_(cp.pivot - cp.width, cp.pivot + cp.width, l);
    let region = array<f32, 4>((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t, 1.0);
    for (var r = 0u; r < 4u; r++) {
        let sat = cp.grade_amounts[r * 2u];
        let lum = cp.grade_amounts[r * 2u + 1u];
        if (sat == 0.0 && lum == 0.0) {
            continue;
        }
        let w = region[r];
        a += w * sat * cp.k[3] * cp.grade_dirs[r * 2u];
        b += w * sat * cp.k[3] * cp.grade_dirs[r * 2u + 1u];
        l += w * lum * cp.k[4];
    }
    let lms = mat(3u, vec3<f32>(max(l, 0.0), a, b));
    return mat(1u, lms * lms * lms);
}

@compute @workgroup_size(256)
fn color(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= size.pixels) {
        return;
    }
    let rgb = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let lms = mat(0u, rgb);
    let lab = mat(2u, vec3<f32>(cbrt(lms.x), cbrt(lms.y), cbrt(lms.z)));
    var l = lab.x;
    var chroma = length(lab.yz);
    var hue = atan2(lab.z, lab.y);

    let bw = band_weights(hue);
    let bands = array<u32, 2>(u32(bw.x), u32(bw.y));
    let ws = array<f32, 2>(bw.z, bw.w);
    var out: vec3<f32>;
    if (cp.grayscale == 1u) {
        // Each colour's grey tone from the mix; neutrals stay as they are.
        let mix = ws[0] * amount(bands[0], 3u) + ws[1] * amount(bands[1], 3u);
        let colourfulness = smoothstep_(0.0, 0.08, chroma);
        l *= max(1.0 + mix * cp.k[1] * 1.5 * colourfulness, 0.0);
        out = grade(l, 0.0, 0.0);
    } else {
        var dh = 0.0;
        var sat_gain = 1.0;
        var lum_gain = 0.0;
        for (var j = 0u; j < 2u; j++) {
            dh += ws[j] * amount(bands[j], 0u);
            sat_gain += ws[j] * amount(bands[j], 1u);
            lum_gain += ws[j] * amount(bands[j], 2u);
        }
        // Neutrals have no hue: the HSL hue and luminance sliders fade out near grey.
        let colourfulness = smoothstep_(0.0, 0.08, chroma);
        hue += dh * radians(cp.k[0]) * colourfulness;
        chroma *= max(sat_gain, 0.0);
        l *= 1.0 + lum_gain * cp.k[1] * colourfulness;
        if (cp.vibrance != 0.0) {
            var skin = 0.0;
            for (var j = 0u; j < 2u; j++) {
                if (bands[j] == cp.orange_index) {
                    skin += ws[j];
                }
            }
            let low_sat = 1.0 - smoothstep_(0.0, 0.25, chroma);
            chroma *= max(1.0 + cp.vibrance * cp.k[2] * low_sat * (1.0 - 0.5 * skin), 0.0);
        }
        chroma *= max(1.0 + cp.saturation, 0.0);
        out = grade(l, chroma * cos(hue), chroma * sin(hue));
    }
    img[i * 3u] = out.x;
    img[i * 3u + 1u] = out.y;
    img[i * 3u + 2u] = out.z;
}
