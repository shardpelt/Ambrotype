// SPDX-License-Identifier: GPL-3.0-or-later
//! Smart search: photos and words turned into vectors that sit close
//! together when they mean the same thing (CLIP and SigLIP style models), so
//! the library can be searched by what is in the photos ("dog on a beach")
//! and for photos like another one.
//!
//! A search model is three files: the image half (`<id>.onnx`), the text
//! half and its tokenizer (`ModelSpec::files`). The text half reads ids from
//! a byte-pair tokenizer, written here for the two kinds the listed models
//! use: CLIP's (lower case, words, byte-level symbols) and Gemma's
//! (SigLIP 2: the whole string, with a byte fallback).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

use crate::{ModelSpec, file_of};

/// How a search model reads text and photos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchSpec {
    /// `clip` or `gemma`: the tokenizer's kind.
    pub tokenizer: String,
    /// Tokens the text half takes (padded or cut to this).
    pub length: usize,
    /// Lower-case the text first.
    #[serde(default)]
    pub lowercase: bool,
    /// Which output is the photo's vector, and which the text's.
    #[serde(default)]
    pub image_output: usize,
    #[serde(default)]
    pub text_output: usize,
    /// `crop`: the short side to the input size, then the middle square;
    /// `squash`: the whole photo to a square.
    #[serde(default = "crop")]
    pub fit: String,
}

fn crop() -> String {
    "crop".into()
}

type Model = std::sync::Arc<TypedRunnableModel>;

/// A loaded search model.
pub struct Embedder {
    pub spec: ModelSpec,
    image: Model,
    text: Model,
    tokens: Bpe,
}

impl Embedder {
    /// Load the installed model `spec` from `dir`.
    pub fn load(dir: &Path, spec: &ModelSpec) -> Result<Embedder, String> {
        let s = spec.search.as_ref().ok_or("not a search model")?;
        let size = spec.input.size;
        let named = |name: &str| dir.join(format!("{}.{name}", spec.id));
        let image = load_fixed(&file_of(dir, &spec.id), f32::fact([1, 3, size, size]))?;
        let text = load_fixed(&named("text.onnx"), i64::fact([1, s.length]))?;
        let tokens = Bpe::load(&named("tokenizer.json"), &s.tokenizer)?;
        Ok(Embedder {
            spec: spec.clone(),
            image,
            text,
            tokens,
        })
    }

    fn search(&self) -> &SearchSpec {
        self.spec.search.as_ref().expect("checked in load")
    }

    /// The vector of a photo: `rgb`, sRGB 0–1 interleaved, `w` × `h`.
    pub fn image(&self, rgb: &[f32], w: usize, h: usize) -> Result<Vec<f32>, String> {
        let size = self.spec.input.size;
        let (mean, std) = (self.spec.input.mean, self.spec.input.std);
        let square = fit(rgb, w, h, size, self.search().fit == "crop");
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, size, size), |(_, c, y, x)| {
            (square[(y * size + x) * 3 + c] - mean[c]) / std[c]
        });
        let out = self
            .image
            .run(tvec!(input.into_tensor().into_tvalue()))
            .map_err(|e| format!("{}: {e}", self.spec.name))?;
        vector(&out, self.search().image_output)
    }

    /// The vector of a search's words.
    pub fn text(&self, query: &str) -> Result<Vec<f32>, String> {
        let s = self.search();
        let ids = self.tokens.ids(query, s.lowercase, s.length);
        let input =
            tract_ndarray::Array2::from_shape_vec((1, s.length), ids).map_err(|e| e.to_string())?;
        let out = self
            .text
            .run(tvec!(input.into_tensor().into_tvalue()))
            .map_err(|e| format!("{}: {e}", self.spec.name))?;
        vector(&out, s.text_output)
    }
}

/// How alike two vectors from [`Embedder`] are: −1 to 1 (they're unit length).
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Load a model with fixed input sizes. Exported models carry the shapes
/// of their inner values with named sizes (`batch_size`, `height`) that
/// can't be matched with fixed ones, so those are dropped and worked out
/// again from the input.
fn load_fixed(path: &Path, input: TypedFact) -> Result<Model, String> {
    let name = path.display();
    let mut model = tract_onnx::onnx()
        .model_for_path(path)
        .map_err(|e| format!("{name}: {e}"))?;
    let inputs: Vec<usize> = model
        .input_outlets()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|o| o.node)
        .collect();
    for node in model.nodes.iter_mut() {
        if inputs.contains(&node.id) || node.op_is::<tract_onnx::tract_hir::ops::konst::Const>() {
            continue;
        }
        for o in node.outputs.iter_mut() {
            o.fact = InferenceFact::default();
        }
    }
    model
        .with_input_fact(0, input.into())
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("{name} didn't load: {e}"))
}

/// Output `i` of a run as a unit-length vector.
fn vector(out: &TVec<TValue>, i: usize) -> Result<Vec<f32>, String> {
    let t = out.get(i).ok_or("the model gave no vector")?;
    let v: Vec<f32> = t
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .copied()
        .collect();
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    Ok(v.iter().map(|x| x / n).collect())
}

