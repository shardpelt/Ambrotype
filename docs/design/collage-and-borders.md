# Borders and the collage maker

Status: borders built (2026-10-08), the collage maker built (2026-10-08); open items are listed under each. Steps 5–6 of the UI track in [NEXT.md](../NEXT.md). Both use the size presets (Instagram, Snapchat, print, custom) from the crop tool.

## Borders

*Built (2026-10-08): `crates/amb-export/src/border.rs` (layout, colours, compositing), border presets in the export dialog (`ui/borders.js`, built-ins in `crates/amb-gui/data/border-presets.json`, yours in the settings file), `amb render --border 5%,#fff` or `--border preset.json`, and batches with `amb render A.nef B.nef … --out-dir DIR`. Captions (2026-10-13): a line of text in the bottom band, from a template filled in with the photo's camera, lens, exposure, date, title or creator (`{camera} · {exposure}`); the export dialog's Caption field (previewed with the photo's own details) and `amb render --border-caption`. One line. Fonts (2026-10-15): Sans (FreeSans Bold), Handwritten (Caveat), Pen (Reenie Beanie), Marker (Permanent Marker), Typewriter (Special Elite) and Date stamp (DSEG7), for captions and text watermarks (`amb-export/src/fonts.rs`). The editor's Frame panel (Shift+B) previews a border and text around the photo, outside the edit; on export it becomes the Border ("Frame") and, for text on the photo, the watermark.*

![Export with a border: Instagram 4:5, the whole photo inside a Polaroid-style frame](../images/export-border.png)

A border is an export option, saved and shared as a **border preset**. Since 2026-10-22 the built-ins start with White, White with a black line (a keyline around the photo) and Polaroid (a deeper bottom), and a border is shared as a file (Share… and Open… in the export dialog) or installed from the hub (`borders/` in the hub repo).

- **Settings:**
  - width as a percentage of the long or short edge, or in pixels; separate inner and outer widths
  - colour, or "sampled from the photo" (average or dominant colour)
  - corner radius
  - an optional caption or EXIF line (size, alignment, colour, font, text template such as `{camera} · {focal} · {aperture} · {shutter}`; built)
- **Fit to a size preset:** for example "Instagram portrait 1080×1350 with a 4% white border". The photo is scaled to fit inside the border, and the rest is filled with the border colour. This gives the common "white frame around a landscape photo on a portrait post" look without cropping.
- **Where it applies:** in `amb-export`, after resize and output sharpening, so it is the same in the GUI, the CLI (`amb render --border preset.json`) and batch export.
- **Preview:** shown in the export dialog, and optionally in the viewer as a soft-proof overlay.

## Watermarks

Status: built 2026-10-12 (`crates/amb-export/src/watermark.rs`).

- Text, or a PNG logo with its own transparency, on the photo: after resizing and output sharpening (so it isn't sharpened), before the border (so it sits on the photo, not the frame).
- Size is the mark's width as a percentage of the photo's width, so it looks the same at any export size. Nine places; the inset is a percentage of the short edge. Opacity, and for text a colour and a soft shadow so it reads on light and dark parts.
- Text is set in GNU FreeSans Bold, bundled (GPL-3.0-or-later with the font exception).
- In the export dialog (Watermark, saved with export presets) and `amb render --watermark "© Name" | logo.png | watermark.json` with `--watermark-at`, `--watermark-size`, `--watermark-opacity`.
- Rotation (any angle) and tiling across the photo (rows offset by half a step) since 2026-10-12: `rotate`, `tile`; `--watermark-rotate`, `--watermark-tile`; the export dialog's Rotate and Repeat across the photo.
- Not yet: other fonts.

## Collage maker

*Built (2026-10-08):*
- *the layout format, compositing and page planner: `crates/amb-export/src/collage.rs`*
- *from the command line (2026-10-13): `amb collage <layout> <photos>... --out page.jpg` (a built-in by name or number, `amb collage list`, or a layout .json; `--size`, `--background`, `--gap`, `--margin` and the export options), pages when there are more photos than cells*
- *rendering one collage or a batch of pages, and the layouts list: `crates/amb-gui/src/collages.rs`; built-in layouts in `crates/amb-export/data/collage-templates.json` (`collage::builtin_templates`), yours in the settings file*
- *the editor screen: `crates/amb-gui/ui/collage.js` (the Collage button, or C)*

![The collage maker: layouts on the left, the canvas, canvas and cell settings on the right, and the photo tray below](../images/collage.png)

How it works:
- **Photos** come from a tray under the canvas: the open photo's folder, plus any photos or folders you add. Drag one into a cell, or click it to fill the selected (or next empty) cell. **Fill empty cells** puts tray photos in order. The ⠿ grip swaps two cells.
- **In a cell:** drag to move the photo, scroll to zoom (around the pointer), double-click to fit again. **Rotate** (R) turns it a quarter; **Empty** (Delete) takes it out.
- **Edit photo** opens it in the editor with all its tools; **← Back to collage** returns, showing the new edit. The collage uses each photo's own edit (its sidecar), so there is one edit per photo, not one per collage.
- **The shape:** pick a layout, then split or remove cells. A built-in layout changes only the shape; your own layouts bring their size, gaps and background too.
- **Canvas:** size (any size preset, or your own width × height), background colour, gap between photos, outer border and rounded corners.
- **Export** renders every photo from its raw at the size its cell needs, composites in linear light and exports through `amb-export`. JPEG, PNG, TIFF, WebP or AVIF, in sRGB, Display P3 or Adobe RGB.
- **Batch** puts every tray photo (or only those with enough stars) into pages of the layout: `<layout name>-001.jpg`, `-002`, …. Landscape photos go to wide cells and portrait photos to tall cells where the layout has both; a short last page is filled in order.
- **Sharing:** Save layout keeps it in your list; Share… saves it as a small file (no photos); Import… adds one.
- **Arrange freely** (built 2026-10-08) turns the layout into frames placed by hand, like layers in GIMP:
  - drag a frame to move it, its corners to resize it (Shift keeps the shape), and the round handle to turn it to any angle (Shift: 15° steps); or type the angle
  - frames may overlap: **To front** / **To back** (`]` / `[`) stack them; arrow keys nudge (Shift: 10 px)
  - **Move frames / Move photos inside** chooses what a drag moves (Alt swaps it for one drag); inside a frame, scroll zooms and R turns the photo a quarter, as in a grid
  - drop a photo on empty canvas for a new frame there, shaped like the photo; **Add frame** adds an empty one
  - rounded corners apply to every frame, turned ones too; frames can hang over the edge and are cut off there, as in the export
  - **Back to a grid layout** returns to the grid it came from (or a fresh grid if frames were added)
- **Collage files** (built 2026-10-08): **Save** / **Save as…** (Ctrl+S, Ctrl+Shift+S) write the whole collage to a `.ambcollage` file: the layout, the photo in each cell and how it sits, the tray and the export choices. **Open…** (Ctrl+O), dropping the file on the window, or `ambrotype summer.ambcollage` reopens it to edit and export again. Export has "Also save the collage file next to it" (on by default), so every exported image has its re-editable collage beside it. Photos are referred to, not copied: by full path and by a path relative to the file, so a folder holding both can be moved; missing photos are named when it opens.

![Arranged freely: overlapping frames with rounded corners, one turned 14.8°, with its handles](../images/collage-free.png)

### Layout format (the shareable part)

```json
{ "format": "ambrotype-collage/1", "name": "Hero and two",
  "width": 1080, "height": 1350, "background": "#ffffff",
  "gap": 1.5, "margin": 3, "radius": 0,
  "cells": [ [0, 0, 1, 0.6], [0, 0.6, 0.5, 0.4], [0.5, 0.6, 0.5, 0.4] ] }
```

- `gap`, `margin` (the outer border) and `radius` are percentages of the canvas's short edge, so a layout looks the same at any size.
- A cell is `[x, y, width, height]` as fractions of the area inside the margin. Gaps are taken off the edges cells share, so neighbours stay `gap` apart. Up to 64 cells.
- **Free layouts** (`"free": true`) place cells on the whole canvas instead (no margin or gaps). Cells may overlap (later ones on top) and stick out of the canvas, and a fifth number turns one: `[x, y, width, height, degrees clockwise]`. They are saved and shared like any layout.
- **No photos:** layouts never contain photos, so they are small, private and shareable.
- **Built-ins:** two side by side or stacked, three columns or rows, grids 2 × 2 and 3 × 3, hero on top with two or three below, hero left with two right, big and three small, a mosaic of five, a film strip of four.

### Still to come

- In grids: drag cell edges to resize neighbours together, merge cells, snap to thirds. Round or polygon cells. Snapping and guides when arranging freely.
- Undo inside the collage maker (today: Back to a grid layout, or reopen the saved file).
- A per-cell edit separate from the photo's own (virtual copies), flipping, per-cell borders and shadows.
- The collage stored inside exported images (so a JPEG reopens as its collage); today the `.ambcollage` file sits next to it.
- Batch: ordering by capture time, a page grid to swap photos between pages before exporting, running in the background, and shapes that follow a photo's crop (batch uses the camera preview's shape).
- The hub for collage layouts, and `amb collage render` in the CLI.

### Original plan (for reference)

#### Per photo on the canvas

- Each cell holds one photo and its **own develop edit**: the normal develop panels apply to the selected cell, using the same protocol commands. It's a virtual copy, so the photo's own sidecar is untouched.
- **Pan, zoom and rotate inside the cell** (cut is the cell mask), plus free rotation and flipping.
- Swap photos by dragging them between cells. Drop a folder on the canvas to fill the cells in order.
- An optional per-cell border and shadow.
- **The collage document** (template + photo paths + per-cell transforms and edits) is saved as its own small file next to the photos, so a collage can be reopened and re-exported.

#### Rendering and export

- Each cell is rendered by the engine at the size it needs on the final canvas (never upscaled from a preview), then composited.
- Export goes through `amb-export` (formats, colour spaces, sharpening, metadata).
- **CLI:** `amb collage render template.json photo1.nef photo2.nef … out.jpg`.

#### Batch mode (big folders)

- Choose a template and a folder: photos fill the cells N at a time, giving pages 1, 2, 3, ….
- **Order:** by capture time, by name, or "best first" once ratings exist.
- **Placement:** portrait and landscape photos go to cells of the matching orientation when the template has both; a face- or subject-aware default crop comes later, with masks.
- **Preview:** a page grid shows the result, where you can swap photos between pages before exporting.
- **Background job:** export runs as a background job with progress, and resumes if interrupted.
- **By groups (built 2026-10-23):** **Group alike** in the tray puts alike photos and bursts together, best first (the library's Duplicates finder, looser: up to 10 of 64 fingerprint bits may differ, bursts within 2 s; exact copies count once). Each group's **2 × 2** button lays its best four into the square 2 × 2 grid on a square canvas, where you drag in each square to line its photo up and scroll to zoom. **One page per group** in the batch makes a page from each group's best photos (`amb_export::collage::group_pages`); groups smaller than the layout are left out.
- The same batch idea applies to borders: "apply this border preset to every photo in the folder at Instagram portrait size".

## Open questions

- Text and stickers on the canvas: in scope for v1, or only the border caption?
- Templates with mixed cell sizes but a fixed aspect per cell (for example "always 4:5"), so batch-filled photos crop predictably.
- Print layouts (contact sheets, picture packages) overlap with Lightroom's Print module; decide whether this is the same feature.
