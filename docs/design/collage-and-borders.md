# Borders and the collage maker

Status: agreed direction (2026-10-07), nothing built yet. Steps 5–6 of the UI track in [NEXT.md](../NEXT.md). The size presets they use (Instagram, Snapchat, print, custom) are being built with the crop tool.

## Borders

*Built (2026-10-08): `crates/amb-export/src/border.rs` (layout, colours, compositing), border presets in the export dialog (`ui/borders.js`, built-ins in `crates/amb-gui/data/border-presets.json`, yours in the settings file), `amb render --border 5%,#fff` or `--border preset.json`, and batches with `amb render A.nef B.nef … --out-dir DIR`. Captions and EXIF lines aren't built yet.*

![Export with a border: Instagram 4:5, the whole photo inside a Polaroid-style frame](../images/export-border.png)

A border is an export option, saved and shared as a **border preset**.

- **Settings:**
  - width as a percentage of the long or short edge, or in pixels; separate inner and outer widths
  - colour, or "sampled from the photo" (average or dominant colour)
  - corner radius
  - an optional caption or EXIF line (font, size, position, text template such as `{camera} · {focal} · f/{aperture} · {shutter}`)
- **Fit to a size preset:** for example "Instagram portrait 1080×1350 with a 4% white border". The photo is scaled to fit inside the border, and the rest is filled with the border colour. This gives the common "white frame around a landscape photo on a portrait post" look without cropping.
- **Where it applies:** in `amb-export`, after resize and output sharpening, so it is the same in the GUI, the CLI (`amb render --border preset.json`) and batch export.
- **Preview:** shown in the export dialog, and optionally in the viewer as a soft-proof overlay.

## Collage maker

A separate canvas mode with its own panel set (a layout in the sense of [customization-and-hub.md](customization-and-hub.md)).

### Template (the shareable part)

```jsonc
{
  "format": "ambrotype-collage/1",
  "name": "Three-up story",
  "canvas": { "size": "preset:instagram-story", "background": "#ffffff" },
  "gap": 0.015, "outer": 0.03, "radius": 0.0,          // fractions of the canvas short edge
  "cells": [
    { "rect": [0, 0, 1, 0.5] },                         // x, y, w, h in canvas fractions
    { "rect": [0, 0.5, 0.5, 0.5] },
    { "rect": [0.5, 0.5, 0.5, 0.5], "shape": "circle" } // or a polygon: "points": [[x,y],...]
  ]
}
```

- **Contents:** the canvas size (a size preset or custom W×H), the cells, gaps, outer margin, corner radius, background and an optional border preset.
- **No photos:** templates never contain photos, so they are small, private and shareable through the hub.
- **Built-ins:** grids (2×2, 3×3), strips, a "hero plus thumbnails" layout, film-strip and contact sheet.
- **Editor:** drag cell edges to resize neighbours together; split or merge cells; snap to thirds and to equal sizes.

### Per photo on the canvas

- Each cell holds one photo and its **own develop edit**: the normal develop panels apply to the selected cell, using the same protocol commands. It's a virtual copy, so the photo's own sidecar is untouched.
- **Pan, zoom and rotate inside the cell** (cut is the cell mask), plus free rotation and flipping.
- Swap photos by dragging them between cells. Drop a folder on the canvas to fill the cells in order.
- An optional per-cell border and shadow.
- **The collage document** (template + photo paths + per-cell transforms and edits) is saved as its own small file next to the photos, so a collage can be reopened and re-exported.

### Rendering and export

- Each cell is rendered by the engine at the size it needs on the final canvas (never upscaled from a preview), then composited.
- Export goes through `amb-export` (formats, colour spaces, sharpening, metadata).
- **CLI:** `amb collage render template.json photo1.nef photo2.nef … out.jpg`.

### Batch mode (big folders)

- Choose a template and a folder: photos fill the cells N at a time, giving pages 1, 2, 3, ….
- **Order:** by capture time, by name, or "best first" once ratings exist.
- **Placement:** portrait and landscape photos go to cells of the matching orientation when the template has both; a face- or subject-aware default crop comes later, with masks.
- **Preview:** a page grid shows the result, where you can swap photos between pages before exporting.
- **Background job:** export runs as a background job with progress, and resumes if interrupted.
- The same batch idea applies to borders: "apply this border preset to every photo in the folder at Instagram portrait size".

## Open questions

- Text and stickers on the canvas: in scope for v1, or only the border caption?
- Templates with mixed cell sizes but a fixed aspect per cell (for example "always 4:5"), so batch-filled photos crop predictably.
- Print layouts (contact sheets, picture packages) overlap with Lightroom's Print module; decide whether this is the same feature.