/// The photo as a `size` × `size` square (sRGB 0–1, interleaved): the middle
/// of it after the short side is scaled to `size` (`crop`), or all of it.
/// Each output pixel averages the input pixels it covers.
fn fit(rgb: &[f32], w: usize, h: usize, size: usize, crop: bool) -> Vec<f32> {
    let (w, h) = (w.max(1), h.max(1));
    let (sx, sy, ox, oy) = if crop {
        let side = w.min(h) as f64;
        (side, side, (w as f64 - side) / 2.0, (h as f64 - side) / 2.0)
    } else {
        (w as f64, h as f64, 0.0, 0.0)
    };
    let (fx, fy) = (sx / size as f64, sy / size as f64);
    let mut out = vec![0.0; size * size * 3];
    for y in 0..size {
        let y0 = (oy + y as f64 * fy) as usize;
        let y1 = ((oy + (y + 1) as f64 * fy).ceil() as usize).clamp(y0 + 1, h);
        for x in 0..size {
            let x0 = (ox + x as f64 * fx) as usize;
            let x1 = ((ox + (x + 1) as f64 * fx).ceil() as usize).clamp(x0 + 1, w);
            let mut sum = [0.0f32; 3];
            for yy in y0.min(h - 1)..y1 {
                for xx in x0.min(w - 1)..x1 {
                    for c in 0..3 {
                        sum[c] += rgb[(yy * w + xx) * 3 + c];
                    }
                }
            }
            let n = ((y1 - y0.min(h - 1)) * (x1 - x0.min(w - 1))).max(1) as f32;
            for c in 0..3 {
                out[(y * size + x) * 3 + c] = sum[c] / n;
            }
        }
    }
    out
}

/// A byte-pair tokenizer from a `tokenizer.json` (vocabulary and merges).
struct Bpe {
    kind: Kind,
    vocab: HashMap<String, i64>,
    /// (left, right) → (rank, merged token).
    merges: HashMap<(i64, i64), (u32, i64)>,
    /// Start, end and padding ids.
    bos: Option<i64>,
    eos: i64,
    pad: i64,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Words, byte-level symbols, `</w>` on each word's last symbol.
    Clip,
    /// The whole string, `▁` for spaces, unknown characters as bytes.
    Gemma,
}

impl Bpe {
    fn load(path: &Path, kind: &str) -> Result<Bpe, String> {
        let kind = match kind {
            "clip" => Kind::Clip,
            "gemma" => Kind::Gemma,
            other => return Err(format!("unknown tokenizer {other:?}")),
        };
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let model = &json["model"];
        let vocab: HashMap<String, i64> = model["vocab"]
            .as_object()
            .ok_or("tokenizer.json has no vocabulary")?
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_i64()?)))
            .collect();
        let mut merges = HashMap::new();
        for (rank, m) in model["merges"]
            .as_array()
            .ok_or("tokenizer.json has no merges")?
            .iter()
            .enumerate()
        {
            // "a b" (CLIP) or ["a", "b"] (Gemma).
            let (a, b) = match m {
                serde_json::Value::String(s) => match s.split_once(' ') {
                    Some((a, b)) => (a.to_string(), b.to_string()),
                    None => continue,
                },
                serde_json::Value::Array(p) if p.len() == 2 => (
                    p[0].as_str().unwrap_or_default().to_string(),
                    p[1].as_str().unwrap_or_default().to_string(),
                ),
                _ => continue,
            };
            if let (Some(&l), Some(&r), Some(&m)) =
                (vocab.get(&a), vocab.get(&b), vocab.get(&format!("{a}{b}")))
            {
                merges.entry((l, r)).or_insert((rank as u32, m));
            }
        }
        let id = |t: &str| vocab.get(t).copied();
        let (bos, eos, pad) = match kind {
            Kind::Clip => {
                let end = id("<|endoftext|>").ok_or("no <|endoftext|>")?;
                (id("<|startoftext|>"), end, end)
            }
            Kind::Gemma => (
                None,
                id("<eos>").ok_or("no <eos>")?,
                id("<pad>").ok_or("no <pad>")?,
            ),
        };
        Ok(Bpe {
            kind,
            vocab,
            merges,
            bos,
            eos,
            pad,
        })
    }

    /// The ids for `text`, `length` of them (cut, then padded).
    fn ids(&self, text: &str, lowercase: bool, length: usize) -> Vec<i64> {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if lowercase || self.kind == Kind::Clip {
            text.to_lowercase()
        } else {
            text
        };
        let mut ids: Vec<i64> = self.bos.into_iter().collect();
        match self.kind {
            Kind::Clip => {
                for word in clip_words(&text) {
                    let mut syms: Vec<String> =
                        word.bytes().map(byte_char).map(String::from).collect();
                    if let Some(last) = syms.last_mut() {
                        last.push_str("</w>");
                    }
                    let syms: Vec<i64> = syms
                        .iter()
                        .filter_map(|s| self.vocab.get(s).copied())
                        .collect();
                    ids.extend(self.merge(syms));
                }
            }
            Kind::Gemma => {
                let text = text.replace(' ', "\u{2581}");
                let mut syms = Vec::new();
                for ch in text.chars() {
                    match self.vocab.get(ch.encode_utf8(&mut [0; 4]) as &str) {
                        Some(&i) => syms.push(i),
                        // Byte fallback: <0xNN> for each byte of the character.
                        None => syms.extend(
                            ch.to_string()
                                .bytes()
                                .filter_map(|b| self.vocab.get(&format!("<0x{b:02X}>")).copied()),
                        ),
                    }
                }
                ids.extend(self.merge(syms));
            }
        }
        ids.truncate(length.saturating_sub(1));
        ids.push(self.eos);
        ids.resize(length, self.pad);
        ids
    }

    /// Merge adjacent symbols, best-ranked pair first, until none merges.
    fn merge(&self, mut syms: Vec<i64>) -> Vec<i64> {
        loop {
            let best = syms
                .windows(2)
                .enumerate()
                .filter_map(|(i, p)| {
                    self.merges
                        .get(&(p[0], p[1]))
                        .map(|&(rank, m)| (rank, i, m))
                })
                .min();
            match best {
                Some((_, i, m)) => {
                    syms[i] = m;
                    syms.remove(i + 1);
                }
                None => return syms,
            }
        }
    }
}

