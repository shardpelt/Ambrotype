//! Collage templates for the editor: the built-ins (`data/collage-templates.json`,
//! read-only) and the user's own (kept in the settings file), plus rendering a
//! collage: each photo developed with its own edits, placed, then exported.

use amb_export::collage::{Canvas, Placement, Template};
use serde::{Deserialize, Serialize};
use std::path::Path;

const BUILTIN_JSON: &str = include_str!("../data/collage-templates.json");
/// The canvas a built-in starts with (Instagram portrait); the editor can change it.
const DEFAULT_SIZE: (u32, u32) = (1080, 1350);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateEntry {
    /// `builtin:<n>` or `custom:<n>`; empty for a new one.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(flatten)]
    pub template: Template,
}

#[derive(Deserialize)]
struct BuiltinFile {
    templates: Vec<BuiltinTemplate>,
}

#[derive(Deserialize)]
struct BuiltinTemplate {
    name: String,
    cells: Vec<[f64; 4]>,
}

pub fn builtin() -> Vec<TemplateEntry> {
    let file: BuiltinFile =
        serde_json::from_str(BUILTIN_JSON).expect("data/collage-templates.json is valid");
    file.templates
        .into_iter()
        .enumerate()
        .map(|(i, t)| TemplateEntry {
            id: format!("builtin:{}", i + 1),
            builtin: true,
            template: Template {
                format: amb_export::collage::FORMAT.into(),
                name: t.name,
                width: DEFAULT_SIZE.0,
                height: DEFAULT_SIZE.1,
                background: "#ffffff".into(),
                gap: 1.5,
                margin: 3.0,
                radius: 0.0,
                cells: t.cells,
            },
        })
        .collect()
}

pub fn all(custom: &[TemplateEntry]) -> Vec<TemplateEntry> {
    let mut out = builtin();
    out.extend(
        custom
            .iter()
            .filter(|t| t.id.starts_with("custom:") && t.template.validate().is_ok())
            .cloned(),
    );
    out
}

/// Add a template of the user's (or update one with the same id). Returns its id.
pub fn upsert(custom: &mut Vec<TemplateEntry>, t: &TemplateEntry) -> Result<String, String> {
    let mut t = t.clone();
    t.template.name = t.template.name.trim().to_string();
    if t.template.name.is_empty() {
        return Err("a template needs a name".into());
    }
    t.template.validate()?;
    t.builtin = false;
    if let Some(existing) = custom.iter_mut().find(|c| !t.id.is_empty() && c.id == t.id) {
        *existing = t.clone();
        return Ok(t.id);
    }
    let next = custom
        .iter()
        .filter_map(|c| c.id.strip_prefix("custom:")?.parse::<u64>().ok())
        .max()
        .map_or(1, |n| n + 1);
    t.id = format!("custom:{next}");
    custom.push(t.clone());
    Ok(t.id)
}

pub fn remove(custom: &mut Vec<TemplateEntry>, id: &str) -> Result<(), String> {
    if id.starts_with("builtin:") {
        return Err("built-in templates can't be deleted".into());
    }
    let before = custom.len();
    custom.retain(|c| c.id != id);
    if custom.len() == before {
        return Err(format!("no template {id}"));
    }
    Ok(())
}

/// One cell's photo and how it sits.
#[derive(Debug, Clone, Deserialize)]
pub struct CellPhoto {
    pub path: Option<String>,
    #[serde(default)]
    pub placement: Placement,
}

