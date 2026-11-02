// SPDX-License-Identifier: GPL-3.0-or-later
//! Short videos (Edit Replay): sRGB frames encoded as AV1 with rav1e (the
//! encoder AVIF export already uses) in an MP4 file written here.
//!
//! The MP4 is the smallest one players accept for a single AV1 track, laid
//! out for streaming (the index before the data): `ftyp`, then `moov` with
//! one video `trak` (an `av01` sample entry holding the encoder's `av1C`
//! and a BT.709 `colr`), then `mdat` with every frame. Temporal delimiter
//! OBUs are left out of the samples, as the AV1-in-ISOBMFF spec asks.

use std::path::Path;

use rav1e::prelude::*;

/// One frame: 8-bit sRGB, `width × height × 3`, rows top to bottom.
pub struct Frame<'a> {
    pub rgb: &'a [u8],
}

/// Settings for [`Encoder`].
#[derive(Debug, Clone, Copy)]
pub struct VideoSettings {
    /// Even width and height (4:2:0 halves both).
    pub width: usize,
    pub height: usize,
    pub fps: u32,
    /// rav1e's quantizer, 0..255 (lower is better and larger).
    pub quantizer: usize,
    /// rav1e's speed preset, 0..10 (10 fastest).
    pub speed: u8,
}

impl Default for VideoSettings {
    fn default() -> Self {
        VideoSettings {
            width: 1080,
            height: 720,
            fps: 30,
            quantizer: 90,
            speed: 10,
        }
    }
}

/// An AV1 encoder fed one sRGB frame at a time, finished into an MP4.
pub struct Encoder {
    settings: VideoSettings,
    ctx: Context<u8>,
    samples: Vec<(Vec<u8>, bool)>,
}

impl Encoder {
    pub fn new(s: VideoSettings) -> Result<Encoder, String> {
        if s.width < 16 || s.height < 16 || s.width % 2 != 0 || s.height % 2 != 0 {
            return Err(format!(
                "video size {}x{} must be even, 16 or more",
                s.width, s.height
            ));
        }
        let mut enc = EncoderConfig::with_speed_preset(s.speed);
        enc.width = s.width;
        enc.height = s.height;
        enc.time_base = Rational::new(1, u64::from(s.fps.max(1)));
        enc.bit_depth = 8;
        enc.chroma_sampling = ChromaSampling::Cs420;
        enc.quantizer = s.quantizer.min(255);
        enc.min_key_frame_interval = 0;
        enc.max_key_frame_interval = u64::from(s.fps.max(1)) * 5;
        enc.color_description = Some(ColorDescription {
            color_primaries: ColorPrimaries::BT709,
            transfer_characteristics: TransferCharacteristics::BT709,
            matrix_coefficients: MatrixCoefficients::BT709,
        });
        enc.pixel_range = PixelRange::Limited;
        // Tiles encode in parallel; no look-ahead (a replay has few cuts).
        enc.tiles = 8;
        enc.low_latency = true;
        let cfg = Config::new().with_encoder_config(enc);
        let ctx = cfg.new_context().map_err(|e| format!("AV1 encoder: {e}"))?;
        Ok(Encoder {
            settings: s,
            ctx,
            samples: Vec::new(),
        })
    }

    /// Encode `frame` (sRGB, the settings' size).
    pub fn push(&mut self, frame: Frame) -> Result<(), String> {
        let (w, h) = (self.settings.width, self.settings.height);
        if frame.rgb.len() != w * h * 3 {
            return Err("a frame of the wrong size".into());
        }
        let mut f = self.ctx.new_frame();
        let (y, u, v) = yuv420(frame.rgb, w, h);
        f.planes[0].copy_from_raw_u8(&y, w, 1);
        f.planes[1].copy_from_raw_u8(&u, w / 2, 1);
        f.planes[2].copy_from_raw_u8(&v, w / 2, 1);
        self.ctx
            .send_frame(f)
            .map_err(|e| format!("AV1 encoder: {e}"))?;
        self.drain()
    }

    fn drain(&mut self) -> Result<(), String> {
        loop {
            match self.ctx.receive_packet() {
                Ok(p) => self.samples.push((
                    without_temporal_delimiters(&p.data),
                    p.frame_type == FrameType::KEY,
                )),
                Err(EncoderStatus::Encoded) => continue,
                Err(EncoderStatus::NeedMoreData | EncoderStatus::LimitReached) => return Ok(()),
                Err(e) => return Err(format!("AV1 encoder: {e}")),
            }
        }
    }

    /// Flush the encoder and write the MP4 to `path`. Returns its size.
    pub fn finish(mut self, path: &Path) -> Result<u64, String> {
        self.ctx.flush();
        self.drain()?;
        if self.samples.is_empty() {
            return Err("no frames".into());
        }
        let av1c = self.ctx.container_sequence_header();
        let bytes = mp4(&self.settings, &av1c, &self.samples);
        std::fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(bytes.len() as u64)
    }
}

