# Flightbroom: project scope

*A free, open-source Lightroom alternative that renders purchased Lightroom presets faithfully.*

Status: scoping draft, 2026-10-03. This document synthesises the research in [`docs/research/`](research/) and the digests of Adobe's official help in [`docs/adobe-docs/`](adobe-docs/). Where it states a fact, the linked doc gives the source. Where it gives an estimate or recommendation, it says so.

---

## 1. TL;DR

1. **The differentiator is preset fidelity, and nobody does it well today.**
   - darktable imports only a subset of sidecar XMP tags (about 50). It has no WB, Highlights/Shadows/Whites, Contrast, Texture, Dehaze, Color Grading, profiles or masks, and it does not import standalone presets.
   - RawTherapee has no Lightroom importer at all.
   - RapidRAW (AGPL, about 9k stars, Rust + Tauri + wgpu) has a "universal preset importer". It is a regex-based remap of about 40 tags with constants tuned by eye (e.g. `shadows × 1.5`, `hue × 0.75`), and it ignores profiles/Looks, calibration, parametric curves and masks.
   - Sources: [03 §6](research/03-darktable-survey.md), [02 §5](research/02-preset-and-profile-formats.md), [07 §3](research/07-rapidraw-survey.md).
2. **Pixel-exact parity with Adobe is impossible; "visually indistinguishable on most images" is achievable for most preset content.**
   - A preset is a sparse set of `crs:` parameters fed to Adobe's closed algorithms.
   - Some controls can be reproduced almost exactly from public specs (the DNG SDK and spec):
     - white balance (Robertson temperature/tint)
     - DCP application order
     - point curves (natural cubic spline)
     - camera matrices
   - The rest must be **fitted against Lightroom renders** with a black-box calibration harness: Highlights/Shadows, Clarity/Texture/Dehaze, HSL, Color Grading.
   - Sources: [05 §1–2, §18](research/05-processing-algorithms.md).
3. **The single hardest problem is colour, not sliders.** Most commercial presets reference an Adobe camera profile ("Adobe Color", "Adobe Standard", creative profiles). Those profiles are Adobe-copyrighted per-camera DCPs and XMP profiles with embedded LUTs (`crs:Table_…`, Ascii85-variant + zlib, undocumented). We can't ship them. Our options are:
   - read the user's own installed Adobe profiles, if present
   - decode LUTs in preset/profile files the user owns
   - ship our own neutral "Adobe-like" base profiles fitted per camera family
   - Source: [02 §3](research/02-preset-and-profile-formats.md).
4. **Recommendation: build the "preset-compatible engine" first, as a headless Rust library and CLI, with a calibration harness.**
   - Decide afterwards whether our own UI or a RapidRAW-derived UI hosts it.
   - This puts the riskiest, most differentiating work first.
   - It keeps the UI-framework decision reversible.
   - The engine is useful on its own: batch rendering, a library other apps can embed.
5. **Licence recommendation: GPL-3.0-or-later.**
   - It lets us port proven algorithms directly from darktable, RawTherapee and ART (all GPLv3) and use exiv2.
   - That saves person-years.
   - It's the owner's call; see §6.
6. **Order of magnitude (rough estimate, not measured):**
   - Engine + CLI with PV2012 global controls and a fidelity report: about 6 person-months.
   - Usable develop-only desktop app: about 12–15 person-months cumulative.
   - Lightroom-class v1 (masks, lens, catalog/DAM, export): about 2.5–3.5 person-years.

---

## 2. Vision, goals, non-goals

**Goals**
- Free, open-source, offline-first, cross-platform desktop app (Windows, macOS, Linux).
- Non-destructive raw development with a Lightroom-like workflow and vocabulary, so Lightroom users feel at home.
- **Import and apply Lightroom/ACR presets** (`.xmp` and legacy `.lrtemplate`), including the Amount slider, with a per-preset **fidelity report** (exact / approximated / ignored per tag).
- Read and write Lightroom-compatible XMP sidecars (`crs:` namespace) so edits round-trip where possible.
- Migrate from Lightroom Classic: read `.lrcat` (SQLite) read-only, covering folders, keywords, ratings, collections and develop settings.
- A headless engine and CLI for batch rendering and automated preset testing.

