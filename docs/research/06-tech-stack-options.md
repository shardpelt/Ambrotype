# 06 - Technology Stack Options

Status: research draft, October 2026. Scope: free/open-source, cross-platform (Windows/macOS/Linux) Lightroom alternative with non-destructive raw editing, catalog/DAM, GPU real-time preview, AI masking, and Lightroom XMP preset import.

**Evidence labels.** `[verified]` means confirmed in a web source during this research pass (URL given). `[background]` means from general engineering knowledge and should be re-checked before it drives a decision. Benchmarks for the exact stack proposed here do not exist; where I give numbers they are budgets/estimates, not measurements.

---

## 0. Executive recommendation (TL;DR)

| Layer | Recommendation | Runner-up |
|---|---|---|
| Language | **Rust** for engine, catalog, GPU, CLI. Thin C/C++ only where a library forces it (LibRaw, lcms2, lensfun, exiv2 via FFI/wrappers). | C++20 |
| GPU | **wgpu (WGSL compute)** as the single portable backend (Vulkan / Metal / DX12). fp32 working space, fp16 storage for intermediates. **CPU fallback** is the same node graph executed by a Rust SIMD path. | Hand-written Vulkan+Metal backends (too costly) |
| UI | **Slint or a custom wgpu-hosted canvas inside a native-ish shell; decision gate below.** Default pick: **Tauri 2 shell + native wgpu child surface for the image viewport** (web UI for panels). Qt 6 is the "boring, safe" alternative if the team has C++/QML skills. | Qt 6 (QML) |
| Catalog | SQLite (WAL, FTS5), own schema, XMP sidecars as interchange, tiled/pyramid preview cache on disk. Import `.lrcat` read-only. | - |
| AI | **ONNX Runtime via `ort`** with CoreML/DirectML/CUDA/CPU EPs; models downloaded on demand; only permissively licensed models in the default set. | candle/burn (not yet for segmentation at this quality) |
| Plugins | **WASM (wasmtime/Extism) as the long-term API; Lua (mlua) for user scripting/presets in v1**. | Python (embedded) - no |
| Packaging | Flatpak + AppImage, MSIX/MSI (SignPath Foundation signing), macOS notarized DMG ($99/yr Apple fee is unavoidable). | - |

The single most important architectural rule: **the engine is a headless, pure library with a CLI; the UI is a replaceable shell.** That makes the riskiest decision (UI framework) reversible.

---

## 1. Language

### 1.1 Real-world precedents

| Project | Language / stack | Notes | Source |
|---|---|---|---|
| RapidRAW | Rust + Tauri + wgpu; React/TS front end | GPU-accelerated non-destructive raw editor, pipeline "offloaded to the GPU via WGPU", Win/macOS/Linux, <30 MB. Self-described as not yet as polished as darktable/RawTherapee. `[verified]` Closest direct precedent to this project; the primary repo is CyberTimon/RapidRAW (many forks exist). | https://github.com/cybertimon/rapidraw |
| vkdt | C + Vulkan (GLSL), node-graph DAG | By darktable's founder; all processing in GLSL shaders; needs a Vulkan GPU, ~4 GB VRAM suggested. `[verified]` | https://github.com/hanatos/vkdt |
| darktable | C (+ some C++), GTK3, OpenCL | Pixel pipeline with OpenCL; roughly 45% of users reportedly have no GPU, which is why a GPU-only design was seen as unable to replace it. `[verified via search summary]` | https://discuss.pixls.us/t/a-port-of-darktable-to-the-gpu-a-dev-diary/48074 |
| Ansel | darktable fork, C/GTK | Different UX philosophy | https://github.com/aurelienpierreeng/ansel |
| RawTherapee | C++ / GTK | CPU-centric | `[background]` |
| digiKam, Krita | C++ / Qt | Mature DAM (digiKam) and image-editing (Krita) apps on Qt, shipping on all three OSes | `[background]` |
| lrcat-extractor | Rust | Reads Lightroom catalogs (LR 2/4/6) for import into another app | https://github.com/hfiguiere/lrcat-extractor |

### 1.2 Comparison

| Criterion | Rust | C++ | Zig | Swift | C# |
|---|---|---|---|---|---|
| Memory safety on untrusted input (raw files, XMP, catalogs, ONNX models are all attacker-controllable) | Excellent (safe by default; `unsafe` confined to FFI) | Manual; sanitizer/fuzz discipline needed | Manual-ish, better ergonomics | Good | Good (managed) |
| Image-processing ecosystem | `image`, `rawler` (dng/raw decoding, from the dnglab project), `rawloader`, `kamadak-exif`, `lcms2` crate, `wgpu`, `rayon`, `wide`/`std::simd` (std::simd still nightly `[background]`), `fast_image_resize`, `ort` | Largest: LibRaw, lcms2, lensfun, exiv2, OpenCV, Halide, OpenImageIO, OCIO, Qt | Tiny | Apple-centric; poor on Linux/Windows | .NET imaging OK, weak low-level GPU/SIMD ecosystem |
| FFI to C libs | `bindgen`/`cc`; wrappers exist for LibRaw (`libraw-rs`), lcms2 (`lcms2`), exiv2 (`rexiv2`, GObject-based) - quality varies; budget to own forks | Native | Native C interop is a Zig strength | C interop OK | P/Invoke OK |
| Contributor pool (OSS, 2026) | Smaller than C++, fast-growing, strongly overlaps with the "wants to contribute to a modern OSS app" audience; steep initial learning curve | Largest for imaging/graphics experts | Very small | Small outside Apple | Large, but few OSS imaging contributors |
| Build/packaging cross-platform | Cargo is excellent; cross-compiling C deps is the pain | CMake/vcpkg/Conan; dependency hell on Windows | Good toolchain, immature ecosystem | Poor on Windows/Linux | Easy, but runtime/AOT story is heavier |
| GPU API access | wgpu is first-class Rust | Vulkan/Metal/D3D native SDKs | Bindings only | Metal first-class | via Silk.NET/Veldrid |
| Maturity risk | Moderate (GUI + imaging crates young) | Low | High | Low on Apple, high elsewhere | Low |

### 1.3 Opinion

**Rust.** Reasons, in order of weight:

1. A raw editor's trust boundary is a pile of parsers (raw containers, TIFF/EXIF/MakerNotes, XMP, SQLite, ONNX). Rust removes the memory-corruption class where fuzzing historically finds most bugs in LibRaw/exiv2-like code.
2. wgpu is Rust-native; the best portable GPU abstraction available today has its first-class API in Rust, and RapidRAW demonstrates the combination works for this exact product category.
3. Cargo workspace + headless crates make the "testable engine + CLI" requirement cheap.
4. Contributor pool is the main cost. Mitigation: keep shader code (WGSL) and preset/look definitions accessible to non-Rust contributors, and keep FFI shims small.

