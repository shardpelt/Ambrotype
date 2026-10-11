// SPDX-License-Identifier: GPL-3.0-or-later
//! Highlights/Shadows and Texture/Clarity on the GPU, with the plane filters
//! they are built from (Gaussian and box blurs, the fast guided filter).

use amb_engine::detail::SharpenPlan;
use amb_engine::filters::GaussianPlan;
use amb_engine::local::{GuidedPlan, LEVEL_BOUND, LocalPlan, RATIO_KNEE_Y};
use amb_engine::local_tone::TonePlan;
use bytemuck::{Pod, Zeroable};

use crate::device::{Gpu, Kernel};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct PlaneParams {
    w: u32,
    h: u32,
    n: u32,
    frac: f32,
    norm: f32,
    klen: u32,
    s: u32,
    eps: f32,
    out_offset: u32,
    in_offset: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct LocalParams {
    w: u32,
    h: u32,
    s: u32,
    wl: u32,
    hl: u32,
    amount: f32,
    limit: f32,
    combined_limit: f32,
    lum: [f32; 3],
    knee: f32,
    level_bound: f32,
    remove: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct SharpenParams {
    w: u32,
    h: u32,
    gain: f32,
    win: u32,
    win_frac: f32,
    margin: f32,
    max_ratio: f32,
    masked: u32,
    e0: f32,
    e1: f32,
    lum: [f32; 3],
}

pub(crate) struct LocalKernels {
    sharpen_level: Kernel,
    edge_mask: Kernel,
    sharpen_apply: Kernel,
    conv_h: Kernel,
    conv_v: Kernel,
    box_h: Kernel,
    box_v: Kernel,
    downsample: Kernel,
    square: Kernel,
    guided_coef: Kernel,
    luma_level: Kernel,
    band: Kernel,
    local_apply: Kernel,
    tone_apply: Kernel,
}

impl LocalKernels {
    pub(crate) fn new(gpu: &Gpu, common: &str) -> LocalKernels {
        let planes = format!("{common}\n{}", include_str!("shaders/planes.wgsl"));
        let local = format!("{common}\n{}", include_str!("shaders/local.wgsl"));
        // Bindings: in0, in1, out0, out1, parameters.
        const PLANE: &[bool] = &[true, true, false, false, true];
        // Bindings: image, level, p0, p1, p2, acc0, acc1, parameters.
        const LOCAL: &[bool] = &[false, false, true, true, true, false, false, true];
        let sharpen = format!("{common}\n{}", include_str!("shaders/sharpen.wgsl"));
        // Bindings: image, level, blur, mask, raw mask, parameters.
        const SHARPEN: &[bool] = &[false, false, true, true, false, true];
        let sh = |entry: &str| Kernel::new(gpu, entry, &sharpen, entry, SHARPEN);
        let p = |entry: &str| Kernel::new(gpu, entry, &planes, entry, PLANE);
        let l = |entry: &str| Kernel::new(gpu, entry, &local, entry, LOCAL);
        LocalKernels {
            sharpen_level: sh("sharpen_level"),
            edge_mask: sh("edge_mask"),
            sharpen_apply: sh("sharpen_apply"),
            conv_h: p("conv_h"),
            conv_v: p("conv_v"),
            box_h: p("box_h"),
            box_v: p("box_v"),
            downsample: p("downsample"),
            square: p("square"),
            guided_coef: p("guided_coef"),
            luma_level: l("luma_level"),
            band: l("band"),
            local_apply: l("local_apply"),
            tone_apply: l("tone_apply"),
        }
    }
}

/// Recording plane filters into one command encoder.
pub(crate) struct Planes<'a> {
    pub gpu: &'a Gpu,
    pub enc: &'a mut wgpu::CommandEncoder,
}

impl<'a> Planes<'a> {
    pub(crate) fn new(gpu: &'a Gpu, enc: &'a mut wgpu::CommandEncoder) -> Self {
        Planes { gpu, enc }
    }

    /// Buffers for the slots a kernel doesn't use. Each slot gets its own:
    /// one buffer may not be bound read-only and read-write in one dispatch.
    fn unused<const N: usize>(&self) -> [wgpu::Buffer; N] {
        std::array::from_fn(|_| self.gpu.buffer(1))
    }

    /// The kernels, borrowed from the GPU (not from `self`).
    fn k(&self) -> &'a LocalKernels {
        let gpu: &'a Gpu = self.gpu;
        &gpu.kernels().local
    }

    /// One plane kernel: inputs `in0` (and `in1`), outputs `out0` (and `out1`).
    #[allow(clippy::too_many_arguments)]
    fn plane(
        &mut self,
        kernel: &Kernel,
        p: PlaneParams,
        in0: &wgpu::Buffer,
        in1: Option<&wgpu::Buffer>,
        out0: &wgpu::Buffer,
        out1: Option<&wgpu::Buffer>,
        items: usize,
    ) {
        let params = self.gpu.storage(&p);
        let [u1, u2] = self.unused();
        kernel.dispatch(
            self.gpu,
            self.enc,
            &[in0, in1.unwrap_or(&u1), out0, out1.unwrap_or(&u2), &params],
            items,
        );
    }

    /// A box blur of radius `r` (fractional: the end taps weighted by its
    /// fraction), rows then columns; into `out` at `offset`.
    fn frac_box(
        &mut self,
        src: &wgpu::Buffer,
        w: usize,
        h: usize,
        r: f32,
        out: &wgpu::Buffer,
        offset: usize,
    ) {
        let n = r.floor();
        let p = PlaneParams {
            w: w as u32,
            h: h as u32,
            n: n as u32,
            frac: r - n,
            norm: 1.0 / (2.0 * r + 1.0),
            ..Default::default()
        };
        let tmp = self.gpu.buffer(w * h);
        let k = self.k();
        self.plane(&k.box_h, p, src, None, &tmp, None, w * h);
        self.plane(
            &k.box_v,
            PlaneParams {
                out_offset: offset as u32,
                ..p
            },
            &tmp,
            None,
            out,
            None,
            w * h,
        );
    }

    /// `src` blurred as `plan` says (`None`: unchanged).
    pub(crate) fn gaussian(
        &mut self,
        src: &wgpu::Buffer,
        w: usize,
        h: usize,
        plan: &GaussianPlan,
    ) -> Option<wgpu::Buffer> {
        match plan {
            GaussianPlan::Identity => None,
            GaussianPlan::Kernel(k) => {
                let weights = self.gpu.upload(k);
                let p = PlaneParams {
                    w: w as u32,
                    h: h as u32,
                    klen: k.len() as u32,
                    ..Default::default()
                };
                let (tmp, out) = (self.gpu.buffer(w * h), self.gpu.buffer(w * h));
                let k = self.k();
                self.plane(&k.conv_h, p, src, Some(&weights), &tmp, None, w * h);
                self.plane(&k.conv_v, p, &tmp, Some(&weights), &out, None, w * h);
                Some(out)
            }
            GaussianPlan::Boxes(radii) => {
                let mut cur: Option<wgpu::Buffer> = None;
                for &r in radii {
                    if r == 0 {
                        continue;
                    }
                    let out = self.gpu.buffer(w * h);
                    let input = cur.as_ref().unwrap_or(src).clone();
                    self.frac_box(&input, w, h, r as f32, &out, 0);
                    cur = Some(out);
                }
                cur
            }
        }
    }

    /// The fast guided filter's upsampling coefficients of `src` (the plane
    /// guides itself): `A` then `B`, each `wl × hl`, in one buffer.
    pub(crate) fn guided(
        &mut self,
        src: &wgpu::Buffer,
        w: usize,
        h: usize,
        g: &GuidedPlan,
    ) -> wgpu::Buffer {
        let (wl, hl) = (g.wl, g.hl);
        let n = wl * hl;
        let low = if g.s == 1 {
            src.clone()
        } else {
            let low = self.gpu.buffer(n);
            self.plane(
                &self.k().downsample,
                PlaneParams {
                    w: w as u32,
                    h: h as u32,
                    s: g.s as u32,
                    ..Default::default()
                },
                src,
                None,
                &low,
                None,
                n,
            );
            low
        };
        let (mean, corr, sq) = (self.gpu.buffer(n), self.gpu.buffer(n), self.gpu.buffer(n));
        let lowp = PlaneParams {
            w: wl as u32,
            h: hl as u32,
            eps: g.eps,
            ..Default::default()
        };
        self.frac_box(&low, wl, hl, g.r, &mean, 0);
        self.plane(&self.k().square, lowp, &low, None, &sq, None, n);
        self.frac_box(&sq, wl, hl, g.r, &corr, 0);
        let (a, b) = (self.gpu.buffer(n), self.gpu.buffer(n));
        self.plane(
            &self.k().guided_coef,
            lowp,
            &mean,
            Some(&corr),
            &a,
            Some(&b),
            n,
        );
        let ab = self.gpu.buffer(2 * n);
        self.frac_box(&a, wl, hl, g.r, &ab, 0);
        self.frac_box(&b, wl, hl, g.r, &ab, n);
        ab
    }

    fn local(
        &mut self,
        kernel: fn(&LocalKernels) -> &Kernel,
        p: LocalParams,
        b: [&wgpu::Buffer; 7],
        items: usize,
    ) {
        let params = self.gpu.storage(&p);
        kernel(self.k()).dispatch(
            self.gpu,
            self.enc,
            &[b[0], b[1], b[2], b[3], b[4], b[5], b[6], &params],
            items,
        );
    }

    fn level(&mut self, img: &wgpu::Buffer, w: usize, h: usize) -> (wgpu::Buffer, LocalParams) {
        let lvl = self.gpu.buffer(w * h);
        let p = LocalParams {
            w: w as u32,
            h: h as u32,
            lum: amb_engine::buffer::luminance_weights(),
            knee: RATIO_KNEE_Y,
            level_bound: LEVEL_BOUND,
            ..Default::default()
        };
        let [a, b, c, d, e] = self.unused();
        self.local(|k| &k.luma_level, p, [img, &lvl, &a, &b, &c, &d, &e], w * h);
        (lvl, p)
    }

    /// Highlights and Shadows on `img` (`w × h` RGB).
    pub(crate) fn highlights_shadows(
        &mut self,
        img: &wgpu::Buffer,
        w: usize,
        h: usize,
        plan: &TonePlan,
    ) {
        let (lvl, p) = self.level(img, w, h);
        let ab = self.guided(&lvl, w, h, &plan.guided);
        let change = self.gpu.upload(&plan.change);
        let g = plan.guided;
        let p = LocalParams {
            s: g.s as u32,
            wl: g.wl as u32,
            hl: g.hl as u32,
            ..p
        };
        let [a, b, c] = self.unused();
        self.local(
            |k| &k.tone_apply,
            p,
            [img, &lvl, &ab, &a, &change, &b, &c],
            w * h,
        );
    }

    /// Texture and Clarity on `img` (`w × h` RGB).
    pub(crate) fn local_contrast(
        &mut self,
        img: &wgpu::Buffer,
        w: usize,
        h: usize,
        plan: &LocalPlan,
    ) {
        let (lvl, p) = self.level(img, w, h);
        let (boost, remove) = (self.gpu.buffer(w * h), self.gpu.buffer(w * h));
        for band in &plan.bands {
            // Without a pre-blur the source is the level itself, copied: the
            // level is bound read-write in the same dispatch.
            let src = match self.gaussian(&lvl, w, h, &band.prefilter) {
                Some(b) => b,
                None => {
                    let copy = self.gpu.buffer(w * h);
                    self.enc
                        .copy_buffer_to_buffer(&lvl, 0, &copy, 0, (w * h * 4) as u64);
                    copy
                }
            };
            let ab = self.guided(&src, w, h, &band.guided);
            let weight = self.gpu.upload(&band.weight);
            let g = band.guided;
            let bp = LocalParams {
                s: g.s as u32,
                wl: g.wl as u32,
                hl: g.hl as u32,
                amount: band.amount,
                limit: band.limit,
                remove: (band.amount <= 0.0) as u32,
                ..p
            };
            self.local(
                |k| &k.band,
                bp,
                [img, &lvl, &src, &ab, &weight, &boost, &remove],
                w * h,
            );
        }
        let ap = LocalParams {
            combined_limit: plan.combined_limit,
            ..p
        };
        let [a, b, c] = self.unused();
        self.local(
            |k| &k.local_apply,
            ap,
            [img, &lvl, &a, &b, &c, &boost, &remove],
            w * h,
        );
    }

    fn sharpen_kernel(&mut self, kernel: &Kernel, p: SharpenParams, b: [&wgpu::Buffer; 5]) {
        let params = self.gpu.storage(&p);
        kernel.dispatch(
            self.gpu,
            self.enc,
            &[b[0], b[1], b[2], b[3], b[4], &params],
            (p.w * p.h) as usize,
        );
    }

    /// Sharpening on `img` (`w × h` RGB).
    pub(crate) fn sharpen(&mut self, img: &wgpu::Buffer, w: usize, h: usize, plan: &SharpenPlan) {
        let k = self.k();
        let mut p = SharpenParams {
            w: w as u32,
            h: h as u32,
            gain: plan.gain,
            win: plan.win as u32,
            win_frac: plan.win_frac,
            margin: plan.margin,
            max_ratio: plan.max_ratio,
            lum: amb_engine::buffer::luminance_weights(),
            ..Default::default()
        };
        let lvl = self.gpu.buffer(w * h);
        let [a, b, c] = self.unused();
        self.sharpen_kernel(&k.sharpen_level, p, [img, &lvl, &a, &b, &c]);
        // The unsharp mask's blur, and the edge mask.
        let blur = self
            .gaussian(&lvl, w, h, &GaussianPlan::Kernel(plan.kernel.clone()))
            .expect("a kernel blurs");
        let mask = match &plan.mask {
            Some(m) => {
                p.masked = 1;
                p.e0 = m.e0;
                p.e1 = m.e1;
                let blurred = self.gaussian(&lvl, w, h, &m.blur);
                let blurred = blurred.as_ref().unwrap_or(&lvl).clone();
                let raw = self.gpu.buffer(w * h);
                let [a, b] = self.unused();
                // `lvl` is unused by this kernel; a scratch buffer stands in
                // so the blurred level isn't bound twice.
                self.sharpen_kernel(&k.edge_mask, p, [&a, &b, &blurred, &blur, &raw]);
                self.gaussian(&raw, w, h, &m.smooth).unwrap_or(raw)
            }
            None => self.gpu.buffer(1),
        };
        let [a] = self.unused();
        self.sharpen_kernel(&k.sharpen_apply, p, [img, &lvl, &blur, &mask, &a]);
    }
}