**Non-goals (at least for v1)**
- Cloud sync, mobile apps, web gallery/Book/Slideshow modules, Adobe-style generative AI (cloud models), Lightroom plug-in (Lua SDK) compatibility.
- Pixel-exact equality with Lightroom.
- Using the name "Lightroom" or Adobe trademarks in the product name, logo or marketing. Descriptive compatibility claims such as "imports Lightroom® presets" are fine with a trademark notice.

---

## 3. What Lightroom is (feature map)

Full detail lives in [01-lightroom-feature-inventory](research/01-lightroom-feature-inventory.md). It is written from secondary sources and background knowledge, and tagged by confidence. The source-only digests of Adobe's own help pages are the authoritative reference:

- [classic-develop.md](adobe-docs/classic-develop.md): every Develop panel, slider and mask type, plus process versions
- [classic-library.md](adobe-docs/classic-library.md): catalog, previews, import, keywords, collections, metadata
- [classic-output.md](adobe-docs/classic-output.md): export, print, publish, tethering, plug-ins
- [camera-raw.md](adobe-docs/camera-raw.md): the shared ACR engine, presets, profiles, XMP and DNG facts
- [cloud-editing.md](adobe-docs/cloud-editing.md) and [cloud-organize-and-share.md](adobe-docs/cloud-organize-and-share.md): Lightroom (cloud), AI features, differences from Classic

**Develop pipeline surface to replicate (PV2012 / PV5 / PV6):**
- Profile + Amount
- White balance (Temp/Tint)
- Exposure, Contrast, Highlights, Shadows, Whites, Blacks
- Texture, Clarity, Dehaze, Vibrance, Saturation
- Parametric and point tone curve (+ RGB)
- Color Mixer / HSL (8 bands) and B&W mixer
- Point Color
- Color Grading (3-way + global, Blending, Balance)
- Detail: sharpening and NR
- Lens corrections (profile, CA, defringe)
- Transform / Upright
- Effects (post-crop vignette, grain)
- Calibration
- Crop, heal/clone/remove
- Masks:
  - linear, radial, brush
  - colour, luminance and depth range
  - AI subject, sky, background, objects, people parts, landscape
  - add/subtract/intersect
- Enhance (Denoise, Raw Details, Super Resolution), Lens Blur
- HDR / panorama merge
- HDR editing and output

---

## 4. Landscape and what we learn from each

| Project | Licence | Stack | Relevance |
|---|---|---|---|
| **darktable** | GPL-3 | C, GTK, OpenCL | About 110 IOPs and 520k LOC. A scene-referred pixelpipe with ROI/tiling/caching. Mature demosaic (AMaZE, RCD, LMMSE, Markesteijn), profiled denoise, lensfun. Recently added **ONNX-based AI object masks** (`src/develop/masks/object.c`, `src/common/ai/`). Partial LR sidecar import. Best algorithm donor. [03](research/03-darktable-survey.md) |
| **RawTherapee / ART** | GPL-3 | C++, GTK | High-quality algorithms: capture sharpening, CIECAM, wavelets, Locallab, haze removal. `rtengine/dcp.cc` is the best open DCP implementation. No LR import. [04](research/04-oss-ecosystem.md) |
| **RapidRAW** | **AGPL-3** | Rust, Tauri, React, wgpu | Closest product to our goal. About 37k LOC of Rust and 2.6k of WGSL. Fast GPU pipeline, AI masks/inpaint/denoise via ONNX, Android build. Its colour science is its own: no DCP/ICC, multiplier WB, hand-tuned tone mapper. JSON sidecars, no catalog DB, solo maintainer with heavy churn. Strong **UI/UX reference**, weak base for Adobe fidelity. [07](research/07-rapidraw-survey.md) |
| vkdt | BSD-2 | C, Vulkan | GPU node-graph design reference. |
| digiKam | GPL-2+ | C++, Qt | DAM reference: faces, metadata, huge libraries. |
| LibRaw / rawler / rawspeed | LGPL / LGPL / LGPL | C++ / Rust / C++ | Raw decoding options. rawler is used by RapidRAW. |
| lensfun | LGPL-3 (lib), CC-BY-SA (DB) | C | Lens profiles. Adobe LCP files can't be shipped. |
| lcms2, exiv2, Adobe XMP Toolkit | MIT, GPL-2+, BSD | C/C++ | Colour management and metadata. |
| Adobe DNG SDK | Adobe EULA (permissive, not OSI) | C++ | Reference for DCP and the baseline render. Re-implement from the spec rather than copying. [05 §0–1](research/05-processing-algorithms.md) |
| SAM2, BiRefNet, Depth Anything V2-Small, LaMa, SCUNet | Apache/MIT | ONNX | Permissive models for AI masks, depth, removal and denoise. **RMBG-1.4/2.0 and Depth Anything Base+ are non-commercial; don't bundle them.** [06 §5](research/06-tech-stack-options.md) |

