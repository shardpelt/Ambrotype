# Roadmap

Everything planned, in the order we mean to do it. What to do right now is in [WHATS-NEXT.md](WHATS-NEXT.md); what is missing or approximate in what already exists is in [GAPS.md](GAPS.md); small code cleanups are cards in [cleanup.md](cleanup.md).

## How to read it

Each section is a table: **Id**, **What**, **Size**, **Tier**, **Ready**, **Notes**.

- **Size:** S a day or two, M about a week, L more.
- **Tier:** who can build it, see [delegation.md](delegation.md): small, medium, large or top.
- **Ready:**
  - `ready`: a recipe ([recipes.md](recipes.md)) or a design doc covers it, so a medium model can start.
  - `brief`: a large model must write a brief first. Notes say what has to be decided.
  - `owner`: waits on the owner. Notes say for what.
- Ids never change and rows are not numbered, so adding a row touches one line.
- Done rows are deleted. The feature goes to [FEATURES.md](FEATURES.md) and one line to WHATS-NEXT's "Recently done".

## State

- **Branch:** `main`. Checks and test runs: [AGENTS.md](../AGENTS.md).
- **Name:** **Ambrotype**. The GitHub repo is `shardpelt/Ambrotype`.
- **Built:** what people can do is in [FEATURES.md](FEATURES.md), how it is built in [ARCHITECTURE.md](ARCHITECTURE.md), and what's missing in [GAPS.md](GAPS.md). Size of the code: run `python3 tools/code-metrics.py`.

## 1. First release (v0.1)

The repo is public, CI is green on every platform, and the `Release` workflow has run by hand ([releasing.md](releasing.md)). Left:

- [ ] Try the editor on the Mac: menu bar (and Interface → Back to the Built-in Interface), library, a custom interface, Connect an AI App… → Open a terminal here, export
- [ ] Try it once on Windows (same list; the terminal opens with `cmd`)
- [ ] Read `docs/release-notes/v0.1.0.md`, tag `v0.1.0`, check the draft release, publish
- [ ] After publishing: README's "no installers yet" becomes a link to the release

After the release: the owner's sample raw replaces the stand-in sample photo (see `sample-raw` in section 5).

## 2. Quick wins

Fastest first.