/// sRGB to Y′CbCr 4:2:0, BT.709, limited range.
fn yuv420(rgb: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (kr, kb) = (0.2126f32, 0.0722f32);
    let kg = 1.0 - kr - kb;
    let mut y = vec![0u8; w * h];
    let mut u = vec![0u8; w * h / 4];
    let mut v = vec![0u8; w * h / 4];
    let px = |i: usize| [0, 1, 2].map(|c| f32::from(rgb[i * 3 + c]) / 255.0);
    for (i, out) in y.iter_mut().enumerate() {
        let [r, g, b] = px(i);
        *out = (16.0 + 219.0 * (kr * r + kg * g + kb * b))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    for cy in 0..h / 2 {
        for cx in 0..w / 2 {
            let mut acc = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px((cy * 2 + dy) * w + cx * 2 + dx);
                (0..3).for_each(|c| acc[c] += p[c] / 4.0);
            }
            let [r, g, b] = acc;
            let luma = kr * r + kg * g + kb * b;
            let cb = (b - luma) / (2.0 * (1.0 - kb));
            let cr = (r - luma) / (2.0 * (1.0 - kr));
            let i = cy * (w / 2) + cx;
            u[i] = (128.0 + 224.0 * cb).round().clamp(0.0, 255.0) as u8;
            v[i] = (128.0 + 224.0 * cr).round().clamp(0.0, 255.0) as u8;
        }
    }
    (y, u, v)
}

/// `data` (OBUs, each with its size field) without temporal delimiters.
fn without_temporal_delimiters(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let header = data[i];
        let kind = (header >> 3) & 0x0f;
        let extension = header & 0x04 != 0;
        let has_size = header & 0x02 != 0;
        let mut j = i + 1 + extension as usize;
        if !has_size {
            // No size: the OBU runs to the end.
            if kind != 2 {
                out.extend_from_slice(&data[i..]);
            }
            break;
        }
        let (size, read) = leb128(&data[j.min(data.len())..]);
        j += read;
        let end = (j + size).min(data.len());
        if kind != 2 {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
    }
    out
}

fn leb128(b: &[u8]) -> (usize, usize) {
    let mut v = 0usize;
    for (i, &byte) in b.iter().take(8).enumerate() {
        v |= ((byte & 0x7f) as usize) << (7 * i);
        if byte & 0x80 == 0 {
            return (v, i + 1);
        }
    }
    (v, b.len().min(8))
}

/// A box: size, four-character type, contents.
fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// A full box: a box whose contents start with a version and flags.
fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = vec![version];
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// The identity matrix of `mvhd` and `tkhd`.
const MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn mp4(s: &VideoSettings, av1c: &[u8], samples: &[(Vec<u8>, bool)]) -> Vec<u8> {
    let n = samples.len() as u32;
    let fps = s.fps.max(1);
    // Movie time in ms; media time in frames.
    let duration_ms = (u64::from(n) * 1000 / u64::from(fps)) as u32;
    let ftyp = bx(
        b"ftyp",
        &[
            b"isom".as_slice(),
            &0x200u32.to_be_bytes(),
            b"isomiso2av01mp41",
        ]
        .concat(),
    );

    let mvhd = full(
        b"mvhd",
        0,
        0,
        &[
            u32s(&[0, 0, 1000, duration_ms, 0x0001_0000]),
            vec![0x01, 0x00, 0, 0],
            u32s(&[0, 0]),
            u32s(&MATRIX),
            u32s(&[0; 6]),
            u32s(&[2]),
        ]
        .concat(),
    );
    let tkhd = full(
        b"tkhd",
        0,
        3,
        &[
            u32s(&[0, 0, 1, 0, duration_ms, 0, 0, 0]),
            u32s(&MATRIX),
            u32s(&[(s.width as u32) << 16, (s.height as u32) << 16]),
        ]
        .concat(),
    );
    let mdhd = full(
        b"mdhd",
        0,
        0,
        &[u32s(&[0, 0, fps, n]), vec![0x55, 0xc4, 0, 0]].concat(),
    );
    let hdlr = full(
        b"hdlr",
        0,
        0,
        &[
            u32s(&[0]),
            b"vide".to_vec(),
            u32s(&[0, 0, 0]),
            b"Ambrotype replay\0".to_vec(),
        ]
        .concat(),
    );
    let vmhd = full(b"vmhd", 0, 1, &[0u8; 8]);
    let dref = full(
        b"dref",
        0,
        0,
        &[u32s(&[1]), full(b"url ", 0, 1, &[])].concat(),
    );
    let dinf = bx(b"dinf", &dref);

    // The av01 sample entry: the visual sample entry fields, then av1C and
    // the colour (BT.709, limited range).
    let mut entry = vec![0u8; 6];
    entry.extend_from_slice(&1u16.to_be_bytes());
    entry.extend_from_slice(&[0u8; 16]);
    entry.extend_from_slice(&(s.width as u16).to_be_bytes());
    entry.extend_from_slice(&(s.height as u16).to_be_bytes());
    entry.extend_from_slice(&u32s(&[0x0048_0000, 0x0048_0000, 0]));
    entry.extend_from_slice(&1u16.to_be_bytes());
    let mut name = [0u8; 32];
    name[0] = 4;
    name[1..5].copy_from_slice(b"AV1 ");
    entry.extend_from_slice(&name);
    entry.extend_from_slice(&0x18u16.to_be_bytes());
    entry.extend_from_slice(&(-1i16).to_be_bytes());
    entry.extend_from_slice(&bx(b"av1C", av1c));
    entry.extend_from_slice(&bx(
        b"colr",
        &[b"nclx".as_slice(), &[0, 1, 0, 1, 0, 1, 0]].concat(),
    ));
    let stsd = full(b"stsd", 0, 0, &[u32s(&[1]), bx(b"av01", &entry)].concat());

    let stts = full(b"stts", 0, 0, &u32s(&[1, n, 1]));
    let keys: Vec<u32> = (1..=n).filter(|&i| samples[i as usize - 1].1).collect();
    let stss = full(
        b"stss",
        0,
        0,
        &[u32s(&[keys.len() as u32]), u32s(&keys)].concat(),
    );
    let stsc = full(b"stsc", 0, 0, &u32s(&[1, 1, n, 1]));
    let sizes: Vec<u32> = samples.iter().map(|(d, _)| d.len() as u32).collect();
    let stsz = full(b"stsz", 0, 0, &[u32s(&[0, n]), u32s(&sizes)].concat());
    // The chunk offset depends on moov's own size: build it with a
    // placeholder, then again with the real offset (same size).
    let build = |offset: u32| {
        let stco = full(b"stco", 0, 0, &u32s(&[1, offset]));
        let stbl = bx(
            b"stbl",
            &[
                stsd.clone(),
                stts.clone(),
                stss.clone(),
                stsc.clone(),
                stsz.clone(),
                stco,
            ]
            .concat(),
        );
        let minf = bx(b"minf", &[vmhd.clone(), dinf.clone(), stbl].concat());
        let mdia = bx(b"mdia", &[mdhd.clone(), hdlr.clone(), minf].concat());
        let trak = bx(b"trak", &[tkhd.clone(), mdia].concat());
        bx(b"moov", &[mvhd.clone(), trak].concat())
    };
    let moov_len = build(0).len();
    let data_start = (ftyp.len() + moov_len + 8) as u32;
    let moov = build(data_start);
    let payload: usize = samples.iter().map(|(d, _)| d.len()).sum();
    let mut out = Vec::with_capacity(data_start as usize + payload);
    out.extend_from_slice(&ftyp);
    out.extend_from_slice(&moov);
    out.extend_from_slice(&((payload + 8) as u32).to_be_bytes());
    out.extend_from_slice(b"mdat");
    for (d, _) in samples {
        out.extend_from_slice(d);
    }
    out
}

