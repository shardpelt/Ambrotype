# Architecture as built (November 2026)

The target architecture is in [SCOPE.md §9](SCOPE.md). This page describes what exists today.

**Short version:**
- Ambrotype is a Rust **library-first** project. The engine, preset reader, lens database and export are libraries.
- There are two front ends: the `amb` command-line tool and a desktop editor (`amb-gui`, Tauri 2).
- The GUI, the MCP server for AI assistants (`amb mcp`) and the CLI are all clients of the same libraries, through one command protocol, like FFmpeg/libmpv and their players.

## Crates

For the size of the code, run `python3 tools/code-metrics.py`. fmt, clippy and tests run in CI on Linux, Windows, macOS and Windows on ARM; the UI scripts on Linux. Sizes, the crate graph and the size budget: [design/structure.md](design/structure.md).

| Crate | Role | Main modules |
|---|---|---|
| `amb-presets` | Read and write settings, and the tag registry | `masks.rs`, `retouch.rs`, `linked.rs`, `data/tags.json` |
| `amb-protocol` | Typed edit commands for GUI, CLI and AI | commands, queries, `sidecar`, JSON Schema |
| `amb-color` | Colour science | Temperature/Tint ↔ xy, matrices, Bradford adaptation, output colour spaces and generated ICC profiles, reading matrix ICC profiles, sRGB → monitor conversion (`display.rs`) |
| `amb-lens` | Lens profiles | lensfun and Adobe LCP readers, lens matching, interpolation, correction geometry |
| `amb-engine` | Raw decode and develop | The pipeline below; every model constant is in `Calibration` (JSON, fitted later against Lightroom) |
| `amb-ai` | Local AI models | `faces.rs`, `search.rs`, `net/`, `objects.rs`, `remove.rs`, `denoise.rs`, `archive.rs`, `data/models.json` |
| `amb-gpu` | GPU preview and AI kernels | The pipeline's stages as wgpu compute shaders; `nn.rs` |
| `amb-export` | Files out | `dng.rs`, `compare.rs`, `contact.rs`, `video.rs`, `embed.rs`, `border.rs`, `collage.rs`, `watermark.rs`, `fonts.rs` |
| `amb-library` | Library operations on folders and sidecars | `catalog.rs`, `folder.rs`, `lrcat.rs`, `lightroom.rs`, `files.rs` |
| `amb-ops` | Operations that render photos | `convert.rs`, `replay.rs`, `merge.rs`, `object.rs`, `snap.rs`, `burst.rs`, `cleanup.rs`, `dust.rs`, `negative.rs`, `export_batch.rs` (a folder tree to smaller copies, same structure; `amb render --recursive` and the editor's Batch export) |
| `amb-plugin` | The sandboxed plugin host | `manifest.rs`, `effect.rs`, `photo.rs`, `native.rs`, `fuji.rs` |
| `amb-plugin-sdk` | What plugins are written with; MIT OR Apache-2.0 | `lib.rs` |
| `amb-testkit` | Calibration tooling | Synthetic DNG targets (incl. a scanned negative), the Lightroom slider-sweep kit |
| `amb-cli` | The `amb` tool | `src/mcp.rs`, `src/organise.rs` |
| `amb-gui` | Desktop editor (Tauri 2) | Rust `Session` (open raw, undoable edits via `amb-protocol`, downscaled preview + histogram, sidecar save, export) behind Tauri commands; plain HTML/CSS/JS UI generated from the settings registry |

### `amb-presets`

- XMP (`crs:` Adobe tags plus our `amb:` tags), `.lrtemplate`, and XMP embedded in DNG and JPEG.
- The tag registry (`data/tags.json`).
- Preset Amount scaling.
- The mask model (`masks.rs`): Lightroom's correction tags, slider units, parts.
- Spots (`retouch.rs`).
- `linked.rs`: where a sidecar's text comes from when a Lightroom catalogue is opened in place (the newer of the file and the catalogue's copy).

### `amb-protocol`

- Edit commands:
  - `set/adjust/reset_param`, `apply_preset`.
  - Mask commands: `add/set/paint/remove/duplicate/move_mask`, `list_masks`.
  - Spot commands: `add/set/remove_spot`, `list_spots`.
  - Snapshots: `save/apply/rename/delete_snapshot`, `list_snapshots`.
  - Undo and redo. Grouped commands (a plugin's, a preset's) are one undo step.
- Queries, and the JSON Schema.
- The sidecar merge keeps foreign metadata. The edit history and snapshots are stored in it (`amblib:History`, `amblib:Snapshots`).
- The AI settings reference.

### `amb-ai`

- Faces and eyes (`faces.rs`): YuNet, its three grids decoded to boxes and eye points.
- Smart search (`search.rs`):
  - CLIP and SigLIP 2 models, each of an image half, a text half and a tokenizer.
  - Our own byte-pair tokenizer for both kinds.
  - Photos and words become unit vectors.
