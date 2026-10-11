// SPDX-License-Identifier: GPL-3.0-or-later
//! Luminance noise reduction on the GPU: the noise tiles are gathered here,
//! turned into tables on the CPU (`detail::luminance_nr_tables`), and the
//! filters run here.

use amb_engine::detail::{LuminanceNrPlan, TileSums, luminance_nr_tables};
use bytemuck::{Pod, Zeroable};

use crate::device::{Gpu, Kernel};
use crate::local::Planes;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LnrParams {
    w: u32,
    h: u32,
    tw: u32,
    th: u32,
    eps_scale: f32,
    restore: f32,
    threshold: f32,
    contrast_gain: f32,
    max_ratio: f32,
    lum: [f32; 3],
}

pub(crate) struct NoiseKernels {
    level: Kernel,
    tiles: Kernel,
    coef: Kernel,
    guided: Kernel,
    removed: Kernel,
    kept: Kernel,
    apply: Kernel,
}

impl NoiseKernels {
    pub(crate) fn new(gpu: &Gpu, common: &str) -> NoiseKernels {
        let src = format!("{common}\n{}", include_str!("shaders/lnr.wgsl"));
        // Bindings: image, p0, p1, p2, out0, out1, tables, parameters.
        const LNR: &[bool] = &[false, true, true, true, false, false, true, true];
        let k = |entry: &str| Kernel::new(gpu, entry, &src, entry, LNR);
        NoiseKernels {
            level: k("lnr_level"),
            tiles: k("lnr_tiles"),
            coef: k("lnr_coef"),
            guided: k("lnr_guided"),
            removed: k("lnr_removed"),
            kept: k("lnr_kept"),
            apply: k("lnr_apply"),
        }
    }
}