| Id | What | Size | Tier | Ready | Notes |
|---|---|---|---|---|---|
| `speed-edit` | **Speed Edit:** hold a key and drag anywhere on the photo to change the setting last used | S | medium | brief | Decide: which key, what counts as "last used", how it differs from Touch (T). Source: RapidRAW 1.6.9 ([research/17](research/17-competitors-2026.md) item 21) |
| `jpeg-twin-on-open` | **"JPEG if there is one" also when a raw is opened directly** (Open…, drop, command line), not only from the filmstrip | S | medium | ready | `openPath` in `crates/amb-gui/ui/develop/filmstrip.js` already swaps twins when stepping; say so in the status line. Source: owner, 2026-11 |
| `tips-more-places` | **Tips in more empty places:** the other search and filter boxes (library filter, Settings search, hub search) and the empty History and Snapshots lists | S | medium | ready | `boxTip` in `crates/amb-gui/ui/core/search.js` is used on two boxes only. Source: owner, 2026-11-05 |
| `agent-editor-primer` | **Start an AI agent to change the editor itself**, not only to edit photos: a second primer and a choice in Connect an AI app | S | medium | ready | `primer` in `crates/amb-protocol/src/guides.rs`; the layout, theme and plugin guides are already built in ([ai/README.md](ai/README.md)). Source: owner, 2026-11-11 |
| `film-metering-region` | **Draw the negative's metering region by hand** (today the levels are measured inside the crop) | S | medium | ready | [design/film-negatives.md](design/film-negatives.md) "Settings". Source: Film lab track, owner 2026-10-28 |
| `filmstrip-run-action` | **Run an action plugin on the filmstrip selection** (right-click menu) | S | medium | ready | Same list as the Photo menu; `crates/amb-gui/ui/develop/filmstrip.js`. Source: owner, 2026-10-31 |
| `library-view-centred` | **Centre the library bar's View and Slideshow buttons** like the filmstrip's | S | small | ready | `crates/amb-gui/ui/style.css` (`.strip-views`). Source: owner, 2026-11-10 |
| `drag-to-folder` | **Drag photos onto a folder** in the Folders panel to move them | S | medium | ready | [design/library.md](design/library.md) "File care"; sidecars go along. Source: Photo manager track, owner 2026-10-19 |
| `trippy-share-presets` | **Trippy looks shareable:** a Trippy edit saved as a preset with its effects, and shared to the hub | S | medium | ready | Effects live in `amb:Effects`, so New preset from this photo may carry them already: check first, then the hub item kind steps in [recipes.md](recipes.md). Source: owner, 2026-11-11 |
| `collage-all-photos` | **Collage for every photo shown:** one layout over all of them, as pages | S | medium | ready | Batch pages already exist ([design/collage-and-borders.md](design/collage-and-borders.md)). Source: owner, 2026-11-11 |
| `library-speed-number` | **A published speed number for big libraries** (100k photos: open, filter, search) | S | medium | brief | Decide: how the synthetic library is made, what is timed, where it is written down. "Free browsers stall on big libraries" is a top switching complaint. Source: PhotoWorkout, 2026-11 |
| `peek-on-hover` | **Peek on hover:** the area round the pointer magnified after a moment still (the loupe, I, does it as a mode) | S | medium | brief | Decide: delay, size, how it ends. Source: owner, 2026-11-11 |
| `readme-screenshots` | **README screenshots again**, with the library (grid, Insights) and a Trippy edit | S | small | owner | Needs the owner's Fujifilm sample photos ([WHATS-NEXT.md](WHATS-NEXT.md)). Source: owner, 2026-11-11 |
| `trippy-past-range` | **Basic's sliders past their ranges in Trippy** (our own extra settings; Lightroom's ranges stay in the sidecar) | S–M | medium | brief | Decide: which sliders, how far, how they are stored. Source: owner, 2026-11-04 |
| `reverse-geocode` | **Reverse geocoding:** city and country names for placed photos, for search and captions | S | large | brief | Decide: online (Nominatim, as the place search) or offline data, and which sidecar fields hold it. [design/library.md](design/library.md) "Timeline and map". Source: GAPS §3 |
| `library-loupe-zoom` | **Zoom in the library's Loupe and Compare** like in Develop and the viewer | M | medium | ready | Wire `wirePinchZoom` (`crates/amb-gui/ui/core/pinch.js`) to `crates/amb-gui/ui/library/cull.js`, sharing the helper in `crates/amb-gui/ui/library/viewer-zoom.js`. Source: owner, 2026-11-11 |
| `insights-style-summary` | **Insights: an overall style summary** ("what kind of photographer the numbers say you are") | M | medium | brief | Decide: what it says and from which medians. `crates/amb-library/src/insights/advice.rs`. Source: owner, 2026-11-11 |
| `insights-gear` | **Insights: gear suggestions** tied to a stated shortfall (a faster lens for dark scenes, a wider zoom) | M | medium | brief | Decide: the rules and the wording. `crates/amb-library/src/insights/advice.rs`. Source: owner, 2026-11-11 |
| `insights-adjacent-styles` | **Insights: styles next to yours to try**, not only the far side | M | medium | brief | Decide: one step either side of the usual focal length and aperture, or other rules. Source: owner, 2026-11-11 |

## 3. Big things

Most important first. The first four are the Film lab, in the order WHATS-NEXT lists them.

