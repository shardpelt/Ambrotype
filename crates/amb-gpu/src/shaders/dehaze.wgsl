// SPDX-License-Identifier: GPL-3.0-or-later
// Dehaze's per-pixel parts (amb-engine's dehaze.rs is the reference): the
// non-finite values zeroed, the block-averaged copy the haze is measured on
// (on the CPU), and the haze map applied: recovery or fog, then the
// exposure compensation.

struct Dehaze {
    w: u32,
    h: u32,
    factor: u32,
    sw: u32,
    sh: u32,
    omega: f32,
    min_transmission: f32,
    shadow_keep: f32,
    airlight: array<f32, 3>,
    inv_airlight: array<f32, 3>,
    fog: f32,
    fog_depth: f32,
    fog_colour: array<f32, 3>,
    comp: f32,
    inv_airlight_y: f32,
    lum: array<f32, 3>,
}

@group(0) @binding(0) var<storage, read_write> img: array<f32>;
@group(0) @binding(1) var<storage, read_write> small: array<f32>;
@group(0) @binding(2) var<storage, read> coef_a: array<f32>;
@group(0) @binding(3) var<storage, read> coef_b: array<f32>;
@group(0) @binding(4) var<storage, read> dp: Dehaze;

@compute @workgroup_size(256)
fn sanitise(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= dp.w * dp.h * 3u) { return; }
    let v = img[i];
    if (v != v || abs(v) > 3.4028235e38) {
        img[i] = 0.0;
    }
}

// Box-average f × f blocks (partial blocks at the right and bottom average
// what they cover), summing in the CPU's order.
@compute @workgroup_size(256)
fn downscale(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= dp.sw * dp.sh) { return; }
    let bx = i % dp.sw;
    let by = i / dp.sw;
    let f = dp.factor;
    let x0 = bx * f;
    let x1 = min((bx + 1u) * f, dp.w);
    let y0 = by * f;
    let y1 = min((by + 1u) * f, dp.h);
    var acc = vec3<f32>(0.0);
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            let j = (y * dp.w + x) * 3u;
            acc += vec3<f32>(img[j], img[j + 1u], img[j + 2u]);
        }
    }
    let cover = f32((y1 - y0) * (x1 - x0));
    acc /= cover;
    small[i * 3u] = acc.x;
    small[i * 3u + 1u] = acc.y;
    small[i * 3u + 2u] = acc.z;
}

// Position of full-resolution index i on the grid (cell centres aligned).
fn lerp_pos(i: u32, n: u32) -> vec3<f32> {
    let s = clamp((f32(i) + 0.5) / f32(dp.factor) - 0.5, 0.0, f32(n - 1u));
    let lo = min(u32(floor(s)), n - 1u);
    return vec3<f32>(f32(lo), f32(min(lo + 1u, n - 1u)), s - f32(lo));
}

fn bilerp_a(cx: vec3<f32>, cy: vec3<f32>) -> f32 {
    let r0 = u32(cy.x) * dp.sw;
    let r1 = u32(cy.y) * dp.sw;
    let top = coef_a[r0 + u32(cx.x)] + (coef_a[r0 + u32(cx.y)] - coef_a[r0 + u32(cx.x)]) * cx.z;
    let bottom = coef_a[r1 + u32(cx.x)] + (coef_a[r1 + u32(cx.y)] - coef_a[r1 + u32(cx.x)]) * cx.z;
    return top + (bottom - top) * cy.z;
}

fn bilerp_b(cx: vec3<f32>, cy: vec3<f32>) -> f32 {
    let r0 = u32(cy.x) * dp.sw;
    let r1 = u32(cy.y) * dp.sw;
    let top = coef_b[r0 + u32(cx.x)] + (coef_b[r0 + u32(cx.y)] - coef_b[r0 + u32(cx.x)]) * cx.z;
    let bottom = coef_b[r1 + u32(cx.x)] + (coef_b[r1 + u32(cx.y)] - coef_b[r1 + u32(cx.x)]) * cx.z;
    return top + (bottom - top) * cy.z;
}

@compute @workgroup_size(256)
fn dehaze_apply(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) g: vec3<u32>) {
    let i = pixel_index(id, g);
    if (i >= dp.w * dp.h) { return; }
    let p = vec3<f32>(img[i * 3u], img[i * 3u + 1u], img[i * 3u + 2u]);
    let cx = lerp_pos(i % dp.w, dp.sw);
    let cy = lerp_pos(i / dp.w, dp.sh);
    let y = dp.lum[0] * p.x + dp.lum[1] * p.y + dp.lum[2] * p.z;
    let guide = sqrt(max(y, 0.0) * dp.inv_airlight_y);
    let d = clamp(bilerp_a(cx, cy) * guide + bilerp_b(cx, cy), 0.0, 1.0);
    var out: vec3<f32>;
    if (dp.fog > 0.0) {
        let veil = min(dp.fog * (1.0 - dp.fog_depth + dp.fog_depth * d), 1.0);
        let fc = vec3<f32>(dp.fog_colour[0], dp.fog_colour[1], dp.fog_colour[2]);
        out = p + (fc - p) * veil;
    } else {
        var t = max(1.0 - dp.omega * d, dp.min_transmission);
        // Soft shadow floor (dehaze.rs `Recovery::apply`).
        let m = min(p.x * dp.inv_airlight[0], min(p.y * dp.inv_airlight[1], p.z * dp.inv_airlight[2]));
        if (m < 1.0) {
            t = max(t, (1.0 - m) / (1.0 - dp.shadow_keep * m));
        }
        let inv_t = 1.0 / min(t, 1.0);
        for (var c = 0u; c < 3u; c++) {
            let a = dp.airlight[c];
            let below = min(p[c], a) - a;
            out[c] = a + below * inv_t + max(p[c] - a, 0.0);
        }
    }
    var gain = dp.comp;
    if (dp.omega > 0.0) {
        gain = exp(log(dp.comp) * (1.0 - d));
    }
    img[i * 3u] = max(out.x * gain, 0.0);
    img[i * 3u + 1u] = max(out.y * gain, 0.0);
    img[i * 3u + 2u] = max(out.z * gain, 0.0);
}
