// SPDX-License-Identifier: GPL-3.0-or-later
//! What a model is: its inputs, outputs and files, and the list of models to install.

use crate::{archive, search};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Input {
    /// The square side the model takes, in pixels.
    pub size: usize,
    /// Colour normalisation: `(v / brightest − mean) / std`, per channel.
    pub mean: [f32; 3],
    pub std: [f32; 3],
    /// How the photo is laid out: colour planes first (`nchw`, PyTorch
    /// models) or colour last per pixel (`nhwc`, TensorFlow/MediaPipe ones).
    #[serde(default, skip_serializing_if = "Layout::is_nchw")]
    pub layout: Layout,
    /// What the pixels are: numbers (`float`, normalised as above) or
    /// bytes (`bytes`, 0–255 in and out; for a remove model the mask is
    /// 255 where the photo is kept, as MI-GAN's).
    #[serde(default, skip_serializing_if = "Pixels::is_float")]
    pub pixels: Pixels,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Pixels {
    #[default]
    Float,
    Bytes,
}

impl Pixels {
    fn is_float(&self) -> bool {
        *self == Pixels::Float
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Nchw,
    Nhwc,
}

impl Layout {
    fn is_nchw(&self) -> bool {
        *self == Layout::Nchw
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Activation {
    /// The output is used as it is (then stretched to 0–1).
    #[default]
    None,
    /// A logistic function first (models that give logits).
    Sigmoid,
    /// Several maps (one per class) turned into shares that add up to 1
    /// at each pixel; used as they are, not stretched.
    Softmax,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Output {
    #[serde(default)]
    pub activation: Activation,
    /// For a model that gives several maps (one per class), the one to use,
    /// from 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<usize>,
    /// Use the opposite of the map: a background class, inverted, is
    /// everything in front of it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

/// A model in the list, or one of the user's own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub task: String,
    pub name: String,
    /// The task's model when none was chosen.
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub about: String,
    pub licence: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub source: String,
    /// Anything else to know before using it (for example a dataset's terms).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The file's size; 0 when unknown (the user's own).
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    /// Seconds per photo on a 4-core laptop CPU; for the user's own, as
    /// measured when it was added (on their computer). 0 when unknown.
    #[serde(default)]
    pub seconds: f64,
    /// How fine the edges should be, 1 (soft) to 5 (finest); 0 when unknown.
    #[serde(default)]
    pub detail: u8,
    pub input: Input,
    #[serde(default)]
    pub output: Output,
    /// Added by the user ([`add_custom`]), not from the list.
    #[serde(default)]
    pub custom: bool,
    /// More files it needs beside `<id>.onnx` (a search model's text half
    /// and tokenizer), kept as `<id>.<name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ModelFile>,
    /// For a search model: how it reads text and photos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<search::SearchSpec>,
    /// Published only inside a zip: that is downloaded, and the files taken
    /// out of it by name (each still checked against its own sha256).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<archive::Archive>,
}

/// One more file of a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub urls: Vec<String>,
    /// Its name inside the model's archive, when it has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub member: String,
}

impl ModelSpec {
    /// Bytes to download, all files together.
    pub fn total_bytes(&self) -> u64 {
        self.bytes + self.files.iter().map(|f| f.bytes).sum::<u64>()
    }
}

/// A model as [`list`] shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ModelState {
    #[serde(flatten)]
    pub spec: ModelSpec,
    pub installed: bool,
    /// The one its task uses now.
    pub chosen: bool,
    /// Bytes to download, all its files together.
    pub total_bytes: u64,
}

/// What the user says about their own model ([`add_custom`]).
#[derive(Debug, Clone, Deserialize)]
pub struct CustomModel {
    pub task: String,
    pub name: String,
    pub size: usize,
    #[serde(default = "imagenet_mean")]
    pub mean: [f32; 3],
    #[serde(default = "imagenet_std")]
    pub std: [f32; 3],
    #[serde(default)]
    pub activation: Activation,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub channel: Option<usize>,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub licence: String,
    #[serde(default)]
    pub source: String,
}

fn imagenet_mean() -> [f32; 3] {
    [0.485, 0.456, 0.406]
}

fn imagenet_std() -> [f32; 3] {
    [0.229, 0.224, 0.225]
}

#[derive(Deserialize)]
struct Catalogue {
    models: Vec<ModelSpec>,
}

/// The models we can install.
pub fn catalogue() -> Vec<ModelSpec> {
    let c: Catalogue =
        serde_json::from_str(include_str!("../data/models.json")).expect("models.json is valid");
    c.models
}