- Subject, Background and People masks:
  - A list of freely licensed models to install (`data/models.json`), the user's choice per task, and their own ONNX models.
  - Run on the GPU (`net/`): tract's simplified graph is walked layer by layer, each layer with a kernel in `amb_gpu::nn` on the GPU and the rest with tract on the CPU.
  - Small models, and all models when there is no GPU, run with `tract` on all cores.
  - Downloaded from their authors and checked by sha256. Results are kept per photo fingerprint.
  - Registered with `amb_engine::subject` by the editor and `amb` ([design/masks.md](design/masks.md)).
- AI Remove (`remove.rs`):
  - Big-LaMa, or MI-GAN for speed (its pixels are bytes, `input.pixels`).
  - Fills content-aware spots.
  - Registered with `amb_engine::retouch::set_inpainter` ([design/spot-removal.md](design/spot-removal.md)).
- AI Denoise (`denoise.rs`):
  - RawNIND's UtNet2 networks (GPL-3.0 or CC BY 4.0), from darktable-ai's model archive (`archive.rs` takes the files out of the zip).
  - Bayer: the sensor's samples, packed R, G, G, B at half size, in place of our demosaic; others (X-Trans): linear Rec.2020 after it.
  - 512-pixel tiles overlapping by 64, each scaled back to its input's brightness, blended with a feather.
  - Registered with `amb_engine::raw_denoise::set_raw_denoiser`; `amb_ops::denoise` writes the result as `<name>-Denoised.dng` with the photo's edit. `amb denoise`, the library's AI Denoise….

### `amb-gpu`

- The develop pipeline's stages as wgpu compute shaders, run from an `amb_engine::plan::Plan`.
- Stages without a kernel run on the CPU in between.
- Equivalence tests against the CPU pipeline ([design/gpu.md](design/gpu.md)).
- `nn.rs`: tensors and AI model layers, for `amb-ai`:
  - Convolution as a tiled matrix product, and transposed convolution.
  - Matrix products by strides.
  - Element-wise operations, copies and padding.
  - Reductions, softmax, RMS norm, max pooling and gather.

### `amb-export`

- Formats: JPEG, PNG, TIFF, WebP, AVIF, EXR and JPEG XL.
- Colour spaces, resize, output sharpening, EXIF, and the JPEG size limit.
- A smaller copy beside any export (`small_copy`): NAME-small.jpg under a size, as sharp as fits, with fewer pixels only below quality 85.
- Floating-point linear DNG for merged photos (`dng.rs`).
- A before/after web page with both photos inside (`compare.rs`; `amb share`, File → Share Before and After…).
- Contact sheets as film strips or a grid (`contact.rs`):
  - Filled by `amb_library::contact`.
  - `amb contact`, the library's Contact sheet….
- AV1 video in our own MP4 writer (`video.rs`, rav1e) for Edit Replay:
  - Frames come from `amb_ops::replay`.
  - `amb replay`, File → Export Edit Replay….

### `amb-library`

The library:

- The catalogue (`catalog.rs`): a SQLite index, in the config folder, of what was read from each photo's sidecar and raw header. It is checked against file sizes and modified times, so it can't go stale and can be deleted. `amb catalog`.
- Which files in a folder are photos (`folder.rs`).
- Import (`import.rs`):
  - Copy from a card or folder into dated folders, renamed from templates, with companions and duplicate checks.
  - `amb import`, the editor's Import dialog.
- Collections (`collections.rs`) as small JSON files.
- Moving from Lightroom Classic (`lrcat.rs`):
  - Reads a `.lrcat` read-only and immutable with SQLite.
  - Writes each photo's sidecar from the catalogue's develop settings, XMP and library tables, and its collections as ours.
  - `amb lrcat import`, the library's From Lightroom….
- A catalogue opened in place (`lightroom.rs`):
  - The catalogues are listed, and each is read again when its file changes.
  - It answers `amb_presets::linked` for every sidecar read.
  - `amb lrcat open`, the Folders panel's Lightroom section, `ui/library/lightroom.js`.
- Places from GPX tracks (`gpx.rs`).
- Duplicates and bursts (`duplicates.rs`), its measurements kept in the catalogue.
- Moving, renaming and deleting photos with their companions, the trash, and finding photos moved outside the app (`files.rs`).
- The library folders (`roots.rs`) and smart collections (`smart.rs`).
- Smart search over files (`search.rs`):
  - Each photo's vector is kept in the catalogue's `vectors` table until the file changes.
  - Ranked against words or another photo.
  - `amb search`, the library's Smart and Find similar photos.

Operations on photos:

- Raws to JPEGs (`convert.rs`):
  - The camera's own JPEG inside the raw via `raw::embedded_preview`, or Auto, or Match look to an edited photo, solved small and exported full size.
  - `amb convert`, the library's Convert raws….