/// One luminance noise reduction kernel's bindings; `None` slots get
/// scratch buffers (one buffer may not be bound twice in a dispatch).
struct Bind<'b> {
    img: Option<&'b wgpu::Buffer>,
    p: [Option<&'b wgpu::Buffer>; 3],
    out: [Option<&'b wgpu::Buffer>; 2],
}

impl Gpu {
    fn lnr(
        &self,
        enc: &mut wgpu::CommandEncoder,
        kernel: &Kernel,
        b: Bind,
        luts: &wgpu::Buffer,
        params: &LnrParams,
        items: usize,
    ) {
        let params = self.storage(params);
        let spare: [wgpu::Buffer; 6] = std::array::from_fn(|_| self.buffer(1));
        let slots = [
            b.img.unwrap_or(&spare[0]),
            b.p[0].unwrap_or(&spare[1]),
            b.p[1].unwrap_or(&spare[2]),
            b.p[2].unwrap_or(&spare[3]),
            b.out[0].unwrap_or(&spare[4]),
            b.out[1].unwrap_or(&spare[5]),
            luts,
            &params,
        ];
        kernel.dispatch(self, enc, &slots, items);
    }

    /// The self-guided filter of `src` with radius `r` and eps scaled by `eps_scale`.
    fn self_guided(
        &self,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::Buffer,
        r: f32,
        luts: &wgpu::Buffer,
        p: &LnrParams,
    ) -> wgpu::Buffer {
        let k = &self.kernels().noise;
        let (w, h) = (p.w as usize, p.h as usize);
        let n = w * h;
        // Radii beyond the larger side (runaway calibration values only) are capped there.
        let r = r.min(w.max(h) as f32);
        let (mean, corr, sq) = (self.buffer(n), self.buffer(n), self.buffer(n));
        let mut planes = Planes::new(self, enc);
        planes.frac_box(src, w, h, r, &mean, 0);
        planes.square(src, w, h, &sq);
        planes.frac_box(&sq, w, h, r, &corr, 0);
        let (a, b) = (self.buffer(n), self.buffer(n));
        let bind = Bind {
            img: None,
            p: [Some(&mean), Some(&corr), None],
            out: [Some(&a), Some(&b)],
        };
        self.lnr(enc, &k.coef, bind, luts, p, n);
        let (mean_a, mean_b) = (self.buffer(n), self.buffer(n));
        let mut planes = Planes::new(self, enc);
        planes.frac_box(&a, w, h, r, &mean_a, 0);
        planes.frac_box(&b, w, h, r, &mean_b, 0);
        let out = self.buffer(n);
        let bind = Bind {
            img: None,
            p: [Some(src), Some(&mean_a), Some(&mean_b)],
            out: [Some(&out), None],
        };
        self.lnr(enc, &k.guided, bind, luts, p, n);
        out
    }

    /// Luminance noise reduction on `img` (`w × h` RGB).
    pub(crate) fn luminance_nr(
        &self,
        enc: &mut wgpu::CommandEncoder,
        img: &wgpu::Buffer,
        w: usize,
        h: usize,
        plan: &LuminanceNrPlan,
    ) {
        let k = &self.kernels().noise;
        let n = w * h;
        let (tw, th) = ((w / 2).div_ceil(8), (h / 2).div_ceil(8));
        let mut p = LnrParams {
            w: w as u32,
            h: h as u32,
            tw: tw as u32,
            th: th as u32,
            eps_scale: 1.0,
            restore: plan.restore,
            threshold: plan.threshold,
            contrast_gain: plan.contrast_gain,
            max_ratio: plan.max_ratio,
            lum: amb_engine::buffer::luminance_weights(),
        };
        let none = self.buffer(1);
        // 1. The level and its noise tiles; the tables on the CPU.
        let l = self.buffer(n);
        let bind = Bind {
            img: Some(img),
            p: [None; 3],
            out: [Some(&l), None],
        };
        self.lnr(enc, &k.level, bind, &none, &p, n);
        let sums = if tw * th > 0 {
            let tiles = self.buffer(tw * th * 7);
            let bind = Bind {
                img: None,
                p: [Some(&l), None, None],
                out: [Some(&tiles), None],
            };
            self.lnr(enc, &k.tiles, bind, &none, &p, tw * th);
            self.flush(enc);
            self.download(&tiles, tw * th * 7)
        } else {
            Vec::new()
        };
        let tiles: Vec<TileSums> = sums
            .chunks_exact(7)
            .map(|t| TileSums {
                power: [t[0], t[1], t[2]],
                lum: t[3],
                lo: t[4],
                hi: t[5],
                n: t[6] as u32,
            })
            .collect();
        let Some(tables) = luminance_nr_tables(&tiles, plan) else {
            return;
        };
        let luts = self.upload(&[tables.sigma, tables.eps].concat());
        // Two-scale smoothing.
        let mut smooth = self.self_guided(enc, &l, plan.r_fine, &luts, &p);
        if let Some((r, eps_scale)) = plan.coarse {
            let coarse = LnrParams { eps_scale, ..p };
            smooth = self.self_guided(enc, &smooth, r, &luts, &coarse);
        }
        // 2. Detail, and 3. what Contrast blurs.
        let (removed, sq) = (self.buffer(n), self.buffer(n));
        let bind = Bind {
            img: None,
            p: [Some(&l), Some(&smooth), None],
            out: [Some(&removed), Some(&sq)],
        };
        self.lnr(enc, &k.removed, bind, &luts, &p, n);
        let (level, rest) = if plan.restore > 0.0 {
            let energy = self.buffer(n);
            Planes::new(self, enc).frac_box(&sq, w, h, 1.0, &energy, 0);
            let (level, rest) = (self.buffer(n), self.buffer(n));
            let bind = Bind {
                img: None,
                p: [Some(&energy), Some(&smooth), Some(&removed)],
                out: [Some(&level), Some(&rest)],
            };
            self.lnr(enc, &k.kept, bind, &luts, &p, n);
            (level, rest)
        } else {
            (smooth, removed)
        };
        let lowpass = if plan.contrast_gain > 0.0 {
            let blurred = Planes::new(self, enc).gaussian(&rest, w, h, &plan.contrast_blur);
            Some(blurred.unwrap_or(rest))
        } else {
            p.contrast_gain = 0.0;
            None
        };
        let bind = Bind {
            img: Some(img),
            p: [Some(&l), Some(&level), lowpass.as_ref()],
            out: [None, None],
        };
        self.lnr(enc, &k.apply, bind, &luts, &p, n);
    }
}
