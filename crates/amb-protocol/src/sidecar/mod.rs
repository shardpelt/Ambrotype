// SPDX-License-Identifier: GPL-3.0-or-later
//! Writing XMP sidecars without losing what we do not own.
//!
//! A Lightroom sidecar also carries rating, label, keywords, EXIF/TIFF copies
//! and edit history in other namespaces. The `amb-presets` reader keeps only
//! Camera Raw (`crs:`) properties, so re-writing a sidecar from parsed settings
//! alone would silently drop them. [`merge`] writes the new `crs:` settings and
//! carries every other property of the original over verbatim.
//!
//! The photo's rating, colour label and keywords ([`PhotoInfo`]) are the
//! exception: they live in Lightroom's own places (`xmp:Rating`, `xmp:Label`,
//! `dc:subject`, and
//! `lr:hierarchicalSubject` for keyword paths) and [`merge_with_info`] can
//! replace them, so Lightroom and other apps read what Ambrotype writes.
//!
//! Pick and reject flags have no standard place (Lightroom keeps them in its
//! catalogue), so they are ours: `amblib:Flag` in [`LIBRARY_NS`]. A rejected
//! photo without stars also gets `xmp:Rating="-1"`, the XMP way to say
//! rejected (Bridge and darktable read it), and that is read as a reject.
//!
//! The edit history and snapshots are ours too (Lightroom keeps its own in
//! the catalogue): `amblib:History` and `amblib:Snapshots`, JSON text written
//! by [`DevelopState`](crate::DevelopState), read back by [`read_history`]
//! and [`read_snapshots`].

mod read;
#[cfg(test)]
mod tests;
mod write;

pub use read::{read_history, read_info, read_snapshots};
pub use write::{Stored, merge, merge_with_info, merge_with_stored, with_info, write_atomic};

pub(super) const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
pub(super) const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
pub(super) const LR_NS: &str = "http://ns.adobe.com/lightroom/1.0/";
pub(super) const EXIF_NS: &str = "http://ns.adobe.com/exif/1.0/";
/// Ambrotype's library namespace: photo state that isn't a develop setting.
pub const LIBRARY_NS: &str = "http://ns.ambrotype.org/library/1.0/";

/// Lightroom's pick and reject flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    Pick,
    Reject,
}

impl Flag {
    pub fn name(self) -> &'static str {
        match self {
            Flag::Pick => "pick",
            Flag::Reject => "reject",
        }
    }

    pub fn parse(s: &str) -> Option<Flag> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pick" | "picked" => Some(Flag::Pick),
            "reject" | "rejected" => Some(Flag::Reject),
            _ => None,
        }
    }
}

/// A photo's rating, colour label and keywords, as stored in its sidecar.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PhotoInfo {
    /// 1..=5 stars; `None` (or 0 in Lightroom's terms) is unrated.
    pub rating: Option<u8>,
    /// In the order given; duplicates (ignoring case) are dropped when writing.
    pub keywords: Vec<String>,
    /// Colour label as Lightroom and Bridge write it: "Red", "Yellow",
    /// "Green", "Blue" or "Purple" (any text is kept).
    #[serde(default)]
    pub label: Option<String>,
    /// Pick or reject.
    #[serde(default)]
    pub flag: Option<Flag>,
    /// Title, caption, creator and copyright, as Lightroom's Metadata panel
    /// keeps them (`dc:title`, `dc:description`, `dc:creator`, `dc:rights`).
    /// Writing, `None` keeps what the file has and `""` removes it, so code
    /// that only knows the rating can't lose them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Several creators are separated by "; ".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
    /// The stack the photo is in (`amblib:Stack`, `amblib:StackTop`: our
    /// own, as Lightroom keeps stacks in its catalogue). Writing, `None`
    /// keeps what the file has and an empty id takes it out of its stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<StackRef>,
    /// Where the photo was taken, as "latitude,longitude" in decimal
    /// degrees, when the sidecar says (`exif:GPSLatitude` and
    /// `exif:GPSLongitude`, as Lightroom writes a place set on its map).
    /// Writing, `None` keeps what the file has and `""` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gps: Option<String>,
    /// How the photo is turned (EXIF 1–8, `tiff:Orientation`, as Lightroom
    /// writes a photo turned in its library); `None` when the sidecar
    /// doesn't say, so the camera's holds. Writing, `None` keeps what the
    /// file has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<u16>,
    /// The place's names (`photoshop:City`, `photoshop:State`,
    /// `photoshop:Country`, `Iptc4xmpCore:CountryCode`), as Lightroom and
    /// the IPTC standard keep them. Writing, `None` keeps what the file has
    /// and `""` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
}