/// CLIP's split into words: letters, single digits, runs of other
/// non-space characters, and English contractions.
fn clip_words(text: &str) -> Vec<String> {
    static WORDS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = WORDS.get_or_init(|| {
        regex::Regex::new(r"'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+")
            .expect("valid")
    });
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

/// GPT-2's bytes-to-characters table: printable bytes stay themselves, the
/// rest move to 256 and up, so every byte is a visible character.
fn byte_char(b: u8) -> char {
    let printable = |b: u8| (33..=126).contains(&b) || (161..=172).contains(&b) || b >= 174;
    if printable(b) {
        return b as char;
    }
    let n = (0..b).filter(|&x| !printable(x)).count() as u32;
    char::from_u32(256 + n).expect("in range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bpe(kind: Kind, vocab: &[&str], merges: &[(&str, &str)]) -> Bpe {
        let vocab: HashMap<String, i64> = vocab
            .iter()
            .enumerate()
            .map(|(i, t)| (t.to_string(), i as i64))
            .collect();
        let mut m = HashMap::new();
        for (rank, (a, b)) in merges.iter().enumerate() {
            m.insert(
                (vocab[*a], vocab[*b]),
                (rank as u32, vocab[&format!("{a}{b}")]),
            );
        }
        Bpe {
            kind,
            bos: vocab.get("<|startoftext|>").copied(),
            eos: 0,
            pad: 0,
            vocab,
            merges: m,
        }
    }

    #[test]
    fn clip_words_merge_to_whole_tokens() {
        let t = bpe(
            Kind::Clip,
            &[
                "<|endoftext|>",
                "<|startoftext|>",
                "d",
                "o",
                "g</w>",
                "do",
                "dog</w>",
                "a</w>",
                "!</w>",
            ],
            &[("d", "o"), ("do", "g</w>")],
        );
        // "A dog!" → start, a, dog, !, end, padding.
        assert_eq!(t.ids("A  dog!", true, 8), vec![1, 7, 6, 8, 0, 0, 0, 0]);
        assert_eq!(
            t.ids("dog dog dog", true, 3),
            vec![1, 6, 0],
            "cut, with the end kept"
        );
    }

    #[test]
    fn gemma_falls_back_to_bytes() {
        let t = bpe(
            Kind::Gemma,
            &[
                "<pad>",
                "<eos>",
                "a",
                "\u{2581}",
                "b",
                "a\u{2581}",
                "a\u{2581}b",
                "<0xC3>",
                "<0xA9>",
            ],
            &[("a", "\u{2581}"), ("a\u{2581}", "b")],
        );
        let t = Bpe {
            eos: 1,
            pad: 0,
            ..t
        };
        assert_eq!(t.ids("a b é", false, 7), vec![6, 3, 7, 8, 1, 0, 0]);
    }

    #[test]
    fn bytes_map_like_gpt2() {
        assert_eq!(byte_char(b'a'), 'a');
        assert_eq!(byte_char(0), '\u{100}');
        assert_eq!(byte_char(b' '), '\u{120}');
    }

    #[test]
    fn the_square_is_the_middle_or_the_whole() {
        // 4×2, left half black, right half white.
        let rgb: Vec<f32> = (0..8)
            .flat_map(|i| [if i % 4 < 2 { 0.0 } else { 1.0 }; 3])
            .collect();
        let c = fit(&rgb, 4, 2, 2, true);
        assert_eq!((c[0], c[3]), (0.0, 1.0), "the middle two columns");
        let s = fit(&rgb, 4, 2, 2, false);
        assert_eq!((s[0], s[3]), (0.0, 1.0));
    }
}