/// Lay `text` over the bottom left of an sRGB frame: white with a soft
/// dark band behind it, `px` high.
pub fn caption(rgb: &mut [u8], width: usize, height: usize, text: &str, px: f32) {
    if text.trim().is_empty() {
        return;
    }
    let (cov, w, h) = crate::watermark::set_text(crate::fonts::DEFAULT, text, px);
    let pad = (px * 0.6) as usize;
    let (x0, y0) = (pad, height.saturating_sub(h + pad));
    // The band: darken the rows behind the text.
    let band_top = y0.saturating_sub(pad / 2);
    for y in band_top..height {
        for x in 0..width.min(x0 + w + pad) {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                rgb[i + c] = (f32::from(rgb[i + c]) * 0.55) as u8;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let a = cov[y * w + x];
            if a <= 0.0 || x0 + x >= width || y0 + y >= height {
                continue;
            }
            let i = ((y0 + y) * width + x0 + x) * 3;
            for c in 0..3 {
                rgb[i + c] = (f32::from(rgb[i + c]) + (255.0 - f32::from(rgb[i + c])) * a) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_an_mp4_that_holds_every_frame() {
        let s = VideoSettings {
            width: 64,
            height: 48,
            fps: 10,
            ..Default::default()
        };
        let mut enc = Encoder::new(s).unwrap();
        for f in 0..12u8 {
            let rgb: Vec<u8> = (0..64 * 48)
                .flat_map(|i| [(i % 64) as u8 * 4, f * 20, 128])
                .collect();
            enc.push(Frame { rgb: &rgb }).unwrap();
        }
        let path = std::env::temp_dir().join(format!("amb-video-{}.mp4", std::process::id()));
        let size = enc.finish(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, size);
        assert_eq!(&bytes[4..8], b"ftyp");
        let find = |t: &[u8]| bytes.windows(4).any(|w| w == t);
        assert!(find(b"moov") && find(b"av01") && find(b"av1C") && find(b"mdat"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn temporal_delimiters_are_dropped() {
        // A temporal delimiter (type 2, size 0), then a frame OBU (type 6) of 2 bytes.
        let data = [0x12, 0x00, 0x32, 0x02, 0xaa, 0xbb];
        assert_eq!(
            without_temporal_delimiters(&data),
            vec![0x32, 0x02, 0xaa, 0xbb]
        );
    }
}