/// A photo's place in a stack: the stack's id, and whether it is the photo
/// the stack shows when collapsed.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StackRef {
    pub id: String,
    #[serde(default)]
    pub top: bool,
}

/// The Dublin Core properties [`PhotoInfo`]'s text fields are kept in, and
/// whether each is a list (`rdf:Seq`) rather than a text (`rdf:Alt`).
pub(super) const DC_TEXTS: [(&str, bool); 4] = [
    ("title", false),
    ("description", false),
    ("creator", true),
    ("rights", false),
];

impl PhotoInfo {
    /// Trimmed, non-empty, unique keywords and a rating in range.
    pub fn cleaned(&self) -> PhotoInfo {
        let mut seen = std::collections::HashSet::new();
        let keywords = self
            .keywords
            .iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty() && seen.insert(k.to_lowercase()))
            .collect();
        PhotoInfo {
            rating: self.rating.filter(|r| (1..=5).contains(r)),
            keywords,
            label: self
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            flag: self.flag,
            title: self.title.as_deref().map(|t| t.trim().to_string()),
            caption: self.caption.as_deref().map(|t| t.trim().to_string()),
            creator: self.creator.as_deref().map(|t| t.trim().to_string()),
            copyright: self.copyright.as_deref().map(|t| t.trim().to_string()),
            stack: self.stack.clone(),
            gps: self.gps.as_deref().map(|g| match parse_place(g) {
                Some((lat, lon)) => format!("{lat:.6},{lon:.6}"),
                None => String::new(),
            }),
            orientation: self.orientation.filter(|o| (1..=8).contains(o)),
            city: self.city.as_deref().map(|t| t.trim().to_string()),
            state: self.state.as_deref().map(|t| t.trim().to_string()),
            country: self.country.as_deref().map(|t| t.trim().to_string()),
            country_code: self.country_code.as_deref().map(|t| t.trim().to_string()),
        }
    }

    /// The place, as (latitude, longitude) in degrees.
    pub fn place(&self) -> Option<(f64, f64)> {
        parse_place(self.gps.as_deref()?)
    }

    /// The text field kept in `dc:<local>`.
    fn text_mut(&mut self, local: &str) -> &mut Option<String> {
        match local {
            "title" => &mut self.title,
            "description" => &mut self.caption,
            "creator" => &mut self.creator,
            _ => &mut self.copyright,
        }
    }
}

/// "latitude,longitude" in decimal degrees, in range.
pub fn parse_place(s: &str) -> Option<(f64, f64)> {
    let (lat, lon) = s.split_once(',')?;
    let (lat, lon) = (
        lat.trim().parse::<f64>().ok()?,
        lon.trim().parse::<f64>().ok()?,
    );
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// XMP's EXIF GPS coordinate, "DDD,MM.mmmmK" or "DDD,MM,SSK" (K is N, S, E
/// or W), as signed degrees.
pub(super) fn parse_xmp_coordinate(s: &str) -> Option<f64> {
    let s = s.trim();
    let sign = match s.chars().last()? {
        'N' | 'E' | 'n' | 'e' => 1.0,
        'S' | 'W' | 's' | 'w' => -1.0,
        _ => return None,
    };
    let parts: Vec<f64> = s[..s.len() - 1]
        .split(',')
        .map(|p| p.trim().parse::<f64>().ok())
        .collect::<Option<_>>()?;
    let deg = match parts.as_slice() {
        [d] => *d,
        [d, m] => d + m / 60.0,
        [d, m, sec] => d + m / 60.0 + sec / 3600.0,
        _ => return None,
    };
    Some(sign * deg)
}

/// Signed degrees as XMP's "DDD,MM.mmmmmmK".
pub(super) fn xmp_coordinate(v: f64, pos: char, neg: char) -> String {
    let a = v.abs();
    let d = a.trunc();
    format!(
        "{},{:.6}{}",
        d as i64,
        (a - d) * 60.0,
        if v < 0.0 { neg } else { pos }
    )
}

/// A rating as stored: stars, or -1 for rejected.
pub(super) fn parse_rating(v: &str) -> (Option<u8>, bool) {
    let r = v.trim().parse::<f64>().ok().map(|r| r.round() as i64);
    (r.and_then(|r| u8::try_from(r).ok()), r == Some(-1))
}
