# Architecture as built (October 2026)

The target architecture is in [SCOPE.md §9](SCOPE.md). This page describes what exists today.

**Short version:**
- Ambrotype is a Rust **library-first** project. The engine, preset reader, lens database and export are libraries.
- There are two front ends: the `amb` command-line tool and an early desktop editor (`amb-gui`, Tauri 2).
- The GUI, the MCP server for AI assistants (`amb mcp`) and the CLI are all clients of the same libraries, through one command protocol, like FFmpeg/libmpv and their players.

## Crates

About 23,000 lines of Rust and about 270 tests (fmt, clippy and tests run in CI on Linux, Windows, macOS and Windows on ARM).

| Crate | Role | Main parts |
|---|---|---|
| `amb-presets` | Read and write settings | XMP (`crs:` Adobe tags plus our `amb:` tags), `.lrtemplate`, XMP embedded in DNG/JPEG; the tag registry (`data/tags.json`); preset Amount scaling; the mask model (`masks.rs`: Lightroom's correction tags, slider units, parts) and spots (`retouch.rs`) |
| `amb-protocol` | Typed edit commands for GUI/CLI/AI | `set/adjust/reset_param`, `apply_preset`, mask commands (`add/set/paint/remove/duplicate/move_mask`, `list_masks`), spot commands (`add/set/remove_spot`, `list_spots`), snapshots (`save/apply/rename/delete_snapshot`, `list_snapshots`), undo/redo, queries, JSON Schema; sidecar merge that keeps foreign metadata, with the edit history and snapshots stored in it (`amblib:History`, `amblib:Snapshots`); the AI settings reference |
| `amb-color` | Colour science | Temperature/Tint ↔ xy, matrices, Bradford adaptation, output colour spaces and generated ICC profiles |
| `amb-lens` | Lens profiles | lensfun database and Adobe LCP readers, lens matching, interpolation, correction geometry |
| `amb-engine` | Raw decode and develop | The pipeline below; every model constant is in `Calibration` (JSON, fitted later against Lightroom) |
| `amb-ai` | Local AI models | Subject, Background and People masks: a list of freely licensed models to install (`data/models.json`), the user's choice per task, and their own ONNX models; run with `tract` on all cores, downloaded from their authors and checked by sha256, results kept per photo fingerprint; registered with `amb_engine::subject` by the editor and `amb` ([design/masks.md](design/masks.md)) |
| `amb-gpu` | GPU preview | The develop pipeline's stages as wgpu compute shaders, run from an `amb_engine::plan::Plan`; stages without a kernel run on the CPU in between; equivalence tests against the CPU pipeline ([design/gpu.md](design/gpu.md)) |
| `amb-export` | Files out | JPEG/PNG/TIFF/WebP/AVIF/EXR/JPEG XL, colour spaces, resize, output sharpening, EXIF, JPEG size limit; floating-point linear DNG for merged photos (`dng.rs`) |
| `amb-library` | Library operations | The catalogue (`catalog.rs`: a SQLite index in the config folder of what was read from each photo's sidecar and raw header, checked against file sizes and modified times, so it can't go stale and can be deleted; `amb catalog`) and which files in a folder are photos (`folder.rs`). Import: copy from a card or folder into dated folders, renamed from templates, with companions and duplicate checks (`amb import`, the editor's Import dialog); collections as small JSON files; moving from Lightroom Classic (`lrcat.rs`: reads a `.lrcat` read-only and immutable with SQLite, writes each photo's sidecar from the catalogue's develop settings, XMP and library tables, and its collections as ours; `amb lrcat import`, the library's From Lightroom…); HDR merge into a DNG next to the brackets and panorama merge with lens corrections first (`merge.rs`, over `amb_engine::merge` and `amb_engine::pano`; `amb merge hdr` / `amb merge pano`, the library's Merge to HDR… / Merge to Panorama…; [design/hdr-merge.md](design/hdr-merge.md), [design/panorama.md](design/panorama.md)) |
| `amb-testkit` | Calibration tooling | Synthetic DNG targets (incl. a scanned negative), the Lightroom slider-sweep kit |
| `amb-cli` | The `amb` tool | `render`, `import`, `merge hdr`, `merge pano`, `mcp` (the MCP server, `src/mcp.rs`), `preset`, `edit`/`query`, `do` (the compact edit syntax), `stats`, `match` (look matching, `amb_engine::matching`), `ref` (the AI core card), `lens`, `negative`, `sweep`, `settings reference`, … |
| `amb-gui` | Desktop editor (Tauri 2) | Rust `Session` (open raw, undoable edits via `amb-protocol`, downscaled preview + histogram, sidecar save, export) behind Tauri commands; plain HTML/CSS/JS UI generated from the settings registry |

## Data flow of `amb render photo.nef out.jpg --preset look.xmp`

```
raw file ──rawler──▶ CameraImage (camera RGB, white = 1, per-channel clip levels, + metadata: lens, EXIF, orientation;
                     a DNG's gain maps applied, its warp and vignette opcodes kept as the photo's own lens correction)
         (rawler decodes; we demosaic: RCD for Bayer in amb-engine/src/rcd.rs, our Markesteijn port for X-Trans in amb-engine/src/xtrans.rs)
         (then partly clipped highlights rebuilt from their other channels: amb-engine/src/highlights.rs)
JPEG/PNG/TIFF/WebP ──raster.rs──▶ CameraImage too: the image's RGB made linear by its ICC profile
         (sRGB if none), `rendered` set: relative white balance, no base tone curve, no default sharpening
preset/sidecar ──amb-presets──▶ settings map (crs:/amb: tags) ──▶ DevelopSettings
                                       │ (lens profile resolved via amb-lens)
                                       ▼
amb-engine pipeline (linear float RGB, rayon-parallel passes over one buffer):
  0  Lens Corrections (vignetting, CA, distortion), Upright + Transform, crop: one resampling pass
     (Upright found from the photo's lines on a ≤1024 px lens-corrected copy, upright.rs)
     → film negative conversion (optional)
  1  clipped channels → neutral, white balance + camera matrix (× Calibration primaries) → linear ProPhoto
     (a camera profile, profile.rs: its matrices, hue/sat map, exposure offset, look table)
     → Exposure → Calibration shadow tint → Defringe
     → spot removal (heal and clone, retouch.rs) → masks drawn (alpha per mask, mask.rs)
     → local Exposure, Temp, Tint
  2  Dehaze (then local Dehaze)
  3  base look (or the camera profile's tone curve) → Highlights/Shadows (local: guided-filter base) → Contrast, Whites, Blacks (global curve)
     → local tone sliders
  4  Texture / Clarity (then local Texture, Clarity, Saturation, Color)
  5  Vibrance, Saturation, HSL (or the B&W mix), Color Grading → parametric + point curves
     → a .cube LUT (amb:LUT, lut.rs)
  6  noise reduction → sharpening (then local Noise, Sharpness)
  7  post-crop vignette → grain → mask overlay (editor) → paint blank lens areas
                                       ▼
amb-export: orient upright → resize (Lanczos-3, linear) → output colour space
            → transfer curve → output sharpening → 8/16/32-bit → encoder + ICC + EXIF
```

`AMB_TIMING=1` prints each render stage's time to stderr (e.g. `render 3495 ms: dehaze 363, highlights/shadows 393, …`).

Speed today: a 24 MP raw with a typical edit renders in about 3.5 s on a 4-core CPU, plus about 2.5 s to decode it and write a full-size JPEG. Most of that is the CPU pipeline. The editor's preview can run on the GPU (`amb-gpu`, [design/gpu.md](design/gpu.md)); exports and the 100% view stay on the CPU.

**Region renders** (`pipeline::render_region`, for the editor's 100% view) develop only a rectangle of the output, with a margin as wide as the edit's filters reach (each spatial stage reports its reach: `local::reach`, `detail::noise_reduction_reach` and so on), started on a multiple of 64 pixels so subsampled filters see the same blocks. Every stage works on a `buffer::Tile`: it knows where its buffer sits in the full output, so the vignette, grain, masks and spots are placed and filter radii scaled as in the full render. A spot writing into the region pulls in what it reads (its ring and its donor, `retouch::footprints`). What the full render measures over the whole photo (`pipeline::Globals`: a film negative's levels, the haze estimates, the colours under colour range samples) comes from `measure_globals`, a render of a 1600 px copy (colour samples from small full-size renders around them). The region then matches the same part of the full render to about 1e-6, except: haze and negative levels come from the small copy (a fraction of a percent), and noise reduction measures the noise on the region. A 24 MP 100% view takes about 3 s in a debug build, against about 33 s for the full render it replaces. While a slider moves, the 100% view shows a draft: the same window rendered from a reduced copy (about half a megapixel) with the measurements of the crop on screen, enlarged until the full-size crop arrives (`Session::region_draft`).

## Export (amb-export)

| Format | Bit depths | Colour profile | EXIF | Notes |
|---|---|---|---|---|
| JPEG | 8 | ICC | yes | quality, 4:4:4/4:2:2/4:2:0 (auto 4:4:4 at q≥90), progressive, optimised Huffman, **file-size limit** |
| PNG | 8, 16 | ICC | yes | |
| TIFF | 8, 16, 32 float | ICC | yes (Exif and GPS directories) | LZW, with the horizontal predictor for 8 and 16-bit (`tiff_file.rs`) |
| WebP | 8 | ICC | yes | lossless only (lossy needs libwebp) |
| AVIF | 8 | sRGB, Display P3, Rec. 2020 (CICP codes, no ICC) | yes | quality, speed |
| OpenEXR | 32 float | linear, unclipped | n/a | for compositing/HDR work |

- **Colour spaces:** sRGB, Display P3, Adobe RGB (1998)-compatible, ProPhoto RGB and Rec. 2020.
  - The ICC profiles are generated by us, so no profile files are shipped. They're checked with LittleCMS.
- **Resize to fit:** long edge, short edge, width, height, W×H, megapixels or percent. "Don't enlarge" is the default.
- **Output sharpening:** for screen, matte or glossy paper, at low, standard or high.
- **Metadata:** all, all except location, copyright only, or none. Exports are rotated upright and tagged orientation 1.
- **Not planned:**
  - HEIC/HEIF, because of HEVC patent licensing; AVIF covers modern HDR-capable delivery.
  - PSD; TIFF covers the Photoshop round trip.
- **Later:**
  - Lossy JPEG XL, and JPEG XL with an ICC profile and metadata (lossless sRGB JPEG XL exists).
  - Lossy WebP.
  - HDR AVIF/JXL.
  - DNG of the developed image.
  - Text watermarks in other fonts.
  - Export presets in the CLI (the editor has them).
  - Gamut mapping, instead of the current hard clip.

## The desktop editor (amb-gui, v1)

- **Shell:** Tauri 2, with the system webview (WebView2, WKWebView, WebKitGTK). The UI in `crates/amb-gui/ui/` is plain HTML/CSS/JS with no build step or npm. The window opens where and as big as it was left (`tauri-plugin-window-state`).
- **Where things are:**
  - Rust: `src/main.rs` holds the app state, saved settings and startup, and registers every command in `invoke_handler`. The Tauri commands themselves are in `src/commands/`, one file per area: `photo`, `preview`, `export`, `presets`, `appearance`, `settings`, `assistant`, `collage`, `hub`, `plugins`, `library` and `catalog`. The logic they call is in the other `src/` files (`session.rs`, `looks.rs`, `hub.rs`, …).
  - JS: `ui/app.js` holds the shared state (`ui`), the slider rows and `init`, which wires the buttons. The preview loop and 1:1 view are in `preview.js`, the crop panel in `croptool.js`, the Presets panel in `presets.js` and the Export dialog in `export.js`; the other panels and dialogs each have their own file.
- **One code path:** the UI only sends `amb-protocol` commands (`set_param`, `apply_preset`, `undo`, …), so it behaves exactly like the CLI and AI clients. Panels and sliders are generated from the settings registry, so they always match the engine.
- **Preview:** rendered on the CPU from an area-downscaled copy of the raw, at screen size. Slider drags render with temporary overrides and don't touch history; release commits one undoable step. Requests are latest-wins, and a drag renders at 0.6× size.
- **Features:**
  - open (dialog, drag-and-drop, command line)
  - Basic, Tone Curve (parametric), Color Mixer (HSL tabs), Black & White, Color Grading, Detail, Lens Corrections, Transform, Effects, Calibration and Film Negative panels
  - presets list (Lightroom's own folder when present, or added folders) with an Amount slider
  - history, undo/redo (a drag of a curve point or colour wheel, or a WB pick, is one step), before/after, histogram
  - 1:1 view with panning: only the tiles (256 px) the view needs are rendered (`render_region`), and kept until the committed settings change, so panning back is instant. White-balance picker, clipping overlay, point-curve editor (RGB and per channel), colour-grading wheels, crop and straighten tool (draw a crop corner to corner like a screenshot, then adjust; aspect lock, size presets), mask tool (`ui/masks.js`: linear, radial, brush, luminance and colour range masks with add/subtract/intersect and local sliders, drawn on the photo through `mask_frame`'s map; [design/masks.md](design/masks.md)), spot removal (`ui/spots.js`: heal and clone spots with a suggested donor, `spot_source`; [design/spot-removal.md](design/spot-removal.md))
  - size presets (built-in for social sites and print, plus custom) for the crop aspect and exact-size export
  - **looks:** the toolbar, side columns and panels come from a layout file and the colours from a theme (`src/looks.rs` validates and stores them; `ui/layout.js` renders; `ui/controls.js` has the knob, bar, number, toggle and segmented controls; `ui/editor.js` is editor mode, which edits a draft of the layout and re-renders it)
  - **hub:** `src/hub.rs` reads a hub's `index.json` (https, or file:// for testing), checks each look file against its sha256, validates it and saves it as one of the user's looks; it also builds the GitHub link for submitting a look. `ui/hub.js` is the Browse dialog, with previews drawn from the looks' own data. The hub repo is `Ambrotypehq/ambrotype-hub`; the app's built-in looks and starter presets are a copy of some of its items in `data/hub/` (`tools/sync-hub.py`), and the hub shows those as built in
  - **settings and keyboard shortcuts:** `ui/keys.js` has one table of actions with default keys; it runs the shortcuts, writes the toolbar tooltips and drives the Settings dialog (keyboard map, rebinding, clash checks). Only the user's changes are stored (`src/keys.rs`, in the settings file). `ui/menu.js` builds the menu bar from the same table (`MENU_BAR`); `src/commands/menu.rs` puts it up (macOS always; Windows and Linux when Settings → General asks, kept in the webview's storage) and sends a chosen item back as a `menu` event. Built-ins are in `data/looks/`; the user's own are files in the config folder. Format: [ai/ui-layouts.md](ai/ui-layouts.md)
  - rating (keys 1–5), pick/reject flags (P, X, U; `amblib:Flag`, plus `xmp:Rating="-1"` for an unrated reject) and keywords, written to the sidecar at once (`amb_protocol::sidecar::with_info`: `xmp:Rating`, `dc:subject`, keeping `lr:hierarchicalSubject` paths in step) and embedded in exports: XMP in JPEG/PNG/TIFF/WebP and IPTC keywords in JPEG (`amb-export/src/embed.rs`, added to the finished file), plus EXIF Rating and XPKeywords; `amb render --rating 4 --keywords "a, b"` does the same from the command line
  - filmstrip: the raws in the open photo's folder (`src/folder.rs`: natural name order, ratings and edit state from the sidecars, thumbnails from the camera's embedded preview via `amb_engine::raw::embedded_thumbnail`, cached on disk in the app's cache folder; `ui/filmstrip.js`). Opening another photo saves unsaved edits of the current one first
  - long size lists (crop aspect, export size) use a drop-down with collapsible groups (`ui/picker.js`, layered over the native select): common sizes open, sites collapsed
  - file names from a template for batch exports (Import's tokens: `{name}`, `{seq}`, the capture date, `{camera}`; `amb render --out-dir --name` too)
  - presets: hover to preview, ☆ to star as a favourite (shown first), right-click to apply only some setting groups (`apply_preset` `include_groups`)
  - export presets (the dialog's choices under a name, in the app's settings) and Resize by long or short edge, width, height, megapixels or percent
  - watermarks on export: text (GNU FreeSans Bold, bundled) or a PNG logo, in one of nine places, with size, opacity and a shadow (`amb_export::watermark`; `amb render --watermark`)
  - borders on export: presets (built-in and yours) with width, a deeper bottom, colour or the photo's own colour, keyline, rounded corners, "whole photo inside" for exact sizes, and a caption line from the photo's metadata (`border::caption_text`); a live preview in the export dialog. Captions and text watermarks pick a font from `amb-export/src/fonts.rs` (the editor loads the same files, `font_data`). The editor's Frame panel (`ui/frame.js`) draws the border with CSS around the preview and the text as an overlay, outside the edit, and fills the export dialog's Border and Watermark from it
  - plugins: `crates/amb-plugin` loads a plugin folder (`plugin.json` + `plugin.wasm`), checks it (only the `amb.log` import), and runs it in the `wasmi` interpreter with an instruction budget and a 64 MB memory cap. The input is JSON (settings, parameters, a seed, and with permission camera facts and statistics of a ~512 px render, `amb_plugin::photo`); the output is edit commands, limited to set/adjust/reset. The editor runs them as one undo step (`run_plugin` in `src/commands/plugins.rs`; the list, approvals per fingerprint, install and sharing are in `src/plugins.rs`; the dialog is `ui/plugins.js`). Built-in plugins (Auto tone, Random look, Film recipe, Duotone, Light leak) are embedded from `plugins/` (`amb_plugin::BUILTIN`). Effect plugins (`amb_plugin::effect`) export `amb_effect` instead: the edit keeps them in `amb:Effects`, the engine renders them after the LUT (`amb_engine::plugin_effects`, a CPU step in the GPU preview) through a resolver the editor and `amb` register, colour effects as a 33³ table and pixel effects in strips; `ui/fx.js` draws them in the Effects panel. Recipes (named parameter sets) come from the manifest or are saved in the app's config. `amb plugin` in the CLI makes, builds, checks and runs them
  - save as preset (`session::save_preset`): the open photo's settings minus the photo-specific groups (crop, local, retouch, transform, metadata, lens blur), written as a Lightroom-style `.xmp` preset into `<config>/presets/<group>/`, which the presets list always includes
  - library (`ui/library.js`, `src/library.rs`; [design/library.md](design/library.md)): a folder as a grid, with loupe, compare and survey views for culling (`ui/cull.js`) an Import dialog (`ui/import.js`, `amb-library::import`), collections (`ui/collections.js`, `amb-library::collections`, kept in the config folder) and virtual copies (sidecars `IMG.copy-N.xmp`; `amb_engine::virtual_copy_of` / `source_path`), read from the sidecars (ratings, `xmp:Label` colour labels, keywords) and the raw headers (capture time, camera, lens, exposure, in parallel) through the catalogue (`folder::catalog`), which also gives All photos; batch rating/label/keyword changes (the open photo's through its session); paste settings as `reset_group` + `apply_preset` (the source sidecar, limited to the chosen groups), the open photo as one undo step; batch export through the export dialog. Edited thumbnails are rendered and cached until the sidecar changes (`folder::edited_thumbnail`), also for the filmstrip
  - collage maker (`ui/collage.js`, a full-window dialog): free layouts (frames moved, resized, turned and stacked by hand) are composited with a turned, soft-edged resample over what is beneath; collages save as `.ambcollage` project files (`collages::save_project` / `open_project`: photo paths full and relative); layouts (`ambrotype-collage/1`) are validated, laid out and composited by `amb-export/src/collage.rs` (cover fit with pan, zoom and quarter turns, rounded corners, a page planner that matches photo shapes to cell shapes). `src/collages.rs` renders each photo from its raw with its sidecar edits (`Session::render_upright`, one photo at a time) into the canvas, then exports it; batches write numbered pages. The canvas preview mirrors the Rust layout maths in JS and shows the photos' edits (`edited_thumbnail`); the open photo's unsaved edits are saved before a collage export
  - saved preset folders, and a feedback button that opens a pre-filled GitHub issue
  - save the XMP sidecar, keeping other metadata
  - export dialog
  - soft proofing (`OutputSpace::soft_proof`, set on the session with `set_proof`): previews and the 100% view clipped to an export space, with the colours beyond it marked
  - edit in another app (`edit_in` in `src/commands/export.rs`): a TIFF next to the photo (`session::edit_copy_path`, Lightroom's `-Edit` names), opened with the app chosen in Settings (or the system's default)
- **Not yet:** see [GAPS.md](GAPS.md) §2.

## What does not exist yet

- **A full GUI.** The editor exists (see below). Missing: a native GPU viewport (previews are developed on the GPU but still sent to the page as JPEGs). All remaining gaps are listed in [GAPS.md](GAPS.md).
- GPU versions of a camera profile's tables, defringe, spots, masks and LUTs (they run as CPU steps inside the GPU preview), and caching per stage (region renders exist for the 100% view).
- Red eye, reading Lightroom's own Guided Upright guides ([design/transform.md](design/transform.md)), AI masks.
- Camera profiles (DCP) and Adobe-like base looks: today a placeholder filmic base look is used.
- Masks, crop and spots from the assistant panel ([ai/assistant.md](ai/assistant.md)).