---

## 5. Strategic options

| Option | Time to first demo | Fidelity ceiling | Control/arch quality | Licence |
|---|---|---|---|---|
| A. Contribute a compat renderer to RapidRAW | Fastest | Medium. Upstream's look and shader churn conflict with numeric Adobe matching. | Low (solo maintainer) | AGPL |
| B. Fork RapidRAW, replace the engine | Fast | High | Medium. We inherit about 90k LOC we didn't design. | AGPL |
| C. Fork darktable or ART and add an LR compat layer + new UX | Medium | High | Low/medium. GTK/C legacy and UX debt. | GPL-3 |
| **D. New engine library + CLI first, then the UI (own, or B as fallback)** | Slower | **Highest** | **High** | Our choice |

**Recommendation: D, staged so B stays open.**
- Phases 0–1 build the engine and harness as standalone crates (§8). That's the differentiating work under any option.
- At the Phase 2 gate, choose one of two paths:
  - build our own shell (Tauri 2 + native wgpu viewport, or Qt 6)
  - fork RapidRAW's UI and swap in our engine behind its adjustments model
- Two inputs settle the gate: the viewport spike result and a conversation with RapidRAW's maintainer.
- A GPL-3.0-or-later engine can be combined into an AGPL fork (GPLv3 §13 allows it), so no work is wasted either way.

---

## 6. Licensing (decision needed)

The research agents disagreed. This section corrects the record (see the editorial notes in [04 §E](research/04-oss-ecosystem.md)).

- **Permissive licences (Apache-2.0/MIT/MPL)** maximise adoption, but they **forbid porting darktable/RawTherapee/ART code**: only clean reimplementation from papers is allowed. They also exclude exiv2 (GPL-2+).
- **GPL-3.0-or-later** (recommended):
  - port GPL algorithms directly, with attribution
  - use exiv2, LibRaw, lensfun and lcms2
  - combine with AGPL RapidRAW code if we ever take option B (the result is then AGPL)
  - the cost is that proprietary forks are impossible, which fits a "free forever" project
- **AGPL-3.0** buys nothing for a desktop app, beyond making RapidRAW code directly usable.
- **Data licences:**
  - The lensfun DB is CC-BY-SA: ship it as a separate data package.
  - Models are allowed only from a permissive-licence manifest, with a CI gate.
- **Adobe assets we must never ship:**
  - Adobe camera profiles (DCP/XMP)
  - LCP lens profiles
  - Adobe presets
  - anything from decompiling Lightroom
- **Users' purchased presets:** we read them locally and never redistribute or convert them for sharing. Some seller licences forbid conversion; reading them for application in the user's own editor is the defensible interoperability use. This needs counsel before 1.0.
- **Trademark:** don't use "Lightroom" in the product name. "Flightbroom" is an evident pun. Run a quick trademark check before a public launch.

---

## 7. Preset compatibility: what we can reach

The tag-level detail (full `crs:` table, file formats, Amount, Looks, masks) is in [02](research/02-preset-and-profile-formats.md). The per-control algorithms and the harness are in [05](research/05-processing-algorithms.md).

**Fidelity tiers. These are estimates, to be replaced with measured ΔE00 once the harness exists.**