/// Render a collage to `out`: each photo developed with its sidecar edits
/// (one at a time), then the canvas exported with `settings` (no resize).
/// `progress(done, total)` is called after each photo.
pub fn render(
    template: &Template,
    cells: &[CellPhoto],
    out: &Path,
    settings: &amb_export::ExportSettings,
    lens: &crate::LensSources,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<amb_export::ExportReport, String> {
    template.validate()?;
    let mut canvas = Canvas::new(template);
    let total = cells.iter().filter(|c| c.path.is_some()).count();
    let mut done = 0;
    for (i, cell) in cells.iter().enumerate().take(template.cells.len()) {
        let Some(path) = &cell.path else { continue };
        let mut s = crate::session::Session::open(Path::new(path))?;
        let photo = s.render_upright(lens);
        canvas.place(i, &photo, &cell.placement);
        done += 1;
        progress(done, total);
    }
    let mut settings = settings.clone();
    settings.resize = amb_export::Resize::None;
    settings.border = None;
    amb_export::export(&canvas.image, &Default::default(), &settings, out)
        .map_err(|e| format!("{}: {e}", out.display()))
}

/// Many photos as pages of one layout: `dir/<layout name>-001.<ext>`, ...
/// Photos are matched to cells by shape (`photos` are path and width/height
/// of the upright photo), each placed with the default fit.
/// `progress(page, pages, done, total)` is called after each photo.
pub fn render_batch(
    template: &Template,
    photos: &[(String, f64)],
    dir: &Path,
    settings: &amb_export::ExportSettings,
    lens: &crate::LensSources,
    progress: &mut dyn FnMut(usize, usize, usize, usize),
) -> Result<Vec<std::path::PathBuf>, String> {
    template.validate()?;
    let aspects: Vec<f64> = photos
        .iter()
        .map(|(_, a)| if a.is_finite() && *a > 0.0 { *a } else { 1.5 })
        .collect();
    let pages = amb_export::collage::pages(template, &aspects);
    let stem: String = template
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "collage" } else { stem };
    let mut written = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let cells: Vec<CellPhoto> = page
            .iter()
            .map(|slot| CellPhoto {
                path: slot.map(|n| photos[n].0.clone()),
                placement: Placement::default(),
            })
            .collect();
        let out = dir.join(format!(
            "{stem}-{:03}.{}",
            i + 1,
            settings.format.extension()
        ));
        render(
            template,
            &cells,
            &out,
            settings,
            lens,
            &mut |done, total| progress(i + 1, pages.len(), done, total),
        )?;
        written.push(out);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid() {
        let b = builtin();
        assert!(b.len() >= 10);
        for t in &b {
            t.template
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", t.template.name));
            // Cells don't overlap (beyond rounding).
            for (i, a) in t.template.cells.iter().enumerate() {
                for c in &t.template.cells[i + 1..] {
                    let ox = (a[0] + a[2]).min(c[0] + c[2]) - a[0].max(c[0]);
                    let oy = (a[1] + a[3]).min(c[1] + c[3]) - a[1].max(c[1]);
                    assert!(
                        ox <= 1e-3 || oy <= 1e-3,
                        "{} cells overlap",
                        t.template.name
                    );
                }
            }
        }
    }

    #[test]
    fn renders_a_collage_with_edits() {
        let dir = std::env::temp_dir().join(format!("amb-collage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("ramp.dng");
        amb_testkit::dng::write(
            &photo,
            &amb_testkit::patterns::generate("ramp").unwrap(),
            &Default::default(),
        )
        .unwrap();
        let mut t = builtin()[0].template.clone(); // two side by side
        t.width = 400;
        t.height = 200;
        let cells = vec![
            CellPhoto {
                path: Some(photo.display().to_string()),
                placement: Placement::default(),
            },
            CellPhoto {
                path: None,
                placement: Placement::default(),
            },
        ];
        let out = dir.join("collage.png");
        let mut calls = Vec::new();
        let r = render(
            &t,
            &cells,
            &out,
            &amb_export::ExportSettings::new(amb_export::Format::Png),
            &crate::LensSources::default(),
            &mut |d, n| calls.push((d, n)),
        )
        .unwrap();
        assert_eq!((r.width, r.height), (400, 200));
        assert_eq!(calls, [(1, 1)]);
        let img = image::open(&out).unwrap().to_rgb8();
        assert_eq!(
            img.get_pixel(300, 100).0,
            [255, 255, 255],
            "the empty cell shows the background"
        );

        // Three photos in a two-cell layout: two pages, the second half empty.
        t.name = "Side by side".into();
        let photos: Vec<(String, f64)> =
            (0..3).map(|_| (photo.display().to_string(), 1.5)).collect();
        let mut seen = Vec::new();
        let pages = render_batch(
            &t,
            &photos,
            &dir,
            &amb_export::ExportSettings::new(amb_export::Format::Png),
            &crate::LensSources::default(),
            &mut |p, n, d, total| seen.push((p, n, d, total)),
        )
        .unwrap();
        let names: Vec<_> = pages
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, ["Side-by-side-001.png", "Side-by-side-002.png"]);
        assert_eq!(seen, [(1, 2, 1, 2), (1, 2, 2, 2), (2, 2, 1, 1)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