- An object selected (`object.rs`, `Selection`):
  - A mask on the frame from the object selector or the Subject model, its edge snapped to the photo's by a colour guided filter (`snap.rs`).
  - The shadow it casts (`shadow_of`): darker than round it, grown from where it stands. `set_shadow` takes it along.
  - `select_words`: what's named.
    - Found by asking the object selector about a coarse grid and the middles of the windows the search model finds most like the words (refined twice).
    - Each thing found is scored by the search model with what's round it greyed (`amb_ai::search::Words`: the words against what else a photo might be, as a softmax).
    - A smaller thing inside the best one is preferred, and as-good ones apart from it are added.
  - `remove`, `move_by` (a clone, then a Remove spot that leaves it alone) and `mask_dabs` (a distance transform's dabs for a brush mask).
  - Select object in `ui/tools/select.js`.
- An object in a box to one Remove spot (`object.rs`):
  - A click or a box is asked of the object selector (`amb_engine::subject::select`, SAM 2.1 tiny in `amb-ai/src/objects.rs`: the encoder once per photo, the decoder per click).
  - For a box without it, the Subject model on just the box.
  - Its outline is covered by dabs.
  - The Spot panel's Remove object….
- Dust and hairs to heal spots (`dust.rs`, over `amb_engine::dust`):
  - `amb dust`, the Spot panel's Find dust.
  - [design/dust.md](design/dust.md).
- Film negative rolls (`negative/`):
  - The analysis is frozen into settings, for one frame, or a roll is grouped into scenes by light with odd frames kept (`roll`), through `pipeline::negative_analysis`.
  - `amb negative analyze` and `amb negative sync`, the library's Sync negative roll….
- Look matching on files (`look.rs`; `amb match`).
- HDR merge into a DNG next to the brackets, and panorama merge with lens corrections first (`merge.rs`, over `amb_engine::merge` and `amb_engine::pano`):
  - `amb merge hdr` / `amb merge pano`, the library's Merge to HDR… / Merge to Panorama….
  - [design/hdr-merge.md](design/hdr-merge.md), [design/panorama.md](design/panorama.md).
- Best in a burst (`burst.rs`):
  - Each photo's sharpness on its faces or its sharpest part, and how open the eyes are, from faces found by `amb_engine::subject::faces` (YuNet in `amb-ai/src/faces.rs`).
  - Then ranked against the burst's other photos.
  - The library's Best in each burst, `ui/library/bursts.js`.
- Clean up (`cleanup.rs`): duplicates and bursts from `amb_library::duplicates`, bursts ranked by `burst.rs`, then blurry (against a floor and the shoot's middle sharpness), eyes closed and black or white frames; photos picked or rated are left out. `amb cleanup`, the library's Clean up…, `ui/library/cleanup.js`.
- `replay.rs` and `contact.rs` fill `amb-export`'s video and contact sheets (see `amb-export` above).

### `amb-cli`

The `amb` commands, by kind:

- Editing: `do` (the compact edit syntax), `edit`, `query`, `auto`, `match` (look matching, `amb_engine::matching`), `fuji`, `dust`, `negative`.
- The library and files: `import`, `catalog`, `info`, `tag`, `place`, `duplicates`, `search`, `lrcat` (`import`, `open`, `close`, `list`, `sample`), and `move`, `rename`, `sort`, `trash`, `keywords`, `collection` (`src/organise.rs`).
- Files out: `render`, `convert`, `merge` (`hdr`, `pano`), `collage`, `contact`, `replay`, `share`, `slideshow` (opens the editor).
- AI, MCP and plugins: `mcp` (the MCP server, `src/mcp.rs`), `ref` (the AI core card), `protocol schema`, `settings reference`, `models`, `plugin` (makes, builds, checks and runs plugins).
- Measuring and tooling: `preset`, `stats`, `lens`, `profile`, `sweep`, `testimage`, `calibration`.

## Data flow of `amb render photo.nef out.jpg --preset look.xmp`

```
raw file ──rawler──▶ CameraImage (camera RGB, white = 1, per-channel clip levels, + metadata: lens, EXIF, orientation;
                     a DNG's gain maps applied, its warp and vignette opcodes kept as the photo's own lens correction,
                     or a Sony ARW's distortion tags, amb-engine/src/sony.rs)
         (rawler decodes; we demosaic: RCD for Bayer in amb-engine/src/rcd.rs, our Markesteijn port for X-Trans in amb-engine/src/xtrans.rs)
         (then partly clipped highlights rebuilt from their other channels: amb-engine/src/highlights.rs)
JPEG/PNG/TIFF/WebP ──raster.rs──▶ CameraImage too: the image's RGB made linear by its ICC profile
         (sRGB if none), `rendered` set: relative white balance, no base tone curve, no default sharpening;
         the edits go to `photo.jpg.xmp`
preset/sidecar ──amb-presets──▶ settings map (crs:/amb: tags) ──▶ DevelopSettings
                                       │ (lens profile resolved via amb-lens)
                                       ▼
amb-engine pipeline (linear float RGB, rayon-parallel passes over one buffer):
  0  Lens Corrections (vignetting, CA, distortion), Upright + Transform, crop: one resampling pass
     (Upright found from the photo's lines on a ≤1024 px lens-corrected copy, upright.rs)
     → film negative conversion (optional; the film base from the whole scan, the levels inside the crop)
  1  clipped channels → neutral, white balance + camera matrix (× Calibration primaries) → linear ProPhoto
     (a camera profile, profile.rs: its matrices, hue/sat map, exposure offset, look table)
     → Exposure → Calibration shadow tint → Defringe
     → spot removal (heal, clone and AI Remove, retouch.rs) → masks drawn (alpha per mask, mask/)
     → local Exposure, Temp, Tint
  2  Dehaze (then local Dehaze) → halation and bloom (amb:Halation, amb:Bloom, glow.rs) → highlight roll-off (amb:HighlightRolloff, rolloff.rs)
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

Speed today: a 24 MP raw with a typical edit renders in about 3.5 s on a 4-core CPU, plus about 2.5 s to decode it and write a full-size JPEG. Most of that is the CPU pipeline. The editor's preview and full-size exports (Export, Convert raws, `amb render`) can run on the GPU (`amb-gpu`, [design/gpu.md](design/gpu.md)), with the CPU as fallback; the 100% view stays on the CPU.

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
  - Rust: `src/main.rs` holds startup; the app state is in `src/state.rs` and the saved settings in `src/config.rs`: one `Config` behind `AppState::prefs` (read with `prefs`, change with `change_prefs`, which saves). Every command is registered in `handler` in `src/commands/mod.rs`. The Tauri commands themselves are in `src/commands/`, one file per area: `appearance`, `assistant`, `batch`, `catalog`, `collage`, `connect`, `export`, `features`, `files`, `hub`, `library`, `menu`, `models`, `photo`, `places`, `plugins`, `presets`, `preview`, `sample`, `select`, `settings`, `tools` (plus `mod.rs`). The logic they call is in the other `src/` files (`session/`, `preset_files.rs`, `looks/`, `hub/`, …).
  - JS: `ui/core/core.js` (loaded first) holds the shared helpers: `invoke`, `$`, the shared state (`ui`), `toast`, `status`, `askYes`, `el` and the path helpers. `ui/core/photo-row.js` is the sideways row of photos (`photoRow`: wheel, drag-to-scroll, lazy thumbnails, clicks; the filmstrip and the Map's tray use it) and `dragToScroll`; `ui/core/pinch.js` the photo's gestures. `ui/develop/rows.js` holds the slider rows, `ui/develop/history.js` History and snapshots, `ui/develop/photo.js` opening and saving photos, and `ui/core/app.js` `init`, which wires the buttons. The preview loop and 1:1 view are in `preview.js`, the crop panel in `croptool.js`, the Presets panel in `presets.js` and the Export dialog in `export.js`; the other panels and dialogs each have their own file.
  - Monitor colour: `src/monitor.rs` keeps the profile in use (Settings → General: Automatic, Off or a file); the editor's session converts its JPEGs to it as they are encoded (`Session::on_screen`; thumbnails and the assistant's pictures stay sRGB). Linux only: macOS and Windows webviews match colours themselves.
  - UI tests: `ambrotype --ui-test SCRIPT.js` (`src/uitest.rs`) puts the script into the page, where `ui/core/uitest.js` runs it with its helpers and reports back; exports skip the save dialog. Scripts are in `tests/ui/`.
- **One code path:** the UI only sends `amb-protocol` commands (`set_param`, `apply_preset`, `undo`, …), so it behaves exactly like the CLI and AI clients. Panels and sliders are generated from the settings registry, so they always match the engine.
- **Preview:** rendered from an area-downscaled copy of the raw, at screen size, on the GPU when the edit is covered (`amb-gpu`), else on the CPU. Slider drags render with temporary overrides and don't touch history; release commits one undoable step. Requests are latest-wins, and a drag renders at 0.6× size.
- **Features:**
  - open (dialog, drag-and-drop, command line)
  - Basic, Tone Curve (parametric), Color Mixer (HSL tabs), Black & White, Color Grading, Detail, Lens Corrections, Transform, Effects, Calibration and Film Negative panels
  - presets list (Lightroom's own folder when present, or added folders) with an Amount slider
  - history, undo/redo (a drag of a curve point or colour wheel, or a WB pick, is one step), before/after, histogram
  - 1:1 view with panning, and the tools that work on the photo:
    - Only the tiles (256 px) the view needs are rendered (`render_region`), and kept until the committed settings change, so panning back is instant.
    - White-balance picker, clipping overlay, point-curve editor (RGB and per channel), colour-grading wheels.
    - Crop and straighten tool: draw a crop corner to corner like a screenshot, then adjust; aspect lock, size presets ([design/crop.md](design/crop.md)).
    - Mask tool (`ui/tools/masks.js`, `ui/tools/mask-draw.js`, `ui/tools/mask-panel.js`): linear, radial, brush, luminance and colour range masks with add/subtract/intersect and local sliders, drawn on the photo through `mask_frame`'s map ([design/masks.md](design/masks.md)).
    - Spot removal (`ui/tools/spots.js`): heal and clone spots with a suggested donor, and Remove spots filled by AI Remove (`spot_source`). Find dust adds a heal spot per speck and hair (`find_dust`). [design/spot-removal.md](design/spot-removal.md), [design/dust.md](design/dust.md).
  - Guided Upright: guides drawn on the photo, which the Transform sliders straighten ([design/transform.md](design/transform.md)).
  - film negatives ([design/film-negatives.md](design/film-negatives.md)): Pick base, the base measured on the whole scan, the levels inside the crop, Auto off (every frame printed alike on a fixed density range), Expired film and Cast removal along the greys.
  - film base picker (`pick_film_base`: Pick base in the Negative panel; a click on clear film between frames sets `FilmBase*`, Alt-click forgets it; the WB picker shares `Session::shown_patch`, which follows the crop)
  - crop Level (the angle from the photo's long lines, `guide_lines`), guide lines the rotation snaps to, and crop to the view when zoomed in (`ui/tools/croptool.js`)
  - copy the photo to the clipboard (right-click on the photo: `copy_image`, an edited render through the clipboard plugin; `copy_text` for text)
  - right-column tabs (`.column-tabs` in `ui/looks/columns.js`, the chosen tab kept per layout in localStorage `amb.tabs`); the status bar is off unless Settings → General turns it on, and notes then show briefly over the photo (`#status-note`)
  - size presets (built-in for social sites and print, plus custom) for the crop aspect and exact-size export
  - **looks:** the toolbar, side columns and panels come from a layout file and the colours from a theme.
    - `src/looks/` validates (`check.rs`), stores and keeps setups (`setups.rs`) for them.
    - `ui/looks/layout.js` renders them (the columns, in `columns.js`; the Appearance dialog, in `looks-dialog.js`).
    - `ui/develop/controls.js` has the knob, bar, number, toggle and segmented controls.
    - `ui/looks/editor.js` (with `editor-keys.js` and `editor-inspector.js`) is editor mode, which edits a draft of the layout and re-renders it.
    - `ambrotype --check-look FILE...` validates layout and theme files (what AI assistants use, [ai/ui-layouts.md](ai/ui-layouts.md)).
    - Toolbar icons are our own line drawings (`ui/core/icons.js`); a layout with `"toolbar_style": "words"` (Beginner) shows words on every button.
    - Side columns are resized by dragging their inner edge: widths are kept per layout in the page's local storage (`colWidths` in `ui/looks/columns.js`).
    - A column scrolls as one: block headings stick and fold (`amb.fold.<block>`), a strip at its foot names the blocks below the fold, and History stays docked at the foot, outside any column tabs (`ui/looks/blocks.js`, `ui/looks/layout.js`). The presets block is a grid of previews from `preset_thumbnails`, made as tiles come into sight (`ui/develop/preset-grid.js`).
    - A layout with `"chrome": "hidden"` (Minimalism) hides the bars and slides the columns in over the photo from the window's edges (`applyChrome` in `ui/looks/columns.js`).
  - **slideshow:** `ui/library/slideshow.js`. Two image layers on black; each photo's large edited thumbnail (`bigImage`, cached with the library's), the next one fetched while the current shows; CSS classes give the change effect. Full screen through the `window_fullscreen` command
  - **before / after split:** `ui/develop/split.js`. The "before" half keeps the crop, geometry and lens corrections, so both halves line up.
  - **follows outside edits:** `sidecar_watch` (`notify`) watches the open photo's sidecar; a change by another app or an AI agent over MCP is loaded as one undo step.
  - **tasks:** `ui/core/tasks.js` draws the task bar (Library, Develop, Film, Collage, then task plugins; what earns a tab: [design/tabs.md](design/tabs.md)).
    - Library opens the library, Collage the collage maker (back to the task you were in when it closes).
    - The others switch to a built-in layout, or the one the user picked in that task (localStorage `amb.tasks`).
    - Develop's Classic layout holds what were the Color and Retouch tabs: the Retouch tools under Quick fixes, a Colour tab of panels; tool blocks fold by their heading.
  - **custom interfaces:** `src/interface.rs` serves a folder with an `index.html` on the `ambui` scheme (`ambui://localhost/`, `http://ambui.localhost/` on Windows), only files inside it.
    - Tauri treats the app's own schemes as local, so the page calls the same commands.
    - Settings → General picks it: `set_interface` navigates the window and puts up a small menu with a way back. The open photo comes along through `take_initial_path`, and `open_photo` on the open photo keeps its session.
    - Installed ones live in `<config>/interfaces/`; `--builtin-ui` ignores the choice.
    - Guide: [ai/custom-interface.md](ai/custom-interface.md).
  - **connect an AI app:** `ui/dialogs/connect.js` (Help → Connect an AI App…) shows `amb mcp` with the path found by `commands/connect.rs` (next to the editor, else on the PATH) and the setup for the apps in `ui/ai-apps.json`; `open_terminal` opens the system's terminal in the open photo's folder with `amb` first on the PATH, and with an app's `agent` command starts that agent there, told where the built-in guides are (`amb_protocol::guides`, written to the app's data folder; also `amb docs` and the MCP resources)
  - **quick zoom:** `ui/develop/quickzoom.js`.
    - A right-drag box sets `ui.zoom.box`; `zoomScale` in `ui/develop/preview.js` turns it into a scale, and the zoomed view renders a region at 100% (at most 4096 px a side) that the page scales to fit.
    - The loupe asks `region` for a small square at 100% around the pointer, showing the preview enlarged until it arrives.
    - The box can be turned off in Settings (kept in the page's local storage, like the menu bar).
  - **teaching by showing:** `ui/help/teach.js`.
    - After an undo or redo (a History click, Ctrl+Z, or the arrow keys with History focused) the controls that step changed glow and slide to their value.
    - Help → Watch an edit replays the sample's History with a ghost cursor (`watchFlow`); any key or click stops it.
    - Holding Ctrl (⌘) shows each toolbar button's and task's key.
    - On a photo with no edit yet, Exposure, Highlights, Shadows, Temp and Vibrance glow faintly.
    - Help → Open the sample photo (or the empty screen's button) downloads a CC0 Nikon D750 raw from raw.pixls.us into `sample/` in the settings folder, checks its sha256, and adds an edit with one History step per slider (`commands/sample.rs`, `data/sample/d750.xmp`).
    - Hovering a task shows a sketch of its layout (`ui/core/tasks.js`).
    - A "?" by each setting (`ui/help/help.js`) shows its description on hover and, on click, a card with the `coach` text from `descriptions.json` (how it shows in a photo, when to raise or lower it), passed through `schema.rs`.
      - Words in it that are terms in `ui/basics.json` link to Photography basics (`ui/help/basics.js`): hover for a line, click to open the guide.
      - Tone and colour terms also light up their pixels on the preview (`ui/develop/zones.js`, computed in the page from the preview JPEG).
      - The guide's drawings are SVG from `ui/help/figures.js`.
  - **yes-or-no questions:** `askYes` in `ui/core/core.js` (the dialog plugin's message box). The page's own `confirm()` can't be used: the dialog plugin replaces it with a call that fails, so it never asks (a test keeps it out)
  - **hub:**
    - `src/hub/` reads a hub's `index.json` (https, or file:// for testing), checks each look file against its sha256, validates it and saves it as one of the user's looks. A plugin's or interface's other files are listed with their own sha256, and an interface holds only web files.
    - It also builds the GitHub link for submitting a look.
    - `ui/dialogs/hub.js` is the Browse dialog, with previews drawn from the looks' own data.
    - The hub repo is `Ambrotypehq/ambrotype-hub`. The app's built-in looks and starter presets are a copy of some of its items in `data/hub/` (`tools/sync-hub.py`), and the hub shows those as built in.
  - **settings and keyboard shortcuts:**
    - `ui/core/keys.js` has one table of actions with default keys; it runs the shortcuts, writes the toolbar tooltips and drives the Settings dialog (keyboard map, rebinding, clash checks). Only the user's changes are stored (`src/keys.rs`, in the settings file).
    - `ui/core/menu.js` builds the menu bar from the same table (`MENU_BAR`).
    - `ui/core/search.js` (Ctrl+K) searches that table with the settings, panels, presets and looks.
      - It sets a numeric setting from its name and a number ("exposure 0.7").
      - It finds what people want to do in docs/FEATURES.md (built in by `src/commands/features.rs`): a "How to" shows where it is, with buttons for the commands, panels, settings and buttons it names in bold.
    - `src/commands/menu.rs` puts the menu bar up (macOS always; Windows and Linux when Settings → General asks, kept in the webview's storage) and sends a chosen item back as a `menu` event.
    - The built-in layouts and themes are copies of the hub's, in `crates/amb-gui/data/hub/looks/` (`tools/sync-hub.py` copies them); the user's own are files in the config folder. Format: [ai/ui-layouts.md](ai/ui-layouts.md).
  - rating (keys 1–5), pick/reject flags (P, X, U) and keywords:
    - Flags are `amblib:Flag`, plus `xmp:Rating="-1"` for an unrated reject.
    - They are written to the sidecar at once (`amb_protocol::sidecar::with_info`: `xmp:Rating`, `dc:subject`, keeping `lr:hierarchicalSubject` paths in step).
  - rotate and flip (Ctrl+[ and Ctrl+], `ui/develop/info.js` `turnPhotos`): the turn is `tiff:Orientation` in the sidecar, as Lightroom writes it (`PhotoInfo::orientation`, through `library_set_info`'s `rotate`/`flip`). `amb_engine::orientation` composes turns and flips, and `raw::load` and the embedded previews apply the sidecar's orientation over the camera's, so everything that shows or exports a photo turns it. Pixels, crops, masks and spots stay in the stored orientation.
    - They are embedded in exports: XMP in JPEG/PNG/TIFF/WebP and IPTC keywords in JPEG (`amb-export/src/embed.rs`, added to the finished file), plus EXIF Rating and XPKeywords.
    - `amb render --rating 4 --keywords "a, b"` does the same from the command line.
  - filmstrip: the raws in the open photo's folder (`ui/develop/filmstrip.js`).
    - `src/folder.rs`: natural name order, ratings and edit state from the sidecars.
    - Thumbnails come from the camera's embedded preview via `amb_engine::raw::embedded_thumbnail`, cached on disk in the app's cache folder.
    - Opening another photo saves unsaved edits of the current one first.
  - long size lists (crop aspect, export size) use a drop-down with collapsible groups (`ui/core/picker.js`, layered over the native select): common sizes open, sites collapsed
  - file names from a template for batch exports (Import's tokens: `{name}`, `{seq}`, the capture date, `{camera}`; `amb render --out-dir --name` too)
  - preset choice "Apply to the original, not on top" (`#preset-from-original`): on, a preset replaces the look (crop, spots, masks, geometry and a negative's conversion stay); off, presets add up.
  - presets: hover to preview, ☆ to star as a favourite (shown first), right-click to apply only some setting groups (`apply_preset` `include_groups`)
  - export presets (the dialog's choices under a name, in the app's settings) and Resize by long or short edge, width, height, megapixels or percent
  - watermarks on export: text (GNU FreeSans Bold, bundled) or a PNG logo, in one of nine places, with size, opacity and a shadow (`amb_export::watermark`; `amb render --watermark`)
  - borders on export:
    - Presets (built-in and yours) with width, a deeper bottom, colour or the photo's own colour, keyline, rounded corners, "whole photo inside" for exact sizes, and a caption line from the photo's metadata (`border::caption_text`).
    - A live preview in the export dialog.
    - Captions and text watermarks pick a font from `amb-export/src/fonts.rs` (the editor loads the same files, `font_data`).
    - The editor's Frame panel (`ui/develop/frame.js`) draws the border with CSS around the preview and the text as an overlay, outside the edit, and fills the export dialog's Border and Watermark from it.
  - LUTs: Basic → Profile → LUT applies a `.cube` file with an amount (`amb:LUT`, `lut.rs`, [design/luts.md](design/luts.md)).
  - plugins:
    - `crates/amb-plugin` loads a plugin folder (`plugin.json` + `plugin.wasm`), checks it (no imports but the old `amb.log`, refused on Windows x64; log lines come back with the output), and runs it in the `wasmi` interpreter with an instruction budget and a 64 MB memory cap.
    - The input is JSON (settings, parameters, a seed, and with permission camera facts and statistics of a ~512 px render, `amb_plugin::photo`); the output is edit commands, limited to set/adjust/reset.
    - The editor runs them as one undo step (`run_plugin` in `src/commands/plugins.rs`; the list, approvals per fingerprint, install and sharing are in `src/plugins.rs`; the dialog is `ui/dialogs/plugins.js`).
    - Built-in plugins (Auto tone, Random look, Film recipe, Duotone, Light leak, Holographic, Hue trip, Paint, Tilt-shift, and the Trippy task) are embedded from `plugins/` (`amb_plugin::BUILTIN`). The built-in effects also run natively (`native.rs`: the same crates linked in, on all cores, only for the built-in module itself), a user's plugin always in the sandbox.
    - Effect plugins (`amb_plugin::effect`) export `amb_effect` instead:
      - The edit keeps them in `amb:Effects`.
      - The engine renders them after the LUT (`amb_engine::plugin_effects`, a CPU step in the GPU preview; an amount past 100 pushes the change further, and `where` (`Place`) keeps it to the highlights, midtones, shadows or a colour).
      - They go through a resolver the editor and `amb` register: colour effects as a 33³ table and pixel effects in strips.
      - `ui/develop/fx.js` draws them in the Effects panel.
    - `ui/help/tips.js` shows tips from the features list (while busy, and as search and filter hints; used commands skipped).
    - `ui/help/camsim.js` is Help → Camera simulator (a drawn scene through a pinhole camera; nothing edited).
    - `ui/tools/trip.js` has Trip me and the effect setter task tools use.
    - Task plugins (`"kind": "task"`, no module: a layout and tools, [design/plugin-kinds.md](design/plugin-kinds.md)) join the task bar (`ui/core/tasks.js`; their layouts as `plugin:<id>`, checked by `looks::check_layout`). Trippy is one (`plugins/trippy`).
    - Recipes (named parameter sets) come from the manifest or are saved in the app's config.
    - `amb_plugin::fuji` reads a pasted Fujifilm-style recipe into Film recipe's parameters, kept to its sensor generation's settings (the white balance shift is its own setting, `amb:WBShiftRed`/`Blue`), and runs it on an empty edit for a preset (Presets → Fuji recipe…, `fuji_recipe` in `src/commands/presets.rs`; `amb fuji`).
    - `amb plugin` in the CLI makes, builds, checks and runs them.
  - save as preset (`preset_files::save_preset`):
    - The open photo's settings minus the photo-specific groups (crop, local, retouch, transform, metadata, lens blur), written as a Lightroom-style `.xmp` preset into `<config>/presets/<group>/`, which the presets list always includes.
    - Save to… writes it anywhere (`save_preset_to`).
    - Share to the hub opens a pre-filled submission (`preset_submit`).
    - Show file reveals it (`reveal_file`).
    - The presets list skips a photo's own sidecar in a preset folder (`is_photo_sidecar`).
  - library (`ui/library/library.js`, `ui/library/library-grid.js`, `ui/library/library-actions.js`, `src/library.rs`; [design/library.md](design/library.md)):
    - The Folders panel (`ui/library/folders.js`): library folders from `amb-library::roots`, indexed in the background by `commands/catalog.rs`; offline folders listed from the catalogue.
    - A folder as a grid, with loupe and compare views for culling (`ui/library/cull.js`).
    - File care (`ui/library/organise.js`, `commands/files.rs`, `amb-library::files`): moves and renames with companions, the trash, finding moved photos, keywords and metadata templates.
    - Duplicates and bursts (`ui/library/duplicates.js`, `amb-library::duplicates`, its measurements kept in the catalogue).
    - Timeline and Map views (`ui/library/browse.js`, the Map's drag and zoom in `ui/library/map-touch.js`, shared with the place picker; places from GPX tracks by `amb-library::gpx` and guessed from placed photos by `amb-library::places` (`ui/library/guess-places.js`), set by `commands/places.rs`).
    - Insights view (`ui/library/insights.js`): how the photos shown were shot; its buckets come from `amb-library::insights`, which `amb insights` also uses.
    - An Import dialog (`ui/library/import.js`, `amb-library::import`).
    - Collections (`ui/library/collections.js`, `amb-library::collections`, kept in the config folder).
    - Virtual copies (sidecars `IMG.copy-N.xmp`; `amb_engine::virtual_copy_of` / `source_path`).
    - Photos are read from the sidecars (ratings, `xmp:Label` colour labels, keywords) and the raw headers (capture time, camera, lens, exposure, in parallel) through the catalogue (`folder::catalog`), which also gives All photos (the Folders | All photos switch) and a Rated photos view.
    - A JPEG shot with a raw is one photo with the raw (`library_twin_jpegs`); Filter → File: JPEG only lists them.
    - Stars in the filmstrip.
    - Develop or Esc opens the library's chosen photo (`leaveLibrary`).
    - Batch rating/label/keyword changes (the open photo's through its session).
    - Paste settings as `reset_group` + `apply_preset` (the source sidecar, limited to the chosen groups), the open photo as one undo step.
    - Batch export through the export dialog.
    - Edited thumbnails are rendered and cached until the sidecar changes (`folder::edited_thumbnail`), also for the filmstrip.
  - collage maker (`ui/collage/collage.js` and `ui/collage-*.js`, a full-window dialog; [design/collage-and-borders.md](design/collage-and-borders.md)):
    - Free layouts (frames moved, resized, turned and stacked by hand) are composited with a turned, soft-edged resample over what is beneath.
    - Collages save as `.ambcollage` project files (`collages::save_project` / `open_project`: photo paths full and relative).
    - Layouts (`ambrotype-collage/1`) are validated, laid out and composited by `amb-export/src/collage.rs` (cover fit with pan, zoom and quarter turns, rounded corners, a page planner that matches photo shapes to cell shapes).
    - `src/collages.rs` renders each photo from its raw with its sidecar edits (`Session::render_upright`, one photo at a time) into the canvas, then exports it; batches write numbered pages.
    - The canvas preview mirrors the Rust layout maths in JS and shows the photos' edits (`edited_thumbnail`).
    - The open photo's unsaved edits are saved before a collage export.
  - saved preset folders, and a feedback button that opens a pre-filled GitHub issue, or an email to ambrotypeapp@gmail.com for people without a GitHub account, or copies the report
  - save the XMP sidecar, keeping other metadata
  - export dialog
  - soft proofing (`OutputSpace::soft_proof`, set on the session with `set_proof`): previews and the 100% view clipped to an export space, with the colours beyond it marked
  - edit in another app (`edit_in` in `src/commands/export.rs`): a TIFF next to the photo (`session::edit_copy_path`, Lightroom's `-Edit` names), opened with the app chosen in Settings (or the system's default)
- **Not yet:** see [GAPS.md](GAPS.md) §2.

## What does not exist yet

- **A native GPU viewport.** The editor is complete (above), but previews are developed on the GPU and still sent to the page as JPEGs. All remaining gaps are listed in [GAPS.md](GAPS.md).
- GPU versions of a camera profile's tables, defringe, spots, masks and LUTs (they run as CPU steps inside the GPU preview), and caching per stage (region renders exist for the 100% view).
- Red eye, and reading Lightroom's own Guided Upright guides ([design/transform.md](design/transform.md)).
- Adobe-like base looks: without a camera profile (DCP) on the computer, a placeholder filmic base look is used.
- Masks, crop and spots from the assistant panel ([ai/assistant.md](ai/assistant.md)).