**C++ is the credible alternative** if the team's expertise is there, mainly because Qt, LibRaw, lensfun, exiv2, Halide and OpenImageIO are all native C++. Choose it only if Qt is chosen as UI *and* the team is C++-first.

**Zig, Swift, C#**: not recommended. Zig's ecosystem is too thin for a multi-year imaging project; Swift ties you to Apple; C# gives good UI via Avalonia but weak GPU compute/SIMD story and few imaging contributors.

### 1.4 FFI plan for key C libraries

| Library | Purpose | Plan |
|---|---|---|
| LibRaw (LGPL-2.1 / CDDL) | Raw decode, broad camera support | Primary decoder via a `-sys` crate, dynamically or statically linked (LGPL: dynamic linking is the cleanest compliance path, or ship object files/relink instructions). Evaluate `rawler` (pure Rust, from dnglab, LGPL-2.1) as a gradual replacement/second opinion. `[background]` |
| lcms2 (MIT) | ICC transforms | `lcms2` crate; use for input/output/display profiles. GPU path uses baked 3D LUTs/matrix-TRC derived from lcms2, not lcms2 per pixel. |
| lensfun (LGPL-3 code, CC-BY-SA data) | Lens correction DB | Wrap the C API; **note the database is CC-BY-SA** - ship it as separate data. Verify before distribution. `[background]` |
| exiv2 (GPL-2+) | EXIF/IPTC/XMP | **GPL is a licensing trap** for a permissively-licensed app; if the project licence is GPL/AGPL it is fine, else prefer `kamadak-exif`/`xmp-toolkit`/own XMP code or `rexiv2` only if GPL is acceptable. Decide project licence first (see §8.5). `[background]` |

---

## 2. GPU compute

### 2.1 API options

| Option | Portability | Status 2025-26 | Verdict |
|---|---|---|---|
| **wgpu (WebGPU API, WGSL)** | Vulkan, Metal, DX12, (GL fallback) | Mature Rust implementation used by Bevy, Firefox, many apps; RapidRAW uses it `[verified]` | **Choose.** One shader language, one code path. |
| Raw Vulkan compute (GLSL/Slang) | Win/Linux native; macOS via MoltenVK | vkdt proves performance and flexibility `[verified]`; much more code per feature | Only if wgpu limits bite |
| Metal (MSL) | macOS only | Best on Apple Silicon | Not as primary; optionally for Apple-only fast paths |
| DX12 compute | Windows only | Fine | Reached via wgpu |
| OpenCL | Deprecated on macOS since 10.14 (2018); patchy drivers elsewhere | darktable still uses it, but it is a dead end `[background]` | Avoid |
| CUDA | NVIDIA only | - | No (user constraint, and portability) |
| OpenGL compute | GL 4.3+; macOS stuck at 4.1 (no compute) | Legacy | Avoid, except as wgpu's last-resort backend |

**wgpu caveats to plan for:** (a) WGSL has no native fp16 everywhere (the `shader-f16` feature is optional per adapter); (b) storage-texture format support and `read_write` storage textures vary by backend; (c) workgroup/buffer limits (`max_storage_buffer_binding_size`, typically 128 MiB default, raiseable to adapter limits) shape tiling; (d) no subgroup guarantees on all backends yet; (e) shader compile hitches on first run (DX12/Metal) - mitigate with pipeline cache and background warm-up. All `[background]`; validate with a spike (see §8.6).

### 2.2 Precision

- **Working space:** scene-linear, wide-gamut primaries (e.g., Rec.2020 or ACEScg-like / a Lightroom-like linear ProPhoto), **fp32 in compute math**.
- **Storage between nodes:** `rgba16float` textures where adapter supports it (halves bandwidth and memory); fall back to `rgba32float`. For 100 MP: 100e6 px x 8 B (rgba16f) = **800 MB per full-res intermediate**; rgba32f = **1.6 GB**. This is why full-res graphs must be tiled and why interactive editing must run on a downscaled proxy.
- Banding/clipping: fp16 has 11 bits of mantissa; fine for display-referred stages, risky for extreme highlight recovery and large exposure pushes in scene-linear data - keep the first stages (demosaic, WB, exposure, highlight reconstruction) in fp32 or rescale into fp16-safe range.
- Determinism: CPU and GPU outputs should agree within a tolerance; golden-image tests use per-channel epsilon, not bit equality.

### 2.3 Tiling and memory budgets

- Two render modes:
  1. **Interactive preview:** render at viewport resolution (e.g., a 4K viewport = ~8 MP, from a pyramid level of the demosaiced image). Fits on any GPU; target <16 ms per slider tick for the dirty suffix of the graph.
  2. **Export / 1:1 zoom:** tiled full-res render with halo (overlap) per node = sum of kernel radii downstream (blur, sharpen, denoise, local contrast need large halos). Tile sizes of 1024-2048 px square, halo computed by graph analysis (back-propagating "region of interest", as Halide/darktable do).
- Budget rule: peak VRAM = viewport intermediates + tile working set + model weights. Target a **2 GB envelope** on integrated GPUs; query `Adapter::limits()` and `wgpu` memory hints at runtime to pick tile size.
- Raw data upload: keep raw Bayer/X-Trans mosaic as `r16uint` texture(s) (100 MP = 200 MB), demosaic on GPU per tile.
- Out-of-memory: wgpu surfaces device loss/OOM poorly on some drivers; catch, halve tile size, retry, finally fall back to CPU.

### 2.4 Pipeline graph with caching at each stage

```
 raw decode -> [mosaic] -> black/WB -> demosaic -> input color (cam->working)
   -> exposure/tone -> HSL/color grading -> masks+local adj -> lens/geometry
   -> detail (denoise/sharpen) -> output transform (working -> display/export ICC)
```

Design:

- Each node: `{ id, op, params (immutable, hashed), inputs[], output format, ROI function, shader/CPU kernel }`.
- **Cache key per node** = hash(op version, params, input node keys, ROI/scale). On a parameter change only nodes downstream of the change recompute; upstream outputs stay resident in a **GPU texture LRU** (budgeted) with CPU spill to RAM, then disk for expensive nodes (demosaic, denoise).
- **Fusion:** adjacent point-wise nodes (exposure, contrast, curves, HSL, grading) compile into a single "mega-kernel" or generated WGSL, which avoids round trips (this is where the Lightroom-like "Basic panel" latency is won). Neighbourhood nodes (blur, sharpen, denoise, local laplacian) remain separate passes.
- **Masks** are first-class nodes producing single-channel textures (AI mask output, gradients, brushes, luminance/color range); local adjustments take `(image, mask, params)`.
- **Scheduling:** async render thread; superseded requests are cancelled (latest-wins) so dragging sliders never queues stale frames. A low-res "draft" pass renders first, then refinement.
- Graph is **serialisable** (JSON/CBOR) = the edit recipe stored in catalog/XMP - non-destructive by construction.
- vkdt's DAG with multiple inputs/outputs is the closest public reference design `[verified]` (https://github.com/hanatos/vkdt).

### 2.5 CPU fallback

Many users have no usable GPU (see darktable statistic above `[verified]`), and CI/headless servers have none.

| Option | Pros | Cons |
|---|---|---|
| **Same kernels in Rust on CPU** using `rayon` + `wide` (stable SIMD types) or `std::simd` (nightly, `[background]`) | Single language, easy testing, golden-image reference implementation | Duplicate of every kernel (WGSL + Rust); risk of drift |
| **wgpu with a software adapter** (llvmpipe/lavapipe/WARP via Vulkan/D3D) | Zero duplicate code | Slow, driver-dependent |
| Halide (C++) | Separates algorithm from schedule; targets CPU SIMD + GPU backends | C++ build dependency, awkward Rust integration, bigger toolchain |
| ISPC | Great SIMD codegen | Another compiler in the build |

**Recommendation:** write the **CPU implementation first as the reference** (also serves as test oracle and as the headless CLI renderer), and the WGSL kernel as the accelerated path; test equivalence within epsilon in CI (use lavapipe on Linux CI for GPU path tests). Revisit codegen from a shared DSL later if kernel drift becomes a maintenance burden.

---

## 3. UI framework

### 3.1 Requirements

Native-feel, very large zoomable canvas, colour-managed display (per-monitor ICC; macOS EDR/HDR; Windows HDR), accessibility, theming (dark UI is a must), hi-DPI, pen/tablet pressure (brush masks), keyboard-driven filmstrip/grid at 100k+ thumbnails.

### 3.2 Candidates