| Tier | Controls | Approach |
|---|---|---|
| Near-exact (ΔE00 < 1) | Temp/Tint, Exposure, point curves (+RGB), DCP application (with user's DCP), crop/rotate, vignette, gradient/radial mask geometry | Implement from the DNG spec/SDK behaviour: Robertson isotherms, natural cubic spline, ProPhoto-linear working space, documented render order |
| Close (ΔE00 1–3) | Contrast, Whites/Blacks, parametric curve, Vibrance/Saturation, HSL, Color Grading, Calibration, Clarity, Dehaze, sharpening, grain | Our own parametric models, fitted to Lightroom single-slider sweeps |
| Approximate | Highlights/Shadows on HDR scenes, Texture, NR, Upright auto, AI masks, Lens Blur, AI Denoise | Best open algorithm (local Laplacian / guided filter / SAM2 / Depth Anything), tuned visually + harness |
| Not reproducible | Adobe creative/"Adobe Color" profiles without the user's own copy; **Adaptive profiles** (image-adaptive AI, intensity 0–200) and adaptive/AI presets; Generative Remove | Fallback base profile + warning; our own auto-tone as a stand-in; LaMa inpainting as substitute |

**Importer design (summary of [02 §7](research/02-preset-and-profile-formats.md)):**
- **Parsing.** A real namespace-aware XMP/RDF parser plus a Lua-table subset parser for `.lrtemplate`.
- **Canonical model.** It mirrors `crs:` names 1:1, is sparse (present ≠ default), keeps unknown tags losslessly, and dispatches per process version.
- **Tag registry.** A JSON registry records status, range, default and scale per tag.
- **Amount.** It is a lerp from current settings to the preset (curves and LUTs blend toward identity). Adobe's docs confirm two separate Amount sliders: the Preset Amount (ACR 14.4+, June 2022), and the profile Amount, which applies only to Adaptive, Artistic, B&W, Modern and Vintage profiles. The scaling math for either is not documented ([camera-raw.md](adobe-docs/camera-raw.md)), so measure it with the harness.
- **Profiles.** `CameraProfile` and `Look` resolve through a profile registry: the user's Adobe profiles if installed, else our fitted base profiles. Decoding `Look` `RGBTable`/`Table_` LUTs is a research task; no complete public decoder is known.
- **Masks.** Geometric and range masks are native. AI masks are regenerated with our own segmenters, never stored as pixels. `RetouchAreas` from presets are ignored.

**Calibration harness ([05 §18](research/05-processing-algorithms.md)):**
- **Inputs:** synthetic linear DNGs (ramps, edge patterns, hue wheels), ColorChecker shots and a CC0 raw corpus (e.g. raw.pixls.us).
- **Sweeps:** one slider at a time through Lightroom, with interaction pairs, exported as 16-bit TIFF.
- **Fitting:** fit our module parameters to minimise ΔE2000 plus an edge/SSIM term. Results live as versioned JSON calibration tables, not code.
- **CI:** track median and p95 ΔE00 per control, plus whole-preset scores on a private preset corpus.
- **Clean-room process:** observe outputs only, with no decompilation, and document the roles.
- The harness is a **Phase 1 deliverable**, not an afterthought.

---

## 8. Phased roadmap

Durations are rough person-month (pm) estimates for experienced engineers.

### Phase 0: Decisions and spikes (about 1.5 pm)
- Licence, name check, contribution model.
- Spikes:
  - wgpu 100 MP tiled pipeline + viewport in Tauri-native-surface vs Qt RHI on all 3 OSes
  - fp16 vs fp32 intermediates
  - `ort` + SAM2-tiny on CoreML/DirectML/CPU
  - lrcat read on 3 catalog versions
- Collect a preset corpus and run a tag census to prioritise by frequency (private; not committed).
- Exit: decisions recorded in `docs/decisions/` (ADRs).

### Phase 1: Engine + CLI + harness (about 5 pm). *The differentiator.*
- Cargo workspace (`flb-core`, `flb-io`, `flb-color`, `flb-gpu`, `flb-presets`, `flb-cli`) per [06 §8](research/06-tech-stack-options.md).
- Raw decode (rawler or LibRaw), black/white levels, demosaic (port RCD/AMaZE), highlight reconstruction.
- Camera matrix → ProPhoto-linear; DCP reader and applier (HueSatMap, LookTable, ToneCurve, dual-illuminant interpolation).
- WB Temp/Tint (Robertson), Exposure, Contrast, Highlights/Shadows/Whites/Blacks (first model), Vibrance/Saturation, HSL, point and parametric curves, Color Grading, vignette, grain, crop.
- CPU reference kernels first, then WGSL kernels with a CPU/GPU equivalence test in CI.
- Preset importer (XMP + lrtemplate), tag registry, fidelity report, Amount.
- `flb render --preset x.xmp in.raw out.tif`.
- Calibration harness and first fitted tables; fidelity dashboard in CI.
- Exit: on the corpus, the median ΔE00 vs Lightroom for the top 20 most-used tags is under target (proposal: < 2).

### Phase 2: Develop app MVP (about 7 pm)
- Gate: own shell vs RapidRAW-fork UI (see §5).
- Folder-based browser with grid, filmstrip, rating/flag/label and basic filters.
- Develop panels for everything from Phase 1, plus histogram/clipping, before/after, history/snapshots, copy/paste settings, presets panel (import, apply with Amount, save).
- Sharpening + NR (port), Texture/Clarity/Dehaze (first fitted versions).
- XMP sidecar read/write (`crs:`).
- Export JPEG/TIFF/PNG (+AVIF/JXL), sRGB/P3/AdobeRGB/ProPhoto, resize, output sharpening, metadata, naming templates.
- Colour-managed display (ICC display profile).
- Packaging: Flatpak/AppImage, MSI (SignPath), notarised DMG.

### Phase 3: v1 "Lightroom-class" (about 12–18 pm)
- **Masks:** linear, radial, brush, colour/luminance range; add/subtract/intersect; the local-adjustment slider set; preset masks.
- **Lens, geometry, retouch:** lens corrections (lensfun + user-supplied LCP parsing), CA/defringe, Upright (port ART/darktable `ashift`), heal/clone.
- **Catalog:** SQLite catalog (WAL, FTS5), import copy/move/add, keywords (hierarchy, synonyms), collections and smart collections, stacks, virtual copies, preview pyramid / smart-preview equivalent.
- **Migration:** `.lrcat` import (read-only), covering folders, keywords, collections, ratings and develop settings through the preset importer.
- Calibration panel, Point Color, B&W mixer, soft proofing.
- AI subject/sky masks (SAM2/BiRefNet via `ort`; on-demand model download).

### Phase 4: Later
- More AI: people parts, objects, depth range, Lens Blur (Depth Anything V2-S), AI denoise (SCUNet/NAFNet-class), content-aware remove (LaMa), Super Resolution.
- HDR/panorama merge, HDR editing and HDR export (gain maps), tethering (libgphoto2), print, map/GPS, face recognition, plugin API (Lua → WASM), adaptive presets.

---

## 9. Architecture summary

This is the full argument from [06](research/06-tech-stack-options.md), condensed.

- **Rust** workspace. FFI only where unavoidable (LibRaw, lcms2, lensfun, exiv2).
- **Engine = pure headless library.**
  - The recipe is versioned data.
  - The pipeline graph uses ROI, tiling and per-node hash caching.
  - It has a CPU (rayon + SIMD) reference backend and a **wgpu/WGSL** GPU backend, kept equivalence-tested.
  - Processing is scene-referred, in ProPhoto-primaries linear (matches ACR's working space).
- **Catalog service:** SQLite with a single writer, XMP sidecars as interchange, and an on-disk preview pyramid.
- **AI service:** behind a trait, using ONNX Runtime (`ort`) with CoreML/DirectML/CUDA/CPU providers and a permissive-only model manifest.
- **UI shell:** replaceable, behind a typed, pixel-free command protocol. Default is Tauri 2 with a native wgpu child surface for the viewport; Qt 6 is the fallback. The decision depends on the Phase 0 spike.
- **Testing:** golden images, preset fidelity suite, parser fuzzers.

---

## 10. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Users judge us by "does my preset look the same?" and fidelity disappoints | High | Harness from day one; transparent per-preset fidelity report; prioritise by tag frequency; profile strategy (§1.3) |
| Adobe profile dependency ("Adobe Color" et al.) | High | Detect and use the user's installed Adobe profiles locally; fitted per-camera base profiles; LUT decoding research |
| Highlights/Shadows (PV2012 local tone mapping) hard to match | High | Early local Laplacian / guided-filter experiments with sweep data (Phase 1) |
| Scope explosion (a DAM *and* a raw developer) | High | Develop-first; catalog only in v1; strict phase gates |
| UI framework / viewport performance | Medium–High | Phase 0 spike; pixel-free protocol keeps it swappable |
| Camera/lens coverage | Medium | LibRaw/rawler + lensfun; community sample pipeline (raw.pixls.us) |
| Legal (EULA, preset licences, trademarks, patents) | Medium | Clean-room process, no Adobe assets shipped, counsel review before 1.0 |
| Contributor pool for Rust + GPU + colour science | Medium | Clear crate boundaries, documented kernels, "good first kernels" |
| AI model licences and hardware variance | Medium | Permissive manifest + CI gate; CPU-quantised fallbacks |

---

## 11. Team and effort (rough)

- A minimum viable team: 2 engine/GPU engineers, 1 colour/imaging scientist (harness, profiles, fitting), 1 UI engineer, plus part-time DAM/packaging help.
- **Phases 0–2: about 13–15 pm**. That's about 4–6 months for 3 people, or about a year for a solo developer.
- **Phase 3: a further 12–18 pm.**
- Porting from darktable/RT/ART under GPL (demosaic, NR, lens, Upright) is the largest single schedule lever.

---

## 12. Open decisions for the project owner

1. Licence: GPL-3.0-or-later (recommended) vs permissive (§6).
2. Approach: new engine first with the UI decided at the Phase 2 gate (recommended), vs fork RapidRAW now (§5).
3. Fidelity target that defines "compatible" (proposal: median ΔE00 < 2, p95 < 5 over the corpus for the top-20 tags).
4. Who holds a licensed Lightroom Classic install to run the calibration sweeps, and where reference renders are stored (private; only CC0 inputs are committed).
5. Whether to reach out to RapidRAW's maintainer about a shared, Adobe-compatible engine crate.
6. Product name / trademark check.

---

## 13. Document index

| Doc | Contents |
|---|---|
| [research/01-lightroom-feature-inventory.md](research/01-lightroom-feature-inventory.md) | Whole-product feature inventory + MVP/v1/later matrix (secondary sources, confidence-tagged) |
| [research/02-preset-and-profile-formats.md](research/02-preset-and-profile-formats.md) | `.xmp`/`.lrtemplate` formats, full `crs:` tag table, DCP and XMP profiles, existing importers, legal, importer design |
| [research/03-darktable-survey.md](research/03-darktable-survey.md) | darktable architecture, all IOPs ↔ LR, key algorithms, masks, LR import (verified) |
| [research/04-oss-ecosystem.md](research/04-oss-ecosystem.md) | RawTherapee, ART, libraries, models, other apps, licence matrix (with corrections) |
| [research/05-processing-algorithms.md](research/05-processing-algorithms.md) | How ACR works per control (known vs inferred), open algorithms, fidelity harness |
| [research/06-tech-stack-options.md](research/06-tech-stack-options.md) | Language, GPU, UI, catalog, AI runtime, plugins, packaging, reference architecture, spikes |
| [research/07-rapidraw-survey.md](research/07-rapidraw-survey.md) | RapidRAW pipeline, preset converter mapping, gaps, fork/contribute analysis |
| [adobe-docs/](adobe-docs/) | Source-only digests of 236 official Adobe help pages (Classic, cloud, Camera Raw) |

### How this research was produced
- Parallel research agents (Sonnet for synthesis-heavy topics, Haiku for code sweeps) worked over local clones of darktable, RawTherapee and RapidRAW, plus the web.
- helpx.adobe.com blocks automated fetches, so 236 official help pages were mirrored through the Wayback Machine (Feb–Oct 2026 snapshots) and condensed by source-only digest agents. See [`adobe-docs/README.md`](adobe-docs/README.md).
- The lead reviewer spot-checked the agents' claims against source and corrected them: the darktable import table, licence compatibility, Adobe GPU requirements and RapidRAW's converter constants.
- Items tagged `[K]`, `[?]`, `[INF]` or `[background]` inside the research docs are unverified.
