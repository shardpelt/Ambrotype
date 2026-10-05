//! Lightroom / Camera Raw preset and sidecar reader.
//!
//! Parses `.xmp` presets, XMP sidecars and legacy `.lrtemplate` presets into one
//! lossless model: a sparse map of Camera Raw setting names (the `crs:` local
//! names, e.g. `Exposure2012`) to [`Value`]s. Absent keys mean "not touched by
//! this preset", which is different from "set to the default".

pub mod amount;
pub mod lrtemplate;
pub mod registry;
pub mod report;
pub mod value;
pub mod xmp;
pub mod xmp_writer;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

pub use value::{Value, curve_points};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("XML error: {0}")]
    Xml(String),
    #[error("not an XMP packet (no rdf:Description found)")]
    NotXmp,
    #[error("lrtemplate parse error at line {line}: {msg}")]
    Lua { line: usize, msg: String },
    #[error("lrtemplate has no value.settings table")]
    NoSettings,
    #[error("file is not valid UTF-8 text")]
    NotText,
    #[error("unrecognised file format")]
    UnknownFormat,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceFormat {
    /// XMP develop preset (`crs:PresetType` or preset header fields present).
    XmpPreset,
    /// XMP without preset header fields: a sidecar or embedded develop settings.
    XmpSidecar,
    /// Legacy Lua-table `.lrtemplate`.
    Lrtemplate,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub format: SourceFormat,
    /// Camera Raw settings keyed by `crs:` local name, in sorted order.
    pub settings: BTreeMap<String, Value>,
}

/// Header fields that identify an XMP file as a preset rather than a sidecar.
const PRESET_MARKERS: &[&str] = &["PresetType", "SupportsAmount", "Group", "Cluster"];

impl Preset {
    /// Parse text, detecting the format from its content.
    pub fn parse(text: &str) -> Result<Preset, Error> {
        let trimmed = text.trim_start_matches('\u{feff}').trim_start();
        if trimmed.starts_with('<') {
            Self::parse_xmp(text)
        } else if trimmed.starts_with("s =")
            || trimmed.starts_with("s=")
            || trimmed.starts_with("return")
            || trimmed.starts_with('{')
            || trimmed.starts_with("--")
        {
            Self::parse_lrtemplate(text)
        } else {
            Err(Error::UnknownFormat)
        }
    }

    pub fn parse_xmp(text: &str) -> Result<Preset, Error> {
        let settings = xmp::parse_crs(text)?;
        let is_preset = PRESET_MARKERS.iter().any(|k| settings.contains_key(*k));
        Ok(Preset {
            format: if is_preset {
                SourceFormat::XmpPreset
            } else {
                SourceFormat::XmpSidecar
            },
            settings,
        })
    }

    pub fn parse_lrtemplate(text: &str) -> Result<Preset, Error> {
        let top = lrtemplate::parse(text)?;
        let top = top.as_struct().ok_or(Error::NoSettings)?;
        let value = top
            .get("value")
            .and_then(Value::as_struct)
            .ok_or(Error::NoSettings)?;
        let mut settings = value
            .get("settings")
            .and_then(Value::as_struct)
            .cloned()
            .ok_or(Error::NoSettings)?;
        // Carry the header into the same keys an XMP preset uses.
        let title = top.get("title").or_else(|| top.get("internalName"));
        if let (Some(title), false) = (title, settings.contains_key("Name")) {
            settings.insert("Name".into(), title.clone());
        }
        let uuid = value.get("uuid").or_else(|| top.get("id"));
        if let (Some(uuid), false) = (uuid, settings.contains_key("UUID")) {
            settings.insert("UUID".into(), uuid.clone());
        }
        Ok(Preset {
            format: SourceFormat::Lrtemplate,
            settings,
        })
    }

    /// Read and parse a file. XMP packets embedded in other files (DNG, JPEG)
    /// are located by their `<x:xmpmeta` marker.
    pub fn from_path(path: &Path) -> Result<Preset, Error> {
        let bytes = std::fs::read(path)?;
        if let Ok(text) = std::str::from_utf8(&bytes) {
            return Self::parse(text);
        }
        if let Some(packet) = find_embedded_xmp(&bytes) {
            return Self::parse_xmp(packet);
        }
        Err(Error::NotText)
    }

    /// Serialise the settings as an XMP packet (usable as a sidecar or preset).
    pub fn to_xmp(&self) -> String {
        xmp_writer::write_packet(&self.settings, is_signed_tag)
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.settings.get(name)
    }

    pub fn name(&self) -> Option<&str> {
        self.get("Name").and_then(Value::display_text)
    }

    pub fn group(&self) -> Option<&str> {
        self.get("Group").and_then(Value::display_text)
    }

    pub fn uuid(&self) -> Option<&str> {
        self.get("UUID").and_then(Value::as_str)
    }

    pub fn process_version(&self) -> Option<&str> {
        self.get("ProcessVersion").and_then(Value::as_str)
    }

    /// Whether the preset declares support for the Amount slider. Adobe only
    /// shows the slider when `SupportsAmount` is true.
    pub fn supports_amount(&self) -> bool {
        self.get("SupportsAmount")
            .or_else(|| self.get("SupportsAmount2"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

/// Whether a tag's range includes negative values (Camera Raw writes `+` on those).
pub fn is_signed_tag(name: &str) -> bool {
    registry::Registry::builtin()
        .lookup(name)
        .and_then(|i| i.min)
        .is_some_and(|min| min < 0.0)
}

/// Locate an XMP packet inside binary data (e.g. a DNG "preset" file sold for
/// Lightroom mobile, or a JPEG with embedded develop settings).
pub fn find_embedded_xmp(bytes: &[u8]) -> Option<&str> {
    const START: &[u8] = b"<x:xmpmeta";
    const END: &[u8] = b"</x:xmpmeta>";
    let start = bytes.windows(START.len()).position(|w| w == START)?;
    let len = bytes[start..].windows(END.len()).position(|w| w == END)? + END.len();
    std::str::from_utf8(&bytes[start..start + len]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        let xmp = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
 <rdf:Description crs:PresetType="Normal" crs:SupportsAmount="True" crs:Exposure2012="0.5"/></rdf:RDF>"#;
        let p = Preset::parse(xmp).unwrap();
        assert_eq!(p.format, SourceFormat::XmpPreset);
        assert!(p.supports_amount());

        let sidecar = xmp.replace(r#"crs:PresetType="Normal" crs:SupportsAmount="True" "#, "");
        assert_eq!(
            Preset::parse(&sidecar).unwrap().format,
            SourceFormat::XmpSidecar
        );

        let lua =
            r#"s = { title = "T", value = { settings = { Exposure2012 = 0.5 }, uuid = "U" } }"#;
        let p = Preset::parse(lua).unwrap();
        assert_eq!(p.format, SourceFormat::Lrtemplate);
        assert_eq!(p.name(), Some("T"));
        assert_eq!(p.uuid(), Some("U"));
    }

    #[test]
    fn finds_embedded_packet() {
        let mut bytes = vec![0xffu8, 0xd8, 0x00];
        bytes.extend_from_slice(br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"><rdf:Description crs:Vibrance="12"/></rdf:RDF></x:xmpmeta>"#);
        bytes.extend_from_slice(&[0x00, 0xff]);
        let packet = find_embedded_xmp(&bytes).unwrap();
        let p = Preset::parse_xmp(packet).unwrap();
        assert_eq!(p.get("Vibrance").unwrap().as_f64(), Some(12.0));
    }
}