| Framework | Language | Strengths | Weaknesses for this app |
|---|---|---|---|
| **Qt 6 (QML / Widgets)** | C++ (Rust via `cxx-qt`) | Mature accessibility, tablet input, hi-DPI, ICC-aware (`QColorSpace`), native dialogs, RHI (Metal/Vulkan/D3D12) for custom GPU items; used by Krita/digiKam `[background]` | LGPL-3 obligations (dynamic linking, relinking, no static-link unless commercial licence); some Qt modules are GPL-only; Rust binding (`cxx-qt`) is workable but younger; heavy install size; Qt Company licensing politics |
| **Tauri 2 (webview + Rust)** | Rust + TS | Small binary (RapidRAW <30 MB `[verified]`), huge web-UI contributor pool, easy theming | **Image viewport is the problem:** Tauri IPC is message passing; shared memory/zero-copy is neither supported nor planned `[verified via discussions]` (https://github.com/tauri-apps/tauri/discussions/4177). Webview engines differ per OS (WebView2/WKWebView/WebKitGTK; the Linux one is the weakest: GPU compositing, WebGPU, colour management, and HDR are inconsistent `[background]`). Webview colour management follows the browser's rules, not an app-chosen display profile. |
| **egui / eframe** | Rust | Fast to build, wgpu-native, immediate mode suits tool panels | Accessibility via AccessKit exists but is partial; styling/native feel limited; large virtualised grids and rich text editing are awkward |
| **iced** | Rust | Elm architecture, wgpu-native, custom widgets/shaders | Accessibility reported lacking (e.g., no Narrator support) `[verified via search summary]` (https://rust-pc.github.io/rust-windows-gui.html); pre-1.0 churn |
| **Slint** | Rust/C++/JS DSL | Declarative, small, accessibility support reported, wgpu/skia backends, good for desktop since 1.x | Licence: GPLv3 / royalty-free (with attribution) / commercial - GPLv3 or the royalty-free licence works for OSS but constrains permissive relicensing; smaller widget set than Qt; tablet input maturity unknown `[background]` |
| **Dioxus** | Rust | React-like DX | Desktop = webview (same limitations as Tauri); native renderer (Blitz) not production-ready `[background]` |
| **Flutter** | Dart | Skia/Impeller, good animation; desktop OK | Dart language split from engine, FFI texture bridging is custom, weak colour management; Linux desktop quirks |
| **Avalonia** | C# | WPF-like, Skia-based, solid cross-platform | Forces C# in the shell; engine bridging via P/Invoke; HDR/ICC limited |
| **GTK4** | C / Rust (`gtk4-rs`) | Native on Linux, darktable's lineage, `gtk4-rs` is excellent | Poor macOS/Windows fidelity (fonts, menus, tablet, installers) |

Sources for the Rust GUI landscape: https://wrenlearnsrust.com/posts/2026-03-11-rust-gui-landscape-2026.html, https://blog.logrocket.com/state-rust-gui-libraries/, perf comparison (older, 2023): http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html.

### 3.3 Displaying GPU-rendered frames in a webview without copies

Options, best to worst:

1. **Native surface behind/over the webview (recommended if Tauri):** wgpu renders directly into a native child window/surface (`NSView`/`HWND`/Wayland/X11 subsurface) positioned under a **transparent webview** that draws only chrome and panels. No pixel crosses IPC. Tauri v2 supports multiple surfaces per window, and there are working experiments: https://github.com/orgs/tauri-apps/discussions/11944, https://github.com/dceddia/wgpu-tauri-experiment, https://github.com/clearlysid/tauri-wgpu-cam `[verified exist]`. Costs: z-order/transparency quirks (especially Linux/Wayland), input event routing (mouse events must go to the right layer), resize sync flicker. This also lets *you* choose the display ICC profile and use EDR/HDR swapchains via Metal/DX12 directly.
2. **WebGPU in the webview** with the engine running inside the webview (WASM): defeats the Rust-native engine design, WebGPU availability in WebKitGTK/WKWebView historically uneven, and no ICC control. Not recommended.
3. **Frame transport (shared bitmap, custom URI protocol, WebSocket, IPC base64):** at least one copy plus encode/decode; fine for thumbnails, too slow and laggy for a 4K+ interactive canvas. The WebGPU working group is discussing opaque shared-texture import (https://github.com/gpuweb/gpuweb/issues/6236) but nothing shippable yet `[verified exists]`.

### 3.4 Colour management specifics

- **ICC display profile:** the editor should render to the display's profile *itself* (own output transform in the pipeline's last node) and present an untagged / sRGB-passthrough swapchain, otherwise the OS or browser may double-convert. Needs per-monitor profile lookup: macOS ColorSync (`CGDisplayCopyColorSpace`), Windows WCS (`GetICMProfile` / `GetColorDirectory`), Linux colord / Wayland `color-management-v1` (protocol landed 2025, compositor support still rolling out `[background]`). In a webview, you cannot reliably control this - another argument for the native viewport.
- **HDR/EDR:** macOS EDR via `CAMetalLayer.wantsExtendedDynamicRangeContent` + `extendedLinearDisplayP3`; Windows HDR via DXGI `R16G16B16A16_FLOAT` scRGB swapchain or HDR10; wgpu's HDR/swapchain colour-space control is limited and may need a small platform-specific patch layer or a fork `[background]`. Treat HDR output as a phase-2 feature, but do not choose a UI stack that prevents it (a webview-only stack does).
- **Accessibility:** a native-viewport plus web panels design inherits the browser's accessibility tree for panels (a real advantage of Tauri); Qt has the most mature support across OSes. For an image canvas, accessibility = keyboard shortcuts, screen-reader labels for filmstrip/panels, high-contrast themes.
- **Tablet input:** Qt exposes pressure/tilt uniformly; in Tauri/webview use Pointer Events (pressure available on WebView2/WKWebView; WebKitGTK variable) or read native events in the native viewport layer; winit has stylus support in progress `[background]`.

### 3.5 Recommendation

Two defensible paths; pick via a 3-week spike (§8.6):

- **Path A (default): Tauri 2 shell + native wgpu viewport** with web UI for panels/DAM. Best for contributor reach and iteration speed; matches RapidRAW's proven direction; engine stays in Rust. Risk: window-layering bugs across three OSes, WebKitGTK quality, and HDR/ICC all depend on the native layer you must write.
- **Path B (safe): Qt 6 + `cxx-qt`, with the viewport as a `QQuickRhiItem`/`QRhi` render node fed by wgpu textures** (or by a Vulkan/Metal interop path). Best accessibility, tablet, colour, and DAM-grid performance out of the box; costs: C++/QML in the project, LGPL compliance, smaller Rust contributor appeal.

My pick for a *photographer-grade* product is **A if the spike shows the viewport layering is stable on all three OSes; otherwise B**. Do not pick egui/iced for the shipping shell (accessibility and polish), though egui is excellent for the internal dev/debug tools and the engine's test harness.

---

## 4. Catalog storage

### 4.1 SQLite design

- **SQLite in WAL mode**, single writer service (a Rust "catalog service" owns the connection; UI/workers send commands), many readers. `PRAGMA foreign_keys=ON`, `synchronous=NORMAL` with WAL, periodic `wal_checkpoint`, `user_version` for migrations (refinery/sqlx migrations).
- **Scale target 500k-1M photos:** key facts (`[background]`): SQLite handles tens of millions of rows; the pain points are (a) unindexed filter/sort combinations on the grid, (b) per-photo EAV metadata tables, (c) large blobs in the main DB, and (d) write amplification from frequent edits. Mitigations: narrow `images` table with the hot columns (id, folder_id, filename, capture_time, rating, flag, colour label, width, height, camera_id, lens_id, has_edits); denormalised lookup tables for camera/lens; composite indexes for the common sorts (capture_time, rating); keyset pagination (not OFFSET) and a virtualised grid; keep thumbnails **out of the DB**.
- **Suggested core schema (sketch):**
  - `volumes`, `folders(parent_id, volume_id, path_rel)` - mirrors the Lightroom root/subfolder model, making relinking moved drives easy.
  - `images(id, uuid, folder_id, file_name, ext, file_hash, mtime, size, capture_time, width, height, orientation, rating, flag, color_label, camera_id, lens_id, iso, aperture, shutter, focal_length, gps_lat, gps_lon, ...)`
  - `virtual_copies` / `versions(image_id, version_id, ...)`
  - `edits(image_id, version_id, schema_version, graph_json BLOB, rev, updated_at)` + `edit_history(...)` (snapshots/history states)
  - `keywords(id, name, parent_id, synonyms, include_on_export)`, `image_keywords(image_id, keyword_id)` - closure table or materialised path for hierarchy
  - `collections(id, type, parent_id, smart_query_json)`, `collection_images`
  - `previews(image_id, level, path, mtime_hash)` index only
  - `metadata_text` (caption, title, creator) + **FTS5** virtual table
  - `sync_state(image_id, xmp_mtime, xmp_hash, db_rev)` for sidecar reconciliation
- **FTS5** for keywords/captions/filenames/IPTC (`tokenize='unicode61 remove_diacritics 2'` or trigram for substring); external-content FTS tables with triggers to avoid duplicated storage. `[background]`
- **Concurrency/safety:** auto-backup before migrations, integrity check at startup on demand, catalog-as-single-file for portability; consider a `catalog.db` + `previews/` directory bundle.

### 4.2 Preview/thumbnail cache

- Disk layout: sharded directory tree `previews/aa/bb/<hash>.<level>` or packed container files (SQLite blob pages are fine up to a few KB; for JPEG/WebP/AVIF thumbnails of 100-500 KB use files).
- **Pyramid levels:** 256 px (grid), 1024 px (loupe/fast cull), 2048-4096 px (standard preview), plus a **tiled full-res demosaiced cache** (e.g., 512x512 tiles in a compressed/float16 format) for instant 1:1 zoom.
- Invalidate by `(file_hash or mtime+size, edit_rev, pipeline_version)`. Smart Previews analogue: lossy DNG-like (downsized linear) so offline editing works.
- Budget: 500k photos x ~30 KB (grid thumb) = ~15 GB at 256 px; 1024-px previews would be ~100+ GB, so make levels optional, LRU-evictable, and generated lazily by a background priority queue.
- Use embedded raw JPEG previews for instant first display during import (as most DAMs do).

### 4.3 XMP sidecar sync

- Read/write `.xmp` next to originals (Adobe `crs:` namespace for develop settings + `xmp:Rating`, `dc:subject`, `lr:hierarchicalSubject`, IPTC). Embed into DNG/JPEG/TIFF as optional.
- Conflict policy: DB is authoritative at runtime; sidecar written (debounced) on change; on file-change detection compare `xmp_mtime/hash` vs `sync_state`; three-way merge for conflicts (last-writer-wins on scalar fields, set-union on keywords), with a user prompt for develop settings.
- Preserve **unknown XMP namespaces** verbatim (round-trip) - essential for interoperability with Lightroom/Capture One/darktable. darktable's own XMP is a cautionary tale of complexity `[background]`; start with read-all/write-minimal.
- Filesystem watching via `notify` crate; handle network drives with polling.

### 4.4 Lightroom Classic catalog (.lrcat) migration

What is known from the community (no official Adobe documentation; the format is unofficial, changes across versions, and must be treated as best-effort and **read-only**; always open a *copy*):

- `.lrcat` is a SQLite database. `[verified]` https://github.com/hfiguiere/lrcat-extractor/blob/main/doc/lrcat_format.md
- Common patterns: each table has `id_local` (integer PK) and `id_global` (UUID) columns. `Adobe_variablesTable` holds catalog version and the root keyword id. `[verified]`
- **Images:** `Adobe_images` (logical images: rating, pick/flag, file format RAW/JPG/VIDEO/DNG, orientation, capture time, colour label, `rootFile` -> `AgLibraryFile`, `masterImage`/`copyName` for virtual copies); `Adobe_imageProperties` (additional metadata, serialised in "lron"/Lua-table-like format). `[verified]`
- **Files/folders:** `AgLibraryFile` (physical file: base name, extension, `sidecarExtensions`, `folder` -> `AgLibraryFolder`, content hash); `AgLibraryFolder` (path *relative* to its root folder) and `AgLibraryRootFolder` (absolute root path). Path reconstruction = root.absolutePath + folder.pathFromRoot + file.baseName + "." + extension. `[verified]`
- **Develop settings:** `Adobe_imageDevelopSettings` (`image` FK, `text` column containing a Lua-serialised table such as `s = { ProcessVersion = "11.0", Exposure2012 = 0.5, ToneCurvePV2012 = {...}, ... }`; also `settingsID`, `grayscale`, `hasDevelopAdjustments`...). Parse with a small Lua-table parser (or embedded Lua sandbox). Setting keys match the `crs:` XMP attributes (Exposure2012, Contrast2012, Highlights2012, Temperature, Tint, HueAdjustmentRed, ParametricShadows, ...), so **one mapping layer serves both .lrcat import and XMP/preset import**. `[background]` for the exact column set; lrcat-extractor and https://github.com/camerahacks/lightroom-database (unofficial table catalog) document it in more detail.
- **Keywords:** `AgLibraryKeyword` (name, parent, genealogy string, include-on-export, synonyms in `AgLibraryKeywordSynonym`) and `AgLibraryKeywordImage` (image <-> keyword). LR6 adds face-related keyword types, plus `AgLibraryFace*` tables. `[verified]`
- **Collections:** `AgLibraryCollection` (`creationId` distinguishes regular/smart collections/system objects; smart collection rules stored as Lua-ish text in `AgLibraryCollection` -> `content`/`AgLibraryCollectionContent`), `AgLibraryCollectionImage`; collection sets via parent. LR2 used `AgLibraryTag` with `kindName`. `[verified]` (partial)
- **History/snapshots/presets:** `Adobe_libraryImageDevelopHistoryStep` (history), `Adobe_libraryImageDevelopSnapshot` (snapshots). Presets themselves live outside the catalog as `.lrtemplate` (Lua) and `.xmp` files. `[background]`
- Version coverage: lrcat-extractor supports LR 2, 4, 6 only `[verified]`; modern Classic (CC 7-14) catalogs have extra tables (masking JSON in develop settings: `MaskGroupBasedCorrections`, point colour, AI masks, etc.) and need fresh reverse-engineering with sample catalogs from each version; recruit community catalog donations (anonymised).
- Other readers to study: https://github.com/thatlarrypearson/LightroomClassicCatalogReader, https://www.seachess.net/notes/dive-into-lightroom-catalogues/, https://github.com/ibips/lrprev-extract (previews extraction).
- **Strategy:** (1) import metadata, folders, keywords, collections, ratings/flags/labels, virtual copies, develop settings; (2) map develop settings through the shared `crs` mapping with a **fidelity report** (what mapped exactly, approximately, or not at all); (3) never write to the `.lrcat`; (4) licence check: lrcat-extractor is LGPL-2.1+ (verify) so reading it is fine but copying code has implications; alternatively write an own reader from the documented schema. Also note Adobe's EULA concerns: interoperability by reading user's own data is widely done, but have counsel review before shipping "import Lightroom" marketing. `[background]`

### 4.5 Lightroom XMP/preset import (cross-link)

Out of scope here (see other docs), but it constrains the stack: Adobe's tone/colour models (Process Version 2012/2023 sliders, ACR camera profiles, HSL, tone curve parametric/point, split toning/colour grading, calibration, dehaze, texture/clarity) must be reproduced **approximately** in the engine's pipeline; keep the node parameter schema close to `crs:` names to minimise translation loss, and keep a per-key `fidelity` annotation.

---

## 5. AI inference runtime

### 5.1 Runtime options

| Runtime | Pros | Cons |
|---|---|---|
| **ONNX Runtime + `ort` crate** | Mature, huge model compatibility (SAM2, BiRefNet, Depth Anything all have ONNX exports); EPs: CUDA/TensorRT (NVIDIA), **DirectML (Windows any DX12 GPU)**, **CoreML (Apple)**, OpenVINO, XNNPACK, CPU. `ort` exposes `directml`, `coreml`, `xnnpack` as Cargo features usable with downloaded prebuilt binaries; other EPs need building ORT from source. `[verified]` https://ort.pyke.io/perf/execution-providers, https://docs.rs/ort | Big native dependency (~10-50 MB+); EP behaviours differ (fp16, dynamic shapes, operator coverage); DirectML is in sustained-engineering mode with Microsoft pointing toward Windows ML/other EPs `[background - verify]`; Linux has no vendor-neutral GPU EP (CUDA/ROCm/OpenVINO only; WebGPU EP is emerging) |
| candle (HF) | Pure Rust, small; CUDA/Metal | Smaller op/model coverage; segmentation models need porting |
| burn | Pure Rust, wgpu backend, ONNX import tool | Maturity of ONNX import for large vision transformers uncertain |
| tract | Pure Rust ONNX inference, CPU-only effectively | Too slow for SAM-class encoders |
| wgpu-based inference (own kernels, e.g., via burn-wgpu / WONNX) | Same GPU stack as the renderer; zero-copy sharing of textures between render and inference | Immature op coverage; SAM2/BiRefNet attention ops costly to hand-implement |

**Recommendation:** `ort` as the default runtime behind an `InferenceBackend` trait, so burn/candle can be swapped in later. Ship **prebuilt ORT per OS** with CoreML on macOS, DirectML on Windows (plus optional CUDA download pack), CPU/XNNPACK on Linux with optional CUDA/ROCm/OpenVINO packs. Tensors from the render graph are exchanged via CPU memory initially (inference runs on a downscaled ~1024 px copy, so copy costs are small); zero-copy GPU sharing is a later optimisation.

### 5.2 Model management

- Models are **not bundled in the installer**: download on first use from a mirrored location (GitHub Releases / Hugging Face) with SHA-256 pinning, resumable downloads, size shown to the user, offline import of model files, and a "models" directory per user. Provide quantised (int8/fp16) variants for CPU.
- Cache computed masks in the catalog (mask is a small PNG/RLE blob per image per mask) so AI results aren't recomputed; store model id+version in the mask record for reproducibility.
- Typical sizes `[background]`: SAM2 hiera-tiny/small/base+ encoders ~40-320 MB (fp32), BiRefNet ~220 MB (fp32; lite variants smaller), Depth Anything V2 Small ~100 MB (25M params), U2Net ~170 MB (u2netp ~4 MB).

### 5.3 Model licensing matrix (critical for a *free* app)

"Free to users" is **not** the same as "non-commercial": a CC-BY-NC model can be a problem if the project is distributed by a company, offers donations/sponsorship/paid builds, or if downstream forks go commercial. Policy recommendation: **the default download set contains only OSI-permissive-licensed weights**; NC models are "bring your own model" at most, off by default, and clearly labelled.

| Model | Task | Licence | Verdict | Source |
|---|---|---|---|---|
| SAM 2 (Meta) | Interactive subject/object segmentation | **Apache-2.0** (checkpoints, demo, training code) | Use. Encoder heavy; use tiny/small + ONNX | https://github.com/facebookresearch/sam2/blob/main/LICENSE `[verified]` |
| MobileSAM / EdgeSAM / SAM 2 distillations | Lightweight SAM | Mostly Apache-2.0 (check each) | Candidate for low-end | `[background]` |
| BiRefNet | High-res salient object / subject matting | **MIT** (code); verify weights card per variant | Use as "select subject" refinement | https://github.com/ZhengPeng7/BiRefNet/blob/main/LICENSE `[verified]` |
| U2-Net / u2netp | Salient object | Apache-2.0 | Use as small CPU-friendly default | `[background]` |
| **BRIA RMBG-1.4 / RMBG-2.0** | Background removal | **Creative Commons non-commercial**; commercial use requires an agreement with BRIA | **Do NOT bundle or auto-download.** Confirmed non-commercial `[verified]` | https://huggingface.co/briaai/RMBG-1.4, https://huggingface.co/briaai/RMBG-2.0 |
| Depth Anything V2 **Small** | Monocular depth (depth-range masks, lens blur) | **Apache-2.0** | Use | `[verified]` https://github.com/DepthAnything/Depth-Anything-V2/issues/162 (see also licence-risk discussion on training-data provenance in issue #320) |
| Depth Anything V2 Base/Large/Giant | Depth | **CC-BY-NC-4.0** | Do not ship | `[verified]` same |
| Sky segmentation | Sky masks | Use SAM2 prompt + simple sky classifier; or train an own small model on permissively-licensed data (ADE20K licence is research-only - check) | Plan to **train/distil our own** small sky and person models under Apache-2.0 | `[background]` |
| Person/portrait segmentation | Person masks | MediaPipe selfie segmentation (Apache-2.0) as a start; also SAM2 with auto prompts | Use / evaluate | `[background]` |
| Denoise | Raw/RGB denoise | NAFNet (MIT), Restormer (custom academic - check), SCUNet (Apache-2.0), DRUNet (MIT); classical: NLM/BM3D/wavelets | Start classical + NAFNet/SCUNet; train on a raw-aware dataset with permissive licence | `[background]` |

Licensing hygiene: record licence + source + hash in a `models.toml` manifest; CI check that the default manifest contains no NC/research-only entries; keep dataset-provenance risk in mind (the "licence-risk" issue #320 on Depth Anything V2 Small shows even Apache weights can have upstream-data ambiguity).

---

## 6. Plugin / scripting

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Lua** (mlua, Lua 5.4/LuaJIT) | Precedent: Lightroom SDK and darktable both use Lua `[background]`; tiny, embeddable, sandboxable; photographers/hobbyists have prior Lua experience; Lightroom preset formats (`.lrtemplate`, lrcat develop settings) *are* Lua tables, so the parser is nearly free | Weak types, no package ecosystem, limited perf | **Yes for v1** user scripting: import/export hooks, batch actions, preset logic, metadata rules |
| **WASM** (wasmtime / Extism, WASI components) | Strong sandbox, language-agnostic (Rust/C/AssemblyScript/Go), versioned ABI, safe to run third-party plugins | Young component-model tooling; GPU/shader access must be mediated by host; authoring story harder | **Yes as the long-term "real plugin" API** (e.g., custom filters operating on tiles, exporters, AI-model adapters) |
| Python (PyO3 embed) | Huge ML/imaging ecosystem, popular with photographers/data folks | Distribution nightmare (interpreter + wheels per OS), GIL, sandboxing hard | No embedded Python; offer a **CLI/JSON-RPC or gRPC local API** so external Python scripts can drive the app/catalog |

Also expose the headless CLI (`flightbroom render`, `flightbroom catalog ...`) as scripting surface, and a stable **recipe (graph) JSON schema** as the primary extensibility contract.

---

## 7. Packaging & distribution

| Target | Recommendation | Notes / cost |
|---|---|---|
| Linux | **Flatpak (Flathub)** primary; **AppImage** secondary; distro packages by community | Flatpak sandbox: needs `--filesystem` access for photo libraries (portals are awkward for DAM; request `--filesystem=home`/xdg-pictures + removable media); GPU via `--device=dri`. `[background]` Flathub licence/ME review is free. AppImage must bundle ORT/WebKit deps (Tauri on AppImage has known WebKitGTK bundling pain `[background]`). |
| Windows | **MSIX and/or MSI (WiX)**, plus portable zip | Code signing: **SignPath Foundation** offers free OV signing for qualifying OSS (publisher shows "SignPath Foundation") `[verified]` https://signpath.org/; Azure Artifact Signing (formerly Trusted Signing) is restricted to orgs in US/Canada/EU/UK and individuals in US/Canada as of Feb 2026 `[verified via search summary]` https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options. Unsigned builds trigger SmartScreen warnings. |
| macOS | Notarized, hardened-runtime **DMG** (universal2 or separate arm64/x86_64) | Notarization requires Apple Developer Program: **US$99/year** with no extra per-notarization fee `[verified]` https://developer.apple.com/support/enrollment/ ; there is no free path for OSS (see discussion https://buckleyisms.com/blog/apple-should-provide-notarization-for-open-source-apps/). Budget this via GitHub Sponsors/OpenCollective/fiscal host (e.g., a non-profit umbrella can hold the Apple Developer account, which also helps with Windows SignPath eligibility). Mac App Store is optional and brings sandbox constraints. Homebrew cask as easy secondary. |
| Auto-update | **Tauri updater** (signed manifests) if Tauri; **Sparkle** (macOS) / **WinSparkle** or **velopack/Squirrel** if native/Qt; Flatpak/Flathub handle their own. Update signature keys held in CI secrets; never auto-update catalogs/schemas without migration backups. | |
| CI | **GitHub Actions matrix:** `ubuntu-24.04`, `ubuntu-24.04-arm`, `windows-latest`, `windows-11-arm`, `macos-14` (arm64) + `macos-13`/cross-compile for x86_64; `cargo nextest`, `cargo clippy`, `cargo deny` (licence gate), `cargo fuzz` nightly on raw/XMP/lrcat parsers; GPU tests via lavapipe (Linux) and WARP (Windows); golden-image regression suite; release workflow builds, signs, notarizes (`notarytool`), uploads. Cache with `sccache`/`Swatinem/rust-cache`. Self-hosted GPU runner (community-donated) for real-GPU tests. | |
| Reproducibility | Pin toolchain, vendor C deps via `vcpkg`/`cargo vendor`; SBOM (`cargo-cyclonedx`) | |

---

## 8. Reference architecture

### 8.1 Diagram

```
+--------------------------------------------------------------------------------+
|                               UI SHELL (replaceable)                           |
|  Tauri 2 web panels (TS)  |  or Qt 6 / QML  |  native wgpu VIEWPORT surface    |
|  (develop panels, grid,   |                 |  (ICC display xform, HDR/EDR,    |
|   filmstrip, DAM sidebar) |                 |   pen input, zoom/pan)           |
+------------------+-----------------------------+---------------+---------------+
                   | commands/events (typed, serde/JSON; no pixel data)
+------------------v-------------------------------------------------------------+
|                          APP SERVICE LAYER (Rust)                              |
|  job queue / scheduler (tokio + rayon) | settings | undo/redo | plugin host      |
|  (Lua / WASM) | local CLI/RPC API                                                |
+-----------+------------------------+-----------------------+-------------------+
            |                        |                       |
+-----------v----------+  +----------v-----------+  +--------v------------------+
|   CATALOG SERVICE    |  |   ENGINE (flb-core)  |  |   AI SERVICE (flb-ai)     |
| SQLite WAL + FTS5    |  | pure, headless, no UI|  | InferenceBackend trait    |
| schema/migrations    |  |                      |  | ort: CoreML/DirectML/CUDA |
| XMP sync (notify)    |  |  +----------------+  |  | model manager (download,  |
| lrcat importer       |  |  | Recipe (graph  |  |  | hash, licence manifest)   |
| preview cache index  |  |  | JSON, versioned|  |  | mask cache                |
| (flb-catalog,        |  |  | crs mapping)   |  |  +--------------------------+
|  flb-lrimport)       |  |  +-------+--------+  |
+-----------+----------+  |          v           |
            |             |  +----------------+  |        +---------------------+
            |             |  | Pipeline graph |  |        |  I/O + COLOUR       |
            |             |  | nodes, ROI,    |<-+--------+  LibRaw/rawler,     |
            |             |  | cache keys,    |  |        |  lcms2, lensfun,    |
            |             |  | fusion planner |  |        |  XMP/EXIF (flb-io,  |
            |             |  +---+--------+---+  |        |  flb-color)         |
            |             |      |        |      |        +---------------------+
            |             |  +---v---+ +--v-----+|
            |             |  | GPU   | | CPU    ||   <- same node semantics,
            |             |  |backend| |backend ||      equivalence-tested in CI
            |             |  | wgpu/ | | rayon+ ||
            |             |  | WGSL  | | SIMD   ||
            |             |  +-------+ +--------+|
            |             +----------------------+
            |
   +--------v------------------------+      +----------------------------------+
   |  flb-cli (headless)              |      |  Test harness: golden images,    |
   |  render, batch export, preset    |      |  preset fidelity suite, fuzzers  |
   |  tests, catalog tools            |      +----------------------------------+
   +----------------------------------+
```

Crate layout (Cargo workspace): `flb-core` (recipe, graph, CPU kernels), `flb-gpu` (wgpu backend + WGSL), `flb-io` (raw/EXIF/XMP), `flb-color`, `flb-catalog`, `flb-lrimport` (lrcat + XMP/preset import), `flb-ai`, `flb-plugin`, `flb-cli`, `flb-app` (service layer), `ui/` (shell).

### 8.2 Why this shape

- **Pure engine + CLI** makes the render deterministic and testable (golden images, preset-import regression, fuzzing), enables headless batch export/servers, and avoids the UI framework infecting the core. Every competitor that coupled pipeline and GUI (older darktable/RawTherapee) pays for it.
- **Recipe as data** (versioned JSON graph) gives non-destructive editing, history, copy/paste settings, presets, XMP serialisation and LR import through one mechanism.
- **Two backends, one graph** gives GPU speed with a guaranteed CPU path (the ~45% no-GPU reality `[verified]`) and a reference oracle.
- **Catalog as a service with a single writer** avoids SQLite lock contention and makes sync logic centralised.
- **AI isolated behind a trait and manifest** contains licensing risk and the heavy ORT dependency; the app runs fully without it.
- **Shell separated by a typed command protocol with no pixel traffic** is what makes Tauri-vs-Qt a reversible choice.

### 8.3 Alternatives considered

1. **All-C++/Qt (digiKam/Krita style):** lowest UI risk, best a11y/tablet; higher memory-safety risk, thinner Rust-style contributor pull, LGPL friction.
2. **RapidRAW-style Tauri + wgpu monolith:** quickest to a demo; pipeline and UI tightly bound; viewport via image transport hits the IPC ceiling for big canvases.
3. **vkdt-style Vulkan-only:** fastest and most flexible; no macOS-native story (MoltenVK) and no CPU fallback; contributor friction.
4. **Fork/extend darktable or RawTherapee:** reuses decades of algorithms and camera support; inherits GTK/C architecture, licence (GPL-3), and UX debt. Worth considering as an *algorithm/lens/camera data donor* rather than a base.
5. **Web-only (WASM + WebGPU in a browser/Electron):** easiest distribution to contributors; weakest colour management, memory limits for 100 MP, and file-system access.

### 8.4 Risks and mitigations

| Risk | Likelihood / impact | Mitigation |
|---|---|---|
| wgpu limits (fp16 availability, storage texture formats, buffer sizes, HDR swapchain control) | Med / High | Early spike, feature-detect with fp32 fallbacks, contribute upstream, abstract swapchain behind own trait |
| Webview viewport layering bugs (esp. Linux/Wayland) | High / High (if Tauri) | Spike on all three OSes first; keep Qt path viable; keep UI protocol pixel-free |
| Colour-managed display correctness (ICC, HDR) | Med / High | Own output transform; per-OS display profile lookup; visual test targets (calibration charts) |
| LibRaw/lensfun/exiv2 licences (LGPL/GPL/CC-BY-SA) | Med / Med | Decide project licence early; dynamic linking; `cargo deny` gate; separate data packages |
| Camera/lens support breadth | High / Med | Use LibRaw + rawler + lensfun data; community sample-submission pipeline |
| Adobe look fidelity (preset import) | High / High (product expectation) | Approximate mapping with fidelity report; calibrate against Adobe renders on a public test set |
| .lrcat format drift | Med / Med | Version-detect, read-only, community catalog donations, graceful partial import |
| AI model licensing / size / hardware variance | Med / High | Permissive-only default manifest, CI licence gate, own distilled models, CPU fallback with quantised models |
| Contributor pool for Rust + GPU | Med / Med | Clear module boundaries, WGSL kernels documented, "good first kernels", scripting layer for non-Rust contributors |
| Signing/notarization costs | Certain / Low | Fiscal host (OpenCollective/SFC), SignPath Foundation, $99 Apple fee budgeted |
| Scope (building a full DAM and developer simultaneously) | High / High | Ship develop-module-first with minimal catalog; strict milestone gates |

### 8.5 Licence decision (needed before the stack is final)

Choose the project licence early: **GPL-3.0-or-later** unlocks exiv2, darktable/RawTherapee algorithm reuse (with attribution), and Slint-GPL, but forecloses proprietary forks; **MPL-2.0/Apache-2.0** maximises adoption but excludes exiv2 and GPL code reuse. For a Lightroom-alternative aiming for community trust and algorithm reuse, **GPL-3.0 or AGPL-free GPL-3.0** is the pragmatic choice; this is a decision for the team, flagged here because it changes the dependency list.

### 8.6 Proposed de-risking spikes (2-4 weeks each, in this order)

1. **Viewport spike:** wgpu 100 MP tiled demosaic+tone pipeline at 60 fps preview in (a) Tauri native-surface-behind-webview and (b) Qt RHI, on Windows 11, macOS (Apple Silicon), Ubuntu Wayland+X11. Measure frame latency, memory, resize flicker, ICC/EDR handling.
2. **GPU precision/limits spike:** fp16 vs fp32 intermediates on Intel iGPU, AMD, NVIDIA, Apple; quantify banding and speed.
3. **CPU/GPU equivalence + golden harness:** 10 nodes in both backends, CI with lavapipe.
4. **lrcat + XMP import spike:** parse 5 catalogs spanning LR 6 -> current; produce fidelity report; compare Adobe renders.
5. **AI spike:** `ort` + SAM2-tiny + BiRefNet + Depth Anything V2 Small on CoreML / DirectML / CPU; latency and memory.

---

## 9. Source list

- RapidRAW (Rust + Tauri + wgpu): https://github.com/cybertimon/rapidraw
- vkdt: https://github.com/hanatos/vkdt ; darktable GPU port discussion: https://discuss.pixls.us/t/a-port-of-darktable-to-the-gpu-a-dev-diary/48074
- Ansel: https://github.com/aurelienpierreeng/ansel
- lrcat-extractor and format doc: https://github.com/hfiguiere/lrcat-extractor , https://github.com/hfiguiere/lrcat-extractor/blob/main/doc/lrcat_format.md
- Lightroom DB catalog (unofficial): https://github.com/camerahacks/lightroom-database ; reader: https://github.com/thatlarrypearson/LightroomClassicCatalogReader ; notes: https://www.seachess.net/notes/dive-into-lightroom-catalogues/ ; previews: https://github.com/ibips/lrprev-extract
- Tauri / wgpu viewport: https://github.com/orgs/tauri-apps/discussions/11944 , https://github.com/tauri-apps/tauri/discussions/4177 , https://github.com/tauri-apps/tauri/issues/13740 , https://github.com/dceddia/wgpu-tauri-experiment , https://github.com/clearlysid/tauri-wgpu-cam , https://github.com/tauri-apps/wry/blob/dev/examples/wgpu.rs , https://github.com/gpuweb/gpuweb/issues/6236
- Rust GUI landscape: https://wrenlearnsrust.com/posts/2026-03-11-rust-gui-landscape-2026.html , https://blog.logrocket.com/state-rust-gui-libraries/ , https://rust-pc.github.io/rust-windows-gui.html , http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html
- ONNX Runtime / ort: https://ort.pyke.io/perf/execution-providers , https://docs.rs/ort , https://onnxruntime.ai/docs/execution-providers/
- Model licences: SAM 2 https://github.com/facebookresearch/sam2/blob/main/LICENSE ; BiRefNet https://github.com/ZhengPeng7/BiRefNet/blob/main/LICENSE ; RMBG https://huggingface.co/briaai/RMBG-1.4 , https://huggingface.co/briaai/RMBG-2.0 ; Depth Anything V2 https://github.com/DepthAnything/Depth-Anything-V2/issues/162 , https://github.com/DepthAnything/Depth-Anything-V2/issues/320
- Signing/notarization: https://developer.apple.com/support/enrollment/ , https://buckleyisms.com/blog/apple-should-provide-notarization-for-open-source-apps/ , https://signpath.org/ , https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options

### Caveats on this document

- Items tagged `[background]` were not re-verified in this pass (notably: macOS OpenCL deprecation details, wgpu feature/limit specifics, Wayland colour-management protocol status, lensfun/exiv2/LibRaw/rawler licence details, SAM2/BiRefNet/Depth model file sizes, Slint licence terms, DirectML maintenance status, and exact .lrcat column names for modern Lightroom versions). A short verification pass is recommended before architectural sign-off.
- The five "verified" lrcat facts come from lrcat-extractor's doc for LR 2/4/6; modern catalog layout is unverified.