| Id | What | Size | Tier | Ready | Notes |
|---|---|---|---|---|---|
| `film-paper-curve` | **A paper curve in density:** toe, straight line and shoulder, ISO-R grades, split grade, preflash, B&W papers fitted from public datasheets | L | large | brief | Candidate for negative model v2 ([design/film-negatives.md](design/film-negatives.md)). Decide: stage, settings, tags. Source: Film lab track item 9, owner 2026-10-28 ([research/16](research/16-film-scan-apps-survey.md)) |
| `film-stock-looks` | **Film looks tied to stocks:** halation, bloom and grain amounts per stock, so picking a stock brings its glow and grain | M | large | brief | [design/film-negatives.md](design/film-negatives.md) "Film stocks"; today they are the same for every photo. Decide: which settings a stock sets. Source: Film lab track, owner 2026-10-28 |
| `film-dust-guided` | **Dust: the guided rectangle and a heal brush that searches its stroke** | M | large | brief | [design/dust.md](design/dust.md) "Not yet". Decide: the search method. Source: Film lab track item 5 |
| `film-darkroom-aids` | **Darkroom aids:** density and zone under the cursor, zone overlay, placing a spot in a zone, step wedge, negative range and scan-clip readouts, test strip, ring-around, peek at the negative; Pick Better | M | large | brief | Decide: which first, and where they live (tool, view or dialog: [design/tabs.md](design/tabs.md)). Source: Film lab track item 7 |
| `film-fade-restoration` | **Fade Restoration** for faded prints and slides, in the Film tab | M | large | brief | Decide: the model. Source: NegPy 0.68 ([research/16](research/16-film-scan-apps-survey.md)) |
| `explain-edit` | **Explain an edit:** Auto, Match look and the Assistant say why each change helps, on the photo | M | large | brief | Decide: where the words come from and how they show. Photon's AI Coach is its whole pitch. Source: [research/17](research/17-competitors-2026.md) item 20 |
| `print-layouts` | **Print layouts and paper sizes; contact sheets as PDF; a flat 16-bit master** | M | large | brief | Contact sheets as pages already exist (`amb contact`). Decide: layout format and PDF writer. Source: [research/17](research/17-competitors-2026.md) item 3; Film lab track item 10 |
| `agent-drives-ui` | **Agents driving the whole UI**, not only edits (menus, tools, brush strokes), with undo | M | large | brief | Decide: protocol commands for UI actions ([design/ai-control.md](design/ai-control.md)). What LightCraft's MCP does. Source: [research/17](research/17-competitors-2026.md) item 14 |
| `second-window` | **A second window** for the photo (the viewer on a second screen) | M | medium | brief | Decide: how the two windows share state (Tauri). Source: RapidRAW 1.6.9 |
| `midi-controllers` | **MIDI controllers** for sliders | M | large | brief | Decide: the crate, the mapping file. Hardware consoles (Loupedeck-style). Source: RAWmakase |
| `face-part-masks` | **Face-part masks** (eyes, lips, skin) | M | large | brief | A new AI model: licence check ([own-models.md](own-models.md), `crates/amb-ai/data/models.json`). [design/masks.md](design/masks.md). Source: RapidRAW, watchlist 2026-11-04 |
| `teach-before-after-strip` | **Teach by showing, the rest:** a before/after strip per slider for this photo, on hover | M | medium | brief | [design/future-ideas.md](design/future-ideas.md#tutorial-mode) |
| `help-more-terms` | **More photography terms** in the "?" cards, terms in Learn's lessons, and translations | M | medium | ready | Text in `crates/amb-gui/ui/basics.json` and `crates/amb-gui/ui/lessons.json`. Source: owner, 2026-10-20 |
| `ai-coach` | **An optional AI coach** (bring-your-own-AI): explains why a preset looks the way it does, or suggests the next step, with only commands a user can use | M | large | brief | [design/future-ideas.md](design/future-ideas.md#tutorial-mode). Overlaps `explain-edit`: brief them together |
| `surprise-locks` | **Surprise me with locks** (keep white balance and exposure), scope to one panel, "mutate" around the current look, film stocks, entering a seed | S–M | medium | brief | [design/future-ideas.md](design/future-ideas.md#randomize-button-surprise-me) |
| `auto-per-slider` | **Auto: Shift-click per slider**, Auto for Vibrance and Saturation, and "Auto in the style of …" via `amb match` | M | large | brief | [design/future-ideas.md](design/future-ideas.md#auto-button). Calibration against Lightroom's Auto is `fit-auto` in section 5 |
| `usage-insights` | **Usage insights** (opt-in, local counters of panels used, settings not rendered, timings, crashes) and "share usage report" | M | large | brief | [design/future-ideas.md](design/future-ideas.md#feedback-button-and-usage-insights). Decide: the counter file and what is shown before sending |
| `feedback-relay` | **Send feedback straight from the app** (a relay that files the issue) and `amb feedback`; a feedback label set and a monthly triage | M | large | brief | [design/future-ideas.md](design/future-ideas.md#feedback-button-and-usage-insights). Decide: who runs the relay |
| `film-scans-in` | **Scans in:** flat-field from a shot of the bare light, a channel-bleed matrix per film type or from a colour chart, half-frame split, a strip split into frames, stitching a large negative | M each | large | brief | Split into five rows when briefed. Source: Film lab track item 8 |
| `film-bw-processes` | **B&W processes and finishing:** lith, cyanotype, Sabattier, chemical toners, edge burn, filed carrier, chroma denoise that keeps edges, a skin chroma ceiling | S–M each | medium | brief | Some as effect plugins ([design/plugin-kinds.md](design/plugin-kinds.md)). Source: Film lab track item 11 |
| `film-metadata` | **Film metadata:** stock, camera and development per roll, search terms such as `film:… iso:>=400`, and a hot folder | M | large | brief | Touches the sidecar: decide the tags. Source: Film lab track item 12 |
| `film-print-proof` | **Printer and paper soft proof**, applied once to the composed page | M | large | owner | Needs the user's printer and paper ICC profiles. Source: Film lab track item 10 |
| `library-background-index` | **Index whole drives in the background** while the editor runs, with a preview cache for very large libraries | M | large | brief | [design/library.md](design/library.md) "Not yet". Source: Photo manager track item 1, owner 2026-10-19 |
| `catalog-queries` | **Queries and full-text search straight from the catalogue** (FTS5); smart collections and All photos without checking every file | M | large | brief | [design/library.md](design/library.md) "Not yet"; [GAPS.md](GAPS.md#3-library-and-catalog) |
| `keyword-hierarchies` | **Keyword hierarchies and synonyms** (read, edit, write `lr:hierarchicalSubject`) | M | large | brief | Touches the sidecar. [design/library.md](design/library.md) "Not yet" |
| `import-more` | **Import:** move instead of copy, eject the card, pick single photos, resumable import | M | medium | ready | [design/library.md](design/library.md) "Import". Source: [SCOPE.md](SCOPE.md) §8 |
| `library-safety` | **Safety:** checksums in the catalogue to catch silent corruption, a backup check, space per folder, year and camera; automatic catalogue backups | M | large | brief | Source: Photo manager track item 7, owner 2026-10-19 |
| `library-several-computers` | **A library on a NAS or synced folder** used from more than one machine, each with its own catalogue cache | L | large | brief | Source: Photo manager track item 8 |
| `library-videos` | **Videos:** list and play the clips next to the photos, with ratings and keywords | L | large | brief | Source: Photo manager track item 10 |
| `face-grouping` | **Local face grouping** (people by name), optional, off by default, never uploaded | L | large | brief | Model licence check first. Left out on purpose for now ([GAPS.md](GAPS.md#3-library-and-catalog)). Source: Photo manager track item 5 |
| `share-gallery-export` | **A slideshow as a video or web page, and a static web gallery** from a collection, with music | M | large | owner | [SCOPE.md](SCOPE.md) calls web gallery modules a non-goal for v1: the owner decides. Source: Photo manager track item 9 |
| `tethering` | **Tethered shooting** (libgphoto2) | L | large | brief | Phase 4 item in [SCOPE.md](SCOPE.md) §8; decide the crate and the platform story |
| `focus-stacking` | **Focus stacking** (after HDR and panorama, as a merge) | L | large | brief | [design/hdr-merge.md](design/hdr-merge.md) and [design/panorama.md](design/panorama.md) are the pattern. Source: Redlamp, RapidRAW |
| `plugin-panels` | **Free-form plugin panels and dialogs** (effects get controls from their manifest only), and a JIT for pixel effects | L | large | brief | [design/plugin-kinds.md](design/plugin-kinds.md); [ai/plugins.md](ai/plugins.md) |
| `plugin-data-kinds` | **Data plugins** (presets, LUTs, profiles); recipes shared through the hub; CI that rebuilds hub plugins from source | M | large | brief | [design/plugin-kinds.md](design/plugin-kinds.md); [GAPS.md](GAPS.md#15-platforms-and-packaging) |
| `hub-ratings` | **Hub: ratings and screenshots**, very large layouts without a hand step | M | medium | brief | [design/customization-and-hub.md](design/customization-and-hub.md) §4 |
| `ui-tests-more` | **More UI test scripts** (masks, crop, develop panels) and the same scripts on macOS and Windows | M | medium | ready | [recipes.md](recipes.md) (UI tests); `crates/amb-gui/tests/ui/`. Source: owner, 2026-10-22 |
| `generative-tools` | **Generative fill and expand, relight, face reshaping** | L | large | owner | Large models, outside our niche for now; [SCOPE.md](SCOPE.md) says "not planned" for relight. Owner decides. Source: Rembrandt, RapidRAW, Lightroom |
| `web-build` | **A web build or a self-hosted browser library** | L | top | brief | An architecture decision (engine in WebAssembly, library server). Source: LightCraft, Rembrandt |
| `android-app` | **An Android app** (iOS is effectively impossible under the GPL) | L | top | owner | [research/08](research/08-cross-platform-and-mobile.md); [SCOPE.md](SCOPE.md) §8. Owner decides if and when |

## 4. Gaps worth closing

What is missing or approximate in what exists, most important first. Each Notes cell links its section of [GAPS.md](GAPS.md).

| Id | What | Size | Tier | Ready | Notes |
|---|---|---|---|---|---|
| `trial-interaction-sidecars` | **Add the interaction pairs to the trial's sweep plan** (Highlights × Whites, Shadows × Blacks, Contrast × Exposure, Vibrance × Saturation, Texture × Clarity) before the Lightroom month | M | medium | ready | [lightroom-trial-plan.md](lightroom-trial-plan.md); [GAPS §14](GAPS.md#14-testing-and-calibration) |
| `gpu-preview-cpu-steps` | **GPU versions of the steps that still run on the CPU inside the GPU preview:** camera profile tables, defringe, spots, masks, LUTs, blank-edge fill | L | large | brief | [design/gpu.md](design/gpu.md); [GAPS §13](GAPS.md#13-performance-and-gpu). Decide the order by measured cost |
| `gpu-stage-cache` | **Caching per stage** in the GPU preview, and region renders that take their globals from the whole photo, not a 1600 px copy | L | large | brief | [design/gpu.md](design/gpu.md); [GAPS §13](GAPS.md#13-performance-and-gpu) |
| `lens-warp-gpu` | **Lens warp and defringe on the GPU** (about 1.5 s per 24 MP on the CPU) | M | large | brief | [GAPS §13](GAPS.md#13-performance-and-gpu); [design/lens-corrections.md](design/lens-corrections.md) |
| `native-gpu-viewport` | **A native GPU viewport** instead of JPEG previews in the webview | L | top | brief | An architecture decision; wry `build_as_child` is X11-only on Linux. [GAPS §2](GAPS.md#2-ui); [research/08](research/08-cross-platform-and-mobile.md) |
| `gpu-precision` | **fp16 against fp32, storage-texture formats and buffer limits** measured on Intel, AMD, NVIDIA and Apple GPUs | M | large | owner | Needs several GPUs. [GAPS §13](GAPS.md#13-performance-and-gpu) |
| `perf-budgets` | **Benchmark CI and performance budgets** (slider latency, import, export, mask count, catalogue size) | M | large | brief | [GAPS §13](GAPS.md#13-performance-and-gpu); [SCOPE.md](SCOPE.md) §7 |
| `demosaic-choice` | **Demosaic choice** (AMaZE, LMMSE), X-Trans 3-pass, highlight rebuild past two clipped channels, capture sharpening | L | large | brief | [GAPS §4](GAPS.md#4-raw-decoding-and-colour) |
| `raw-support-gaps` | **4-colour CFA sensors, cameras without a colour matrix, a LibRaw fallback in a sandboxed helper**, and a camera-support process (fitted fallback profiles, sample submissions) | L | large | brief | [GAPS §4](GAPS.md#4-raw-decoding-and-colour); [GAPS §1](GAPS.md#1-product-legal-and-process) |
| `input-formats` | **HEIC, AVIF and PSD input**, LUT-based ICC profiles, a raw and its JPEG as two separate photos | M | large | brief | [GAPS §4](GAPS.md#4-raw-decoding-and-colour) |
| `adobe-raw-xmp-profiles` | **Read Adobe Raw `.xmp` profiles** (Adobe Color, the default) and decode Creative profiles (`Look`, `RGBTable`) | L | large | owner | Needs the two facts in Help wanted. [GAPS §10](GAPS.md#10-presets-and-compatibility); [design/camera-profiles.md](design/camera-profiles.md) |
| `base-look` | **Replace the placeholder base look** with fitted per-camera base looks | L | large | owner | Needs trial data (`lightroom-trial`). [design/camera-profiles.md](design/camera-profiles.md); [GAPS §4](GAPS.md#4-raw-decoding-and-colour) |
| `tone-equaliser` | **Tone equaliser and a native scene-referred mode** (filmic, sigmoid, AgX) | L | large | brief | [GAPS §5](GAPS.md#5-tone-and-colour-models); [SCOPE.md](SCOPE.md) §3 |
| `legacy-tone-tags` | **Render the legacy tone-curve tags, custom white balance without Temperature, and the HDR editing tags** | M | large | brief | Touches presets. [GAPS §4](GAPS.md#4-raw-decoding-and-colour); [GAPS §10](GAPS.md#10-presets-and-compatibility) |
| `masks-polygon-auto` | **Polygon lasso and an edge-aware auto-mask brush** | M | large | brief | [design/masks.md](design/masks.md); [GAPS §6](GAPS.md#6-local-adjustments-and-retouch) |
| `masks-unrendered` | **Render Moire, LocalHue, LocalGrain and LocalPointColors inside masks; depth-range masks** | M | large | brief | [GAPS §6](GAPS.md#6-local-adjustments-and-retouch) |
| `masks-ai-quality` | **Better Select Sky (sunset, storm), Select People as separate people, landscape parts** | M | large | brief | [design/masks.md](design/masks.md); [GAPS §6](GAPS.md#6-local-adjustments-and-retouch) |
| `blemish-heal` | **Blemish removal with the heal brush** good enough on real portraits | M | large | brief | A soft edge still mixes some blemish texture. [design/spot-removal.md](design/spot-removal.md); owner, 2026-10-19 |
| `spot-drag` | **Move brushed spots by dragging** in the editor | S | medium | ready | Resize and `set_spot` exist. [design/spot-removal.md](design/spot-removal.md) |
| `ai-models-gaps` | **Super Resolution, Raw Details and Lens Blur (depth)** as local models, and a permissive-only model manifest with a CI gate | L | large | brief | Licence first: no non-commercial models. [GAPS §6](GAPS.md#6-local-adjustments-and-retouch); [GAPS §7](GAPS.md#7-detail-and-effects) |
| `lens-profiles-more` | **Built-in lens profiles in maker notes** (MFT, Fujifilm X, many compacts), Sony vignetting and CA tags, fisheye profiles | M | large | brief | [design/lens-corrections.md](design/lens-corrections.md); [GAPS §8](GAPS.md#8-lens-and-geometry) |
| `merge-gaps` | **HDR and panorama merge:** rotation and scale in alignment, JPEG brackets, Create stack, Boundary Warp, Fill Edges | L | large | brief | [design/hdr-merge.md](design/hdr-merge.md); [design/panorama.md](design/panorama.md) |
| `export-formats` | **Export gaps:** lossy WebP and JPEG XL, ICC and 10-bit AVIF, DNG of the developed image, HDR AVIF and JXL, gamut mapping, OpenEXR metadata | L | large | brief | [GAPS §11](GAPS.md#11-export); [ARCHITECTURE.md](ARCHITECTURE.md) "Export" |
| `ai-control-gaps` | **AI control gaps:** `--json` on every command, protocol `list_presets`, Assistant placing masks, crops and spots, a pending diff to Accept, `amb match` with tone curve and texture, Learn-my-style | L | large | brief | [design/ai-control.md](design/ai-control.md); [GAPS §12](GAPS.md#12-ai-control) |
| `wayland-colour` | **Wayland monitor profiles** (colord), LUT-based monitor profiles, thumbnails in the monitor's profile | M | large | brief | [GAPS §15](GAPS.md#15-platforms-and-packaging); [research/08](research/08-cross-platform-and-mobile.md) |
| `fuzz-and-fidelity-ci` | **Coverage-guided fuzzing** (cargo-fuzz), ICC and raw decoding fuzzed, a pinned `rust-toolchain.toml`, fidelity metrics in CI | M | large | brief | [GAPS §1](GAPS.md#1-product-legal-and-process); [GAPS §14](GAPS.md#14-testing-and-calibration) |
| `governance-docs` | **Governance:** contributor AI-use policy, a no-rug-pull charter, data-safety invariants, a decisions record (`docs/decisions/`) | S | medium | brief | [GAPS §1](GAPS.md#1-product-legal-and-process); [SCOPE.md](SCOPE.md) §10. The owner decides the policy |
| `code-over-budget` | **Split the files past the size budget** and keep the UI scripts' load order checked | M | medium | ready | `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs`; [cleanup.md](cleanup.md); [GAPS §16](GAPS.md#16-code-health) |
| `agent-measurement` | **Measure whether the agent maps help:** fresh agents on a fixed task set with and without recipes (rounds A1 and B1 are done) | M | large | brief | [design/agent-navigation.md](design/agent-navigation.md) |

## 5. Waiting on something outside

| Id | What | Size | Tier | Ready | Notes |
|---|---|---|---|---|---|
| `lightroom-trial` | **Run the Lightroom Classic measurement month:** build the kit first, shoot the own photos, run the sweeps and batches, run the plug-in in Lightroom, finish and copy out | L | large | owner | Needs a Lightroom Classic subscription. [lightroom-trial-plan.md](lightroom-trial-plan.md); [design/calibration.md](design/calibration.md) |
| `fit-model-constants` | **Fit the model constants to the trial renders:** every `Calibration` section, Highlights/Shadows, tone, HSL, Texture, Clarity, Dehaze, sharpening, noise reduction, grain, vignette, masks, lens, Amount | L | large | owner | Waits on `lightroom-trial` output. Only fitted numbers go in the repo. [GAPS §14](GAPS.md#14-testing-and-calibration) |
| `fit-transform-crop` | **Check the crop's angle sign and corner semantics, Transform signs and strengths, and Upright against Lightroom** | M | large | owner | Waits on `lightroom-trial`. [design/crop.md](design/crop.md); [design/transform.md](design/transform.md) |
| `point-color` | **Point Color** (`PointColors`) and the B&W mixer fit | M | large | owner | Needs a Lightroom sample of `PointColors`. [GAPS §4](GAPS.md#4-raw-decoding-and-colour) |
| `fit-preset-amount` | **Measure Preset Amount and profile Amount**, and the tag registry's ranges, defaults and steps | M | large | owner | Waits on `lightroom-trial`. [GAPS §10](GAPS.md#10-presets-and-compatibility) |
| `fit-auto` | **Calibrate Auto against Lightroom's Auto** (record what it writes) | M | large | owner | Waits on `lightroom-trial`. [design/future-ideas.md](design/future-ideas.md#auto-button) |
| `red-eye` | **Red eye and pet eye** | M | large | owner | Needs a Lightroom sidecar with red eye: the `RedEyeInfo` format is not documented. [design/spot-removal.md](design/spot-removal.md) |
| `preset-corpus` | **Apply every preset the owner has to the ten raws** (an end-to-end preset test) and run `amb preset census` | M | medium | owner | The owner's preset collections are never committed |
| `color-chart-capture` | **A real ColorChecker capture** for colour fitting | S | small | owner | The synthetic chart is approximate. [GAPS §14](GAPS.md#14-testing-and-calibration) |
| `fujifilm-looks-fit` | **Fit the Film recipe looks** to in-camera re-renders of each film simulation | M | large | owner | Needs the owner's Fujifilm camera. [design/film-look-measurement.md](design/film-look-measurement.md) |
| `film-stock-profiles` | **Measured film stock profiles** from scans of real film shot with a reference target, and a way to share them as data | L | large | owner | Needs real film scans. [design/film-negatives.md](design/film-negatives.md) |
| `auto-density-default` | **Make Auto Density and Auto Grade the default** if they look right on a real roll | S | small | owner | The owner tries both switches ([WHATS-NEXT.md](WHATS-NEXT.md)) |
| `dust-infrared` | **Dust from a scanner's infrared channel** (a fourth channel or an `_ir` file) | M | large | owner | Parked until there is a scan with one. [design/dust.md](design/dust.md) |
| `mac-windows-check` | **Try the things only tried on Linux** on a Mac and on Windows: menu bar, trash, custom interfaces, release-build preview times (`AMB_TIMING=1`), GPU export and AI speed (`AMB_GPU=off`, `AMB_AI_GPU=off`) | M | medium | owner | Needs the owner's Macs and Windows machine. [GAPS §2](GAPS.md#2-ui); [GAPS §13](GAPS.md#13-performance-and-gpu) |
| `packaging-rest` | **Windows signing (SignPath), Apple signing secrets, Flatpak, MSIX, a universal DMG, auto-update** | M | medium | owner | Needs the owner's signing accounts and secrets. [releasing.md](releasing.md); [GAPS §15](GAPS.md#15-platforms-and-packaging) |
| `lrcat-real-catalogues` | **Try `.lrcat` import and opening in place on real Lightroom Classic catalogues** of several versions; convert smart collections | M | large | owner | Needs real catalogues. [design/library.md](design/library.md); [GAPS §3](GAPS.md#3-library-and-catalog) |
| `sample-raw` | **The owner's sample raw** replaces the stand-in sample photo | S | small | owner | After v0.1. Spec: a bright sky and deep shadows about 1 stop under, a colour cast with a grey in it, muted and saturated colours, a slightly tilted horizon, no people or private places, GPS stripped, CC0 or CC BY, DNG preferred, under 30 MB. URL and checksum in `crates/amb-gui/src/commands/sample.rs` |
| `counsel-review` | **A lawyer's review before 1.0** (or before donations at scale): purchased-preset seller licences, the DNG SDK licence against the GPL, the patent notes | S | small | owner | Needs counsel. [legal.md](legal.md); [GAPS §1](GAPS.md#1-product-legal-and-process) |
| `trademark-search` | **Trademark searches** (TMview, WIPO, USPTO, EUIPO) for Ambrotype, and own the `amb:` namespace domain | S | small | owner | [SCOPE.md](SCOPE.md) "Name"; [legal.md](legal.md) |
| `owner-decisions` | **Decisions only the owner can make:** the funding model, the fidelity target that defines "compatible" (proposal: median dE00 < 2, p95 < 5 over the top 20 tags), who holds the Lightroom subscription, contacting RapidRAW's maintainer, a usability test with Lightroom switchers, the cloud Lightroom API or computer use for the trial's part 4 | S | small | owner | [GAPS §1](GAPS.md#1-product-legal-and-process); [SCOPE.md](SCOPE.md) §12; [lightroom-trial-plan.md](lightroom-trial-plan.md) |
| `scroll-mode-choice` | **Confirm the side columns scroll as one**, or bring back the switch between one scroll and blocks that each scroll | S | small | owner | The switch was built and removed on 2026-11-10 ("one scroll won"); not recorded as the owner's choice. Source: owner, 2026-11-10 |
| `reddit-threads` | **Paste Reddit threads by hand** for the recheck (our tools cannot read Reddit) | S | small | owner | [research/watchlist.md](research/watchlist.md) |

## Help wanted

Things anyone with the right kit can do; open an issue first so two people don't do the same one.

- **Try the editor on a Mac or Windows machine** and report what feels wrong (the Feedback button opens a pre-filled GitHub issue, or an email without a GitHub account). Most testing so far is on Linux.
- **Run `amb preset census <your preset folders>`** and share the output (setting names and counts only, no preset files). It ranks what to build next.
- **Lightroom samples:** if you have Lightroom, the measurements in [lightroom-trial-plan.md](lightroom-trial-plan.md) let us fit our models (kit: `tools/lightroom-kit/make-kit.sh`). Share only fitted numbers, never Adobe's renders or files.
- **Film simulation references:** raws plus in-camera re-renders with each film simulation, so the Film recipe looks can be fitted ([design/film-look-measurement.md](design/film-look-measurement.md)).
- **Creative and Adobe Raw profiles** (`Look`, `RGBTable`) need two facts: the 85-character alphabet the DNG SDK uses for `Table_<md5>` text, and the names of the `crs:` properties that hold table fingerprints in an Adobe Raw profile `.xmp`. Facts only; copy no table data.

## Working notes

- Parallel agents, the checks to run and the Linux packages the editor needs: [AGENTS.md](../AGENTS.md) and [desktop-setup.md](desktop-setup.md). A debug `target/` grows to about 20 GB.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
