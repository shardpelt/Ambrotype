# 08 - Cross-Platform Parity and a Future Mobile Version

Status: research draft, 2026-10-03. Scope: (a) what "first-class on Windows/macOS/Linux" requires; (b) what a future iOS + Android app needs, so today's choices (Phases 0-2) do not close the door; (c) licensing and app-store consequences of the earlier GPL-3.0-or-later recommendation ([SCOPE §6](../SCOPE.md), [06 §8.5](06-tech-stack-options.md)). Companion to [06](06-tech-stack-options.md); read its §0, §3, §8 first.

**Evidence labels.** `[verified: URL]` = seen in a source during this pass (several are search-result summaries, noted as "secondary"). `[code]` = read in the RapidRAW clone (`scratchpad/RapidRAW`, AGPL, commit as cloned 2026-10). `[background]` = engineering knowledge, re-check before relying on it. `[unverified]` = could not confirm, or an estimate/proposal of ours. **Nothing here is legal advice; section 5 needs counsel before the first iOS submission.**

---

## 0. Executive summary

1. **Mobile is feasible for the engine** (Rust, wgpu/Metal/Vulkan, ONNX Runtime, SQLite all cross-compile), **hard for the viewport/UI/file layers**, and **gated by licensing on iOS**.
2. **The licensing decision is the only one that is expensive to reverse.** Once outside contributors land GPL-3.0-or-later code, every contributor must consent to relicense (VLC needed ~230 contributors, about a year, and still left ~150k LOC GPL; §5.2). **Recommendation: split licence. Engine core crates under MPL-2.0 (own code + permissive deps only); desktop-only "quality packs" holding ported darktable/RawTherapee/ART code, exiv2 and lensfun under GPL-3.0-or-later; UI shells MPL-2.0 or GPL; DCO instead of a CLA.** MPL-2.0 is the licence VLC used to return to the iOS App Store and Firefox iOS ships under. The desktop app stays GPL-3.0-or-later as a combined work, so the community-trust story is intact. Decide before the first external PR; the crate boundary costs almost nothing now. (§5.7)
3. **UI**: keep Tauri 2 (+ native wgpu viewport) for desktop. For mobile, plan **native SwiftUI + Jetpack Compose shells over a UniFFI binding of the same engine, with the wgpu surface hosted in a native view** (what Element X, Firefox mobile and 1Password do for their cores). A RapidRAW-style Tauri-mobile build is a viable cheap prototype, not the destination. Qt is out for iOS (LGPLv3 installation-information clause). (§3.7)
4. **Make the engine mobile-shaped now** (checklist §8): no `PathBuf`/mmap assumptions in the engine API, tile/limit budgeting against a *mobile floor profile*, two viewport transports (native surface and frame stream), touch-semantic input protocol, a `ResourceBudget` fed by the shell, feature flags, and `aarch64-apple-ios` / `aarch64-linux-android` `cargo check` in CI from Phase 0.
5. **Sync without our cloud**: sidecar XMP + a per-device append-only op-log in a user-chosen folder, plus a ~2560 px "smart-preview" proxy format; never sync the SQLite file. (§6)

---

## 1. Desktop parity requirements per OS

### 1.1 Requirement matrix

| Concern | Windows 10/11 (x64, ARM64) | macOS 13+ (Apple Silicon, Intel) | Linux (X11 + Wayland; x64, arm64) |
|---|---|---|---|
| **wgpu backend** | DX12 primary (RapidRAW forces `Backends::PRIMARY` on Windows `[code: gpu_processing.rs:172]`), Vulkan as fallback; WARP software adapter for CI `[06 §8.4]` | Metal only. Unified memory. wgpu 30 caps Metal buffer-argument-table at 29 buffers/stage `[verified (secondary): https://github.com/gfx-rs/wgpu/releases]` | Vulkan (Mesa RADV/ANV/NVK, NVIDIA proprietary); lavapipe in CI. GL has no useful compute `[background]` |
| **Display ICC** | WCS (`GetICMProfile`); **Windows 11 24H2 Auto Color Management (ACM)** converts apps that assume sRGB to the monitor gamut and has produced desaturation/double-management reports; colour-critical apps must test ACM on/off `[verified (secondary): https://learn.microsoft.com/en-us/answers/questions/5532490/windows-11-latest-updates-cause-dramatic-reduction]` | ColorSync (`CGDisplayCopyColorSpace`). **wgpu issue #10013: `SurfaceColorSpace::Srgb` leaves the CAMetalLayer unmanaged (`setColorspace(nil)`), oversaturating 8-bit surfaces on wide-gamut displays** `[verified: https://github.com/gfx-rs/wgpu/issues/10013]`. RapidRAW pins wgpu 29 with a code comment saying the downgrade avoids P3 colour shifts on Apple devices `[code: Cargo.toml]` | X11: `_ICC_PROFILE` atom / colord `[background]`. Wayland: `color-management-v1` (below) |
| **Wayland colour mgmt** | n/a | n/a | Protocol is **staging, v3**, supports ICC v2/v4 + parametric + PQ/HLG `[verified: https://wayland.app/protocols/color-management-v1]`; merged upstream after ~5 years `[verified (secondary): https://www.phoronix.com/news/Wayland-CM-HDR-Merged]`. Mutter merged support for GNOME 48 (Feb 2025), GNOME 50 adds v2 + per-output ICC `[verified (secondary): https://www.phoronix.com/forums/forum/software/desktop-linux/1529938-gnome-48-mutter-merges-wayland-s-wp_color_management_v1-support, https://github.com/mangowm/mango/issues/782]`; KWin has ICC per screen and protocol support `[verified (secondary): https://userbase.kde.org/Kde_wayland_for_artists]`. **Client side is the gap**: GTK/Qt/Mesa support was "tentative" in late 2024 `[verified: https://www.phoronix.com/news/Wayland-Color-Management-Nears]`; I found no sign winit/wgpu expose it `[unverified]` -> we would bind `wp_color_manager_v1` ourselves on the native surface |
| **HDR output** | scRGB fp16 or HDR10 swapchain. **wgpu 30.0.1 docs table lists DX12 as supporting only `Srgb`/`Bt2100Pq` (not `ExtendedSrgbLinear`)** `[verified (secondary): https://docs.rs/wgpu/latest/wgpu/enum.SurfaceColorSpace.html]` -> may need a DXGI patch layer | Easiest: `Rgba16Float` + `ExtendedSrgbLinear`/`ExtendedDisplayP3` (EDR) supported by wgpu Metal, same docs table | KWin (Plasma 6) HDR works; GNOME experimental `[verified (secondary): https://gitlab.gnome.org/GNOME/mutter/-/work_items/3422]`; Vulkan HDR needs driver exposure. Treat as Phase 4 |
| **FS watching** | `ReadDirectoryChangesW` via `notify` 8.x; long paths; cloud placeholder files (OneDrive) look present but are not local `[background]` | FSEvents (coalesced, directory granularity); TCC prompts for Desktop/Documents/removable volumes `[background]` | inotify (per-user watch limit `fs.inotify.max_user_watches`); **Flatpak sandbox limits what is visible** `[verified (secondary): https://docs.flatpak.org/en/latest/sandbox-permissions.html]` |
| **Packaging/signing** | MSIX/MSI + SignPath Foundation | Notarized DMG, $99/yr | Flatpak + AppImage. Full table: [06 §7](06-tech-stack-options.md) |
| **CPU arch** | **x64 + ARM64.** `aarch64-pc-windows-msvc` is Rust **Tier 1 since 1.91** `[verified: https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/]`; `windows-11-arm` runners `[06 §7]`; RapidRAW's `build.rs` already maps `windows/aarch64` to an ORT DLL `[code]`; QNN EP exists for Snapdragon NPUs `[verified: https://onnxruntime.ai/docs/execution-providers/QNN-ExecutionProvider.html]` | **arm64 first.** GitHub retired `macos-13` on 2025-12-04; `macos-15-intel` is the last x86_64 runner and ends **Aug 2027** `[verified: https://github.blog/changelog/2025-09-19-github-actions-macos-13-runner-image-is-closing-down/, https://github.com/actions/runner-images/issues/13045]`. Plan: build `x86_64-apple-darwin` by cross-compiling on arm64 runners, or drop Intel after 2027 | `ubuntu-24.04-arm` runner `[06 §7]`; Raspberry Pi-class GPUs are out of scope |

### 1.2 Wayland vs X11 (matters if the shell is Tauri)

- wry's `build_as_child` only supports **X11** on Linux; Wayland child webviews need the `gtk::Fixed` route `[verified: https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html]`. This is the "native surface under a transparent webview" design from [06 §3.3](06-tech-stack-options.md), so the viewport spike must include a Wayland-native session, not just XWayland.
- **RapidRAW sidesteps it: on Linux and Android it hard-codes `use_wgpu_renderer = false`** (wgpu compute offscreen, JPEG frames pushed to the webview as `data:image/jpeg;base64,...`) `[code: lib.rs:400-403, :895-915]`. It also ships `tauri-plugin-wayland-nvidia-quirk` `[code: Cargo.toml]`. Treat that as the realistic fallback and the baseline to beat.
- Flatpak: Wayland by default with `--socket=fallback-x11`. NVIDIA + Wayland + Flatpak has launch failures where only XWayland works `[verified (secondary): https://discourse.nixos.org/t/nvidia-kde-plasma-wayland-flatpak-apps-only-launch-as-xwayland/75427]`; XWayland is blurry under fractional scaling `[verified (secondary): https://discuss.kde.org/t/whats-up-with-scaling-of-x11-applications-on-wayland/23809]`.
- Qt path: Qt handles Wayland natively and has the most mature colour/tablet story ([06 §3.5](06-tech-stack-options.md)).

### 1.3 Colour-management plan (applies to all desktops and mobile)

1. The engine's last node is an **own output transform** to a display profile (ICC via lcms2 or a pure-Rust CMM), then presents to a swapchain that the OS must **not** convert again. Each platform crate decides how to declare that (macOS: set `CAMetalLayer.colorspace` ourselves rather than trust wgpu; Wayland: bind `color-manager`; Windows: test ACM).
2. A `DisplayProfileProvider` trait (per OS) returns `{icc_bytes | named space, sdr_white_nits, headroom, change events}`. Mobile returns Display P3 / sRGB plus EDR headroom (iOS `maximumExtendedDynamicRangeColorComponentValue`, Android `Display.getHdrSdrRatio` `[background]`).
3. Do not promise Linux auto-profile detection in v1; offer "pick an ICC file per monitor" and auto-detect where `color-management-v1` is present.

### 1.4 File-system watching

`notify` 8.x: inotify (Linux/Android), FSEvents (macOS), `ReadDirectoryChangesW` (Windows), kqueue (iOS/BSD), polling fallback `[verified (secondary): https://crates.io/crates/notify]`. inotify cannot see SMB/NFS changes `[verified (secondary): https://lwn.net/Articles/896055/]`.
**Rule: a watcher is a hint, never the source of truth.** The catalog stores a per-folder fingerprint (count, max mtime, size sum) and reconciles on start, on focus, and on a timer. On mobile there is no recursive watch at all (§2.6), so the same reconcile path becomes the *only* path; build it first.

---

## 2. Mobile platform constraints

### 2.1 GPU memory and thermal limits

| Constraint | iOS | Android | Source / status |
|---|---|---|---|
| Memory model | Unified RAM; per-process jetsam cap. Entitlement `increased-memory-limit` (iOS 15+) raises it; one report: ~6 GB ceiling even on a 12 GB iPhone with the entitlement | LMK kills; no fixed per-app native cap; low-RAM devices (4-6 GB) common | `[verified (secondary): https://zenn.dev/mtfum/articles/ios_memory_entitlements?locale=en, https://github.com/boardsesh/boardsesh/pull/5524]` |
| Max 2D texture | 16384 on A9+ (older 8192) | typically 16384 (`maxImageDimension2D` 16383-16384 on a Mi 10) | `[verified (secondary): https://gist.github.com/kylehowells/63d0723abc9588eb734cade4b7df660d, http://vulkan.gpuinfo.org/displayreport.php?id=13693]` |
| Max buffer | `maxBufferLength` >= 256 MB on all iOS families, **observed 256 MB (iPhone 6 Plus) to 1433 MB (iPhone 12 Pro)** | `maxStorageBufferRange` 256 MiB on a Mi 10; older Mali/Adreno lower | same sources |
| wgpu defaults | `Limits::default()`: texture 8192, storage binding 128 MiB, buffer 256 MiB; `downlevel_defaults` texture 2048 `[background]` | same | A 100 MP mosaic (200 MB as `u16`) **exceeds both the default binding size and the default texture size** |
| Thermal | `ProcessInfo.thermalState` | ADPF `PowerManager.getThermalHeadroom` (Android 11+); Android 15 added headroom thresholds | `[verified (secondary): https://developer.android.com/games/optimize/adpf/thermal]`. Sustained GPU throughput after minutes of load is well below peak `[unverified: plan for 30-60%]` |

**Consequence:** budget against a *mobile floor profile* (§8.3), not against `adapter.limits()` wholesale. (RapidRAW requests `required_limits: adapter.limits()` and, if the image exceeds `max_texture_dimension_2d`, **bypasses the GPU** with "Try upgrading your GPU" `[code: gpu_processing.rs:236-242, :1829-1833]`; we need real tiling instead.)

### 2.2 wgpu on iOS and Android

- **iOS (Metal):** works; surface is a `CAMetalLayer` (e.g. hosted in a `UIView`); reference integration: https://github.com/jinleili/wgpu-in-app `[verified (secondary)]`. Pitfall: wgpu's Metal layer colour-space handling (§1.1). Background GPU work is not available on iPhone (§2.5).
- **Android (Vulkan, GLES fallback):** wgpu's Vulkan backend needs Vulkan 1.1 (or 1.0 + `VK_KHR_maintenance1/2`), `fragmentStoresAndAtomics`, `fullDrawIndexUint32`, `maxImageArrayLayers >= 256`, and BC *or* ETC2+ASTC `[verified (secondary): https://www.khronos.org/developers/linkto/webgpu-on-android]`. Real-world fragmentation, all 2026:
  - wgpu **30.0.1 panics on Mali G76** (`VK_ERROR_FRAGMENTED_POOL` treated as `unreachable!`), a regression vs v29 `[verified: https://github.com/gfx-rs/wgpu/issues/10263]`.
  - Adreno 610 reporting Vulkan 1.1.0 fails adapter creation and silently falls to the Null backend `[verified (secondary): https://github.com/TwilitRealm/dusklight/issues/2469]`; Adreno 740 Vulkan startup crash fixed by forcing GLES `[verified (secondary): https://github.com/TwilitRealm/dusklight/issues/2563]`; naga GLES shader failures on Adreno 730 `[verified (secondary): https://github.com/grovesNL/glyphon/issues/183]`.
  - **GLES is not a compute fallback below GLES 3.1** `[background]`; plan for "Vulkan GPU path -> CPU path", with GLES only as an experiment.
  - **Surface lifecycle**: Android destroys the `ANativeWindow` on pause; drop and recreate the surface (keep instance/device) `[verified (secondary): https://github.com/vanyastaff/flui/pull/1186]`; iOS emits `Resumed` on `applicationDidBecomeActive` `[verified (secondary): winit docs]`. Offscreen compute has no surface and avoids most of this, which is why RapidRAW's Android path works.
  - Mitigations: pin a wgpu version per platform and test it on a device matrix (Mali, Adreno, PowerVR/Xclipse); crash-guard flag file that falls back to CPU on the next launch after a GPU init crash (RapidRAW already does the flag-file part: `gpu_crash_flag_path` `[code]`); allowlist/denylist by `AdapterInfo` + driver version.

### 2.3 fp16

- Metal: native `half`; wgpu `SHADER_F16` available `[background]`. Vulkan: needs `shaderFloat16` (+ `16bit_storage` for storage buffers); **Mali exposes both, Adreno historically only the shader extension, not storage** `[verified (secondary): https://github.com/KhronosGroup/Vulkan-Samples/issues/363; old, re-check on current Adreno 7xx/8xx]`.
- **Design rule (already in [06 §2.2](06-tech-stack-options.md)): store intermediates as `rgba16float` textures, do arithmetic in f32, never *require* `SHADER_F16`.** RapidRAW does exactly this (converts to `half::f16` on the CPU and uploads, requests only `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`) `[code: gpu_processing.rs:506, :228-233]`. Add an optional f16-math fast path behind runtime detection later.

### 2.4 Tiling a 50-100 MP raw on a phone

| Quantity (100 MP, 11648x8736) | Size | Fits a phone? |
|---|---|---|
| Raw file (compressed) | ~100-200 MB | on disk; must be range-read/mmapped, not slurped |
| Decoded mosaic `u16` | 200 MB | CPU ok; GPU binding exceeds default 128 MiB limit |
| Demosaiced RGB f16 / RGBA f16 / RGBA f32 | 600 MB / 800 MB / 1.6 GB | **no** as a whole-image resource |
| One 2048+2x128 px tile, RGBA f16 | (2304^2 x 8 B) = 42 MB | yes; ~6 live intermediates = ~250 MB |
| One 1024+2x128 tile, RGBA f16 | 13 MB | yes on low-end |
| 2560 px smart-preview proxy (RGBA f16) | ~52 MB | yes, whole-image |

Design: (1) **edit on a proxy** (viewport-sized pyramid level, or the 2560 px proxy); (2) **export is a streaming tile pipeline**: decode in strips -> tile with halo = sum of downstream kernel radii (ROI back-propagation, [06 §2.3](06-tech-stack-options.md)) -> encode strips; never materialise a full-res intermediate; (3) **half-size/binned demosaic** fast path for 1:1-not-needed views (demosaic 2x2 blocks directly: 100 MP -> 25 MP, a classic mobile trick `[background]`); (4) make the tile size a function of `ResourceBudget` and the live thermal level; (5) checkpoint tile progress so an OS kill (jetsam/LMK) resumes instead of restarts. RapidRAW uses 2048 + 128 overlap tiles `[code: gpu_processing.rs:1023-1024]`, which matches the first row above. Phones also *produce* big files: iPhone 48 MP ProRAW and Samsung/Pixel 50 MP DNGs are ordinary inputs `[background]`.

### 2.5 Background execution

| | iOS | Android |
|---|---|---|
| Long export | `BGProcessingTask` gives minutes at the system's discretion; **iOS 26 `BGContinuedProcessingTask`** lets a user-started export continue after backgrounding with a progress UI `[verified (secondary): https://developer.apple.com/videos/play/wwdc2025/227/]` | Foreground service of type `mediaProcessing`/`dataSync`: **6 h per 24 h** budget in Android 15 `[verified: https://developer.android.com/develop/background-work/services/fgs/timeout]`; WorkManager `setForeground` |
| GPU in background | Background GPU is **iPad-only**: `BGTaskScheduler.supportedResources.contains(.gpu)` was false on every iPhone tested `[verified (secondary): same WWDC25 search results]` | Allowed in a foreground service, subject to thermal limits `[background]` |
| Implication | **The CPU path is a product requirement on iPhone** (background exports), not just a CI convenience | Exports should be a resumable service with a notification |

### 2.6 File access

| | iOS | Android |
|---|---|---|
| Camera roll | **PhotoKit**: `PHAsset` -> `PHAssetResource` (`.photo` for DNG/ProRAW, `.alternatePhoto` for the RAW of RAW+JPEG) `[verified (secondary): https://developer.apple.com/videos/play/wwdc2021/10160/]`; no filesystem path; `PHPhotoLibraryChangeObserver` for changes; limited-library selection | **MediaStore** + `READ_MEDIA_IMAGES`; Android 14 "Selected photos" (`READ_MEDIA_VISUAL_USER_SELECTED`) `[verified: https://developer.android.com/about/versions/14/changes/partial-photo-video-access]` |
| Folders (the Lightroom-Classic-style workflow) | `UIDocumentPickerViewController` + **security-scoped bookmarks** (persist across launches); iCloud/Dropbox files may be "dataless" and need `NSFileCoordinator` `[background]` | **SAF** `ACTION_OPEN_DOCUMENT_TREE` -> tree URI; persisted permission; `MANAGE_EXTERNAL_STORAGE` is heavily scrutinised on Play `[verified (secondary): https://anexplorer.io/solve/storage-permissions-android]` |
| Sidecars next to originals | Impossible for PhotoKit assets; possible inside a granted folder | Possible inside a granted SAF tree (sibling document) only |
| Change detection | PhotoKit observer; folder: poll on foreground | MediaStore `ContentObserver`; SAF children observer or poll `[background]` |
| **Tauri 2 plugin reality** | `dialog` returns `file://` and `fs` manages scoped access `[verified (secondary): https://v2.tauri.app/plugin/dialog/]`; known scoped-resource bugs `[verified: https://github.com/tauri-apps/plugins-workspace/issues/3030]` | `dialog` has **no folder picker on Android**; `fs` can read/write a URI but **cannot list a tree or create files in it** `[verified (secondary): https://github.com/tauri-apps/tauri/issues/14587]`; community crate `tauri-plugin-android-fs` fills gaps |

**RapidRAW's answer (the only open-source Rust+Tauri+wgpu data point)**: ~630 lines of hand-written JNI (`android_integration.rs`): resolve `content://` names, read a URI fully into memory, **import = copy into an app-private internal library** (`get_android_internal_library_root`), export via `MediaStore.insert` into the gallery or Downloads, no storage permission in the manifest (only `INTERNET`), the Show in file manager action unsupported `[code: android_integration.rs:97-580, file_management.rs:3123-3139, :3736-3790; AndroidManifest.xml]`. So: no in-place folder library on Android, an import-by-copy model.

**Rule for us:** the engine addresses data by `AssetRef` and `ByteSource` (ranged read), never by path (§8.5).

### 2.7 Raw capture formats and platform decoders

- **iOS:** Apple ProRAW **is DNG** (linearised, already partially processed) `[verified (secondary): https://developer.apple.com/videos/play/wwdc2021/10160/]`. **Core Image `CIRAWFilter`** decodes DNG/ProRAW and hundreds of camera RAWs with ~20 calibrated controls. **WWDC26 "RAW 9" (iOS/macOS 27): a tiled Core ML demosaic+denoise on the Neural Engine, supports DNG and ProRAW** `[verified: https://developer.apple.com/videos/play/wwdc2026/305/]`. RapidRAW already links `objc2-core-image` with `CIRAWFilter` on macOS `[code: Cargo.toml]`.
- **Android:** capture via Camera2 `RAW_SENSOR` + `DngCreator` -> DNG `[background]`. **There is no public "develop a raw" API comparable to `CIRAWFilter`.** The framework's Skia has `SkRawCodec` (Adobe DNG SDK + piex preview extraction) used by `ImageDecoder`/`BitmapFactory` `[verified (secondary): https://android.googlesource.com/platform/external/skia/+/6bd8639%5E!/]`, but it yields a basic render/preview, not controllable linear data; Skia upstream and downstream forks are removing RAW codecs `[verified (secondary): https://github.com/mono/SkiaSharp/pull/5162]`. Use it, at most, for instant thumbnails.
- **Design decision:** our pipeline starts from our own decode (rawler/LibRaw -> own demosaic -> camera matrix/DCP) on every platform, because **parity of look between desktop and phone is the product**. Platform decoders are (a) embedded-preview/thumbnail accelerators and (b) an *optional, non-parity* "Apple RAW 9 enhance" source behind the same trait as AI denoise, labelled as such (its output changes with the OS version).

---

## 3. UI framework mobile story

### 3.1 What RapidRAW actually shipped (ground truth)

From the clone `[code]`: `crate-type = ["staticlib","cdylib","rlib"]`; Cargo `[target.'cfg(target_os = "android")']` deps `ndk-context`, `jni` (two versions), `rustls-platform-verifier` (Android needs the platform trust store), `include_dir`; desktop-only deps (`trash`, `single-instance`, `reqwest`...) gated by `cfg(any(windows, macos, linux))`; `tauri.conf.json` has no mobile section, a Gradle project in `src-tauri/gen/android` (`minSdk 24`, `compileSdk/targetSdk 36`, Kotlin `MainActivity` doing edge-to-edge insets and routing the back button to JS: `window.__handleAndroidBack()`); **ORT is `load-dynamic` with a self-built `libonnxruntime.so` for arm64-v8a only** (`build.rs` panics for other Android targets; ORT 1.22.0 from their HF repo; CI sets `ORT_STRATEGY=manual`); **no iOS**: `gen/` contains only `android`, and `target_os = "ios"` appears twice in the whole tree. The README changelog shows touch support for masks, curves, sliders and scrolling (2026-04), signed APKs (2026-04-21) and mobile UI improvements (2026-08-31). Preview frames on Android are CPU-compressed JPEG data URLs (§1.2). Interpretation: **a ~1-person effort got Android working in ~6 months by accepting JPEG frame transport, copy-import, arm64-only and a web UI re-laid-out for portrait.** That is the Plan B cost floor. It is AGPL, so we may read it but must not copy code into an MPL core.

### 3.2 Tauri 2 mobile

- Stable 2.0 since Oct 2024; mobile "functional but younger than desktop"; mobile plugins in Swift/Kotlin; official plugins for barcode, biometric, geolocation, haptics, NFC, notifications `[verified (secondary): https://v2.tauri.app/blog/tauri-20/, https://blog.codercops.com/blog/tauri-2-desktop-mobile-apps-rust-web-2026]`. Real apps: RapidRAW (Android); an iOS App Store app "flow-like" `[verified (secondary): https://medium.com/@trivajay259/what-happens-when-you-ship-a-rust-heavy-app-to-the-ios-app-store-with-tauri-2-ddea0584bae7]`. 2024 iOS feedback: circular `tauri -> xcode -> tauri` build, thin iOS config schema, plugin gaps, app extensions break the simulator flow `[verified (secondary), dated 2024: https://github.com/orgs/tauri-apps/discussions/10197]`.
- System WebViews: WKWebView on iOS, **Android System WebView whose version depends on the device** `[verified (secondary): https://v2.tauri.app/reference/webview-versions/]`.
- **Native viewport on mobile:** all public wgpu-under-webview material is desktop-only (Tauri discussion #11944 mentions neither Android nor iOS `[verified (secondary)]`); no public Tauri-mobile app with a native GPU surface under the webview found `[unverified absence]`. In principle a Kotlin/Swift mobile plugin can add a `SurfaceView`/`CAMetalLayer` view behind a transparent WebView `[unverified, no reference]`.
- **Verdict:** good for a quick "engine on a phone" prototype; its gaps (folder access, native viewport, background tasks, PhotoKit) coincide with our hardest requirements.

### 3.3 Qt 6 on iOS/Android

- Qt open-source modules are LGPLv3/GPLv3 (some modules GPL-only) `[verified (secondary): https://www.qt.io/development/download-open-source]`.
- **Android:** dynamic linking of LGPL Qt is the supported route; no problem `[verified (secondary): https://wiki.qt.io/Licensing-talk-about-mobile-platforms]`.
- **iOS:** the problem is **LGPLv3's Installation Information clause**: the user must be able to run the re-linked binary, which rules out tivoization `[verified: https://www.qt.io/licensing/open-source-lgpl-obligations]`; on iOS a user cannot install a relinked binary except by re-signing with their own developer account. Qt forum/wiki consensus: use a commercial licence (available for mobile since Qt 5.1) or GPL the app `[verified (secondary): https://wiki.qt.io/Licensing-talk-about-mobile-platforms, https://forum.qt.io/topic/82014/question-about-qt-lgpl-license-for-android-ios]`. The older worry that Apple forbids dynamic libraries is obsolete (embedded frameworks are allowed) `[background]`, but the relink/installation problem remains regardless.
- **Verdict:** Qt remains a legitimate *desktop* fallback ([06 §3.5](06-tech-stack-options.md)); it would force a commercial licence or GPL-only distribution on iOS. Not a mobile shell for a free project.

### 3.4 Flutter + Rust

- `flutter_rust_bridge` v2.x is mature; **Ente** (open-source photos) uses Flutter UI + Rust via FRB `[verified (secondary): https://github.com/ente/ente/pull/13264]`.
- wgpu -> Flutter texture: `flutter_wgpu_texture` renders into shared Metal/D3D12/DMA-BUF surfaces on desktop but **Android/iOS "coming soon"**; `flutter_wgpu` claims IOSurface (iOS) and `SurfaceProducer` (Android); `wgpu-native-texture-interop` imports IOSurface/Vulkan images into wgpu textures `[verified (secondary): https://github.com/flowsai/flutter_wgpu_texture/, https://pub.dev/packages/flutter_wgpu, https://docs.rs/wgpu-native-texture-interop/latest/wgpu_native_texture_interop/]`. All young, single-maintainer, `[unverified in production]`.
- **Verdict:** one mobile UI codebase (Dart) over our Rust core; but we would own texture-sharing glue and Flutter's wide-gamut/EDR behaviour `[unverified]`; Dart is a third language for contributors. Second choice for mobile.

### 3.5 Slint

Slint 1.15 (Feb 2026) improved Android+iOS ports (safe area, virtual keyboard); 1.17 (Jun 2026) added C++ on Android and runs the Slint Viewer on both stores `[verified (secondary): https://slint.dev/blog/slint-1.15-released, https://slint.dev/blog/slint-1.17-released, https://extenly.com/2026/07/09/slint-1-17-whats-new-in-the-modern-ui-toolkit/]`. Licences: GPLv3, **royalty-free** (proprietary desktop/mobile/web, attribution required, no embedded), commercial `[verified: https://github.com/slint-ui/slint, https://slint.dev/faqs]`. On iOS you would use the royalty-free licence (GPLv3 hits §5). Mobile accessibility depth, gesture/pencil input and wgpu-texture embedding `[unverified]`. Attractive only if we also adopted Slint for desktop; we did not. Watch, do not bet.

### 3.6 egui / iced

eframe: Android supported, **no iOS in its native platform list**; AccessKit adapters exist for Windows/macOS, Android/iOS "planned" `[verified (secondary): https://docs.rs/crate/eframe/latest, https://accesskit.dev/looking-back-looking-forward/]`. iced: mobile "not a goal", community `iced_mobile` demo `[verified (secondary): https://github.com/Leinnan/iced_mobile]`. Dev/debug tooling only, as already concluded in [06 §3.5](06-tech-stack-options.md).

### 3.7 Native shells (SwiftUI / Compose) over UniFFI (recommended destination)

- **UniFFI** (MPL-2.0; Kotlin, Swift, Python, Ruby official) is "ready for production use" though pre-1.0 `[verified: https://github.com/mozilla/uniffi-rs]`.
- Who does it: **Mozilla** (Firefox mobile and desktop, application-services) `[verified: uniffi-rs README]`; **Element X** iOS/Android on `matrix-rust-sdk` via `matrix-sdk-ffi` `[verified (secondary): https://github.com/matrix-org/matrix-rust-sdk]`; **1Password**: Rust core for sync/storage/crypto, native SwiftUI/Android toolkit UIs, TypeShare for types `[verified (secondary): https://1password.com/blog/1password-8-the-story-so-far]`; Signal's `libsignal` (Rust core, Swift/JNI bindings) and Bitwarden's SDK `[background / unverified]`.
- **GPU viewport in a native shell:** iOS: `UIViewRepresentable` wrapping a `CAMetalLayer` view, wgpu surface from the layer `[verified (secondary): https://github.com/jinleili/wgpu-in-app]`; Android: `SurfaceView` -> `ANativeWindow` via JNI -> `wgpu::Surface`, recreated on `surfaceCreated/Destroyed` `[background]`.
- **Compose Multiplatform for iOS is stable (1.8.0, May 2025)** `[verified (secondary): https://x.com/kotlin/status/1919804324940550540]`, so a single Kotlin UI for both phones + UniFFI Kotlin bindings is a way to avoid two mobile UI codebases; native `UIKitView`/`AndroidView` interop hosts the viewport `[background]`.
- Why native: the hardest mobile requirements (PhotoKit, SAF, background tasks, gestures, Pencil/S Pen, accessibility, EDR) are platform APIs; the engine does the pixels; the UI is ~10 screens. A small team can afford it because the **mobile v1 UI is small** (§7).

### 3.8 Recommended architecture

```
                 +--------------------- amb-engine crates (pure Rust, MPL-2.0) ----------------------+
                 | amb-core  amb-color  amb-gpu(wgpu)  amb-cpu  amb-presets  amb-catalog  amb-sync   |
                 | amb-io (ByteSource, decode)   amb-ai (InferenceBackend)   amb-viewport             |
                 +-----------------------------------+------------------------------------------------+
                                                     |  one typed, pixel-free command/query protocol
                       +-----------------------------+-------------------------------+
                       |                             |                               |
              amb-shell-tauri (desktop)       amb-ffi (UniFFI + C ABI)           amb-cli (JSON)
              Tauri 2 + native wgpu           staticlib/cdylib                   headless, CI
              child surface; Qt fallback              |
                                          +-----------+-----------+
                                    SwiftUI + CAMetalLayer   Compose + SurfaceView
                                    PhotoKit/Files platform  SAF/MediaStore platform
                                    crate (amb-platform-ios) crate (amb-platform-android)
   optional desktop-only:  amb-gpl-pack (GPL-3.0-or-later: ported demosaic/NR/Upright, exiv2, lensfun C)
```

- **Desktop velocity is unchanged:** Tauri/Qt shells are thin clients of the protocol, exactly as already planned ([SCOPE §9](../SCOPE.md)).
- **Mobile becomes writing two thin shells plus two platform crates**, not porting the app.
- Cheap early proof (Phase 2.5, 2-3 weeks, optional): a Tauri-mobile Android build in RapidRAW style behind a `mobile` feature, purely to run the engine's golden suite and GPU limits on real phones.
- Decision gate for the *shell* (native-per-OS vs Compose Multiplatform vs Flutter) is deferred to a Phase 5 spike; nothing in Phases 0-2 depends on it.

---

## 4. Dependencies on mobile

| Dependency | Licence | Cross-compiles to iOS / Android? | App Store / Play issue | Action |
|---|---|---|---|---|
| **wgpu** | MIT/Apache-2.0 | Yes / Yes (Vulkan, GLES). Per-version device bugs (§2.2) | none | Pin per platform; device matrix |
| **rawler** (dnglab) | **LGPL-2.1** `[verified: https://github.com/dnglab/dnglab]`; API unstable, no SemVer | Pure Rust: yes / yes (RapidRAW ships a fork on Android `[code]`) | Statically linked by Cargo -> LGPL relink + "no further restrictions" problem on iOS (§5.5) | Desktop: fine. iOS: avoid, or build as a separate dynamic framework with counsel sign-off; ask upstream for a permissive/dual licence `[unverified]` |
| **LibRaw** | **dual LGPL-2.1 / CDDL-1.0**, "you can choose" `[verified: https://www.libraw.org/about]`; may change in future versions | C++: yes / yes (NDK/CMake) `[background]` | **CDDL-1.0 is file-level copyleft that allows distributing executables under other terms (§3.5)** `[background]` -> the App-Store-safe option; FSF considers CDDL GPL-incompatible `[background]`, so the GPL desktop build takes the LGPL option | Per-target licence selection; document it in `cargo-deny`/SPDX notes |
| **lcms2** | MIT | C: yes / yes | none | Keep; pure-Rust CMM (`moxcms`, BSD-3/Apache) as alternative `[unverified]` |
| **lensfun** | LGPL-3 (lib), **CC-BY-SA (DB)** | glib dependency makes mobile builds painful `[background]` | **LGPL-3 Installation Information on iOS (§5.5)**; DB share-alike vs store DRM `[unverified]` | Reimplement distortion/TCA/vignetting models in Rust (published formulas); consume the DB as data. RapidRAW does this (`lens_correction.rs` + bundled `lensfun_db`) `[code]` |
| **exiv2** | **GPL-2.0-or-later** `[verified: https://github.com/Exiv2/exiv2]` | C++: possible, no mobile docs | **Excluded from iOS under any GPL-based analysis** | Core: `kamadak-exif` (BSD-2) read, `little_exif` write, `quick-xml` for XMP (RapidRAW uses all three `[code]`). exiv2 desktop-only behind a `MetadataBackend` trait for maker-note breadth |
| **ONNX Runtime** + `ort` | MIT | Yes / Yes. iOS EPs: CoreML, XNNPACK; Android: NNAPI, XNNPACK, QNN (Snapdragon only) `[verified: https://onnxruntime.ai/docs/tutorials/mobile/]`. **NNAPI is deprecated since Android 15** (Google points to the LiteRT GPU runtime) `[verified: https://developer.android.com/ndk/guides/neuralnetworks]` | none. Size: stock AAR ~24 MB; custom reduced-operator build ~7.5 MB (ResNet50 example) `[verified: ORT mobile page]`; needs ORT-format models. **iOS cannot dlopen downloaded code**: `load-dynamic` (as RapidRAW does on Android) must become a statically linked/bundled xcframework `[background]`. Android 16 KB page alignment required for Play (NDK r28+) `[verified (secondary): https://android-developers.googleblog.com/2025/05/prepare-play-apps-for-devices-with-16kb-page-size.html]` | Keep `InferenceBackend` trait; int8/fp16 models; plan a Core ML export path for iOS and a LiteRT path for Android as alternatives to ORT EPs `[unverified]` |
| **SQLite** (`rusqlite` bundled) | public domain | Yes / Yes | none | Never place the live DB in a synced folder (§6) |
| **libheif** | **LGPL** (3.0 `[unverified]`); HEVC via libde265 (LGPL), x265 (GPL) `[verified: https://github.com/strukturag/libheif]` | possible | LGPL on iOS; HEVC patent pools | Mobile: use **ImageIO (iOS)** and **ImageDecoder (Android, HEIC API 28+, AVIF API 31+ `[background]`)** via platform crates |
| **libjxl / jxl-oxide** | BSD-3 `[background]` / MIT-Apache `[code: Cargo.toml]` | Yes / Yes (pure Rust decode) | none | Use as the smart-preview codec (§6.4) |
| `mozjpeg-rs`, `webp`, `image` | permissive | Yes | none | Fine |
| `tauri` and plugins | MIT/Apache | iOS/Android: §3.2 | none | **Must not appear in engine crates** |
| `rayon`, `memmap2` | MIT/Apache | Yes; mmap of `content://` needs an fd (`ParcelFileDescriptor.detachFd`) `[background]` | none | `mmap` feature flag; default to ranged reads |
| `notify` | CC0/Artistic | iOS: kqueue only; no recursive watch | none | Desktop feature `desktop-fs-watch` |
| WASM plugins (wasmtime) | Apache | **JIT is not allowed on iOS** `[background]` | n/a | Plugins desktop-only; Lua (interpreter) ok |

---

## 5. Licensing and app stores (critical)

### 5.1 The conflict and its history

- Apple's store terms (end-user licence limited to Apple devices the user owns/controls, usage rules, FairPlay DRM) impose restrictions that GPLv2 §6 / GPLv3 §10 forbid ("no further restrictions"); GPLv3 §6 adds the anti-tivoization "Installation Information" duty. The FSF's position: because Apple "only" allows listed activities, anything unlisted is forbidden, hence a GPL violation `[verified (secondary): https://www.fsf.org/news/2010-05-app-store-compliance, https://lwn.net/Articles/391423/]`.
- **GNU Go** was pulled by Apple on 2010-05-26 after FSF enforcement `[verified (secondary), same sources]`.
- **VLC**: a copyright holder (Rémi Denis-Courmont) complained; the app left the store in Jan 2011 `[verified (secondary): https://www.fsf.org/blogs/licensing/vlc-enforcement]`. The project then relicensed the **engine** (libVLC) to **LGPL-2.1+** (from Oct 2011; ~230k LOC converted, ~150k LOC remained GPL; consent obtained from 230 contributors = more than half of coders, top 32 = 91% of commits; code from non-responders was not relicensed) `[verified: https://jbkempf.com/blog/How-to-properly-relicense-a-large-open-source-project-part-3/]`, and the iOS app returned in **July 2013 under MPL-2.0** `[verified (secondary): https://en.wikipedia.org/wiki/Software_relicensing]`. Caveat: a VLC contributor says the LGPL move was not only about Apple and that LGPL gave no practical advantage for app store distribution `[verified (secondary, a comment): https://lwn.net/Articles/526355/]`. Lesson: **it took ~2.5 years and a consent campaign, and the winning licence was MPL (weak copyleft with an explicit clause allowing executables under other terms), not LGPL.**

### 5.2 What the law/practice actually is, as of Oct 2026

- **A copyright holder may ship its own code through the App Store** (it is the licensor, not bound by its own licence); the violation arises when the binary contains **third-party GPL code** whose holders have not granted permission. That is why Signal-iOS (AGPLv3) and Nextcloud iOS (GPLv3 **plus an App Store exception**, copyright held by contributors) are on the store `[verified (secondary): https://github.com/signalapp/Signal-iOS, https://nextcloud.com/blog/nextcloud-ios-app-free-for-download-on-app-store/]`, and Signal turned libsignal-protocol-c's rule into an explicit **additional permission** for GPL-compliant apps `[verified: https://signal.org/blog/license-update/]`.
- I found **no 2025-2026 court ruling or revised FSF/Apple statement** that changes the 2010-2013 analysis `[unverified: absence of evidence]`; Apple's end-user licence terms still restrict use to Apple devices `[unverified for 2026]`. Treat "GPL code in the App Store binary" as **a standing legal risk triggered by any single copyright holder's complaint** (VLC precedent), not as a settled "it's fine".
- Same analysis for the **Mac App Store** (optional for us).

### 5.3 Alternative distribution (2025-2026)

| Channel | Status | Fit |
|---|---|---|
| **EU (DMA)** | Alternative marketplaces + web distribution exist; notarization still required; EU-resident Apple IDs and EU location `[verified (secondary): https://iossubmissionguide.com/eu-dma-alternative-app-distribution/ (pre-Oct-2026 terms)]`. **New terms from 2026-10-01**: per-install CTF replaced by a flat **5% Core Technology Commission on digital goods** in apps distributed outside the store; eligibility to run a marketplace/distribute via web loosened to include **nonprofits**, audited orgs, etc. `[verified (secondary): https://www.macrumors.com/2026/08/18/eu-app-store-fee-change/, https://techcrunch.com/2026/08/18/apple-overhauls-its-eu-app-store-fees-loosens-rules-for-alternative-app-stores/]`. A free app with no digital goods should owe no commission `[unverified reading]`; **an Apple Developer Program membership (~$99/yr) is still needed** | Possible for a GPL iOS build outside the Apple EULA, EU only; AltStore PAL exists `[verified (secondary): https://techcrunch.com/2024/07/24/alternative-app-store-altstore-pal-adds-third-party-ios-apps-in-wake-of-eu-apple-ruling/embed/]` |
| **Japan (MSCA)** | Since 2025-12-17 / iOS 26.2: alternative marketplaces, notarization for all apps, 5% commission; AltStore PAL launched there `[verified (secondary): https://www.apple.com/newsroom/2025/12/apple-announces-changes-to-ios-in-japan/, https://9to5mac.com/2025/12/18/following-sweeping-app-store-changes-altstore-pal-launches-in-japan/]` | As above, Japan only |
| **US / rest of world** | App Store only (Epic ruling allows external *payment links*, not sideloading) `[verified (secondary): https://www.macrumors.com/2025/05/01/apple-updates-u-s-app-review-guidelines-epic/]`; TestFlight and free-cert sideloading are still "distribution" | A GPL-only iOS app has a **small, region-limited audience** |
| **Google Play** | GPL fine `[background]` | OK |
| **F-Droid** | GPL fine; builds from source with its own signing key; **Google developer verification** (identity-verified developers only on certified devices) is enforced from 2026-09-30 in Brazil, Indonesia, Singapore, Thailand and globally in 2027; F-Droid's model conflicts with it; F-Droid 2.0 shipped 2026-09-24 `[verified (secondary): https://android.gadgethacks.com/news/google-android-sideloading-restrictions-sept-30-explained/, https://pinggy.io/blog/f_droid_2_0_android_developer_verification/]` | OK today; **register our package name and developer identity with Google early** so sideloaded APKs keep working |

### 5.4 Impact of the earlier recommendation (GPL-3.0-or-later + ports)

SCOPE's licence choice exists to (a) port darktable/RawTherapee/ART code (all GPL-3), (b) use exiv2, (c) use lensfun/LibRaw, (d) allow an AGPL RapidRAW fork. For a future iOS app this means:

1. **Ported code**: owned by hundreds of third-party holders; not exceptable. An iOS binary containing it is exactly the VLC situation (anyone can complain; Apple pulls first).
2. **exiv2** (GPL-2+) and **lensfun** (LGPL-3) cannot ship in the iOS binary either.
3. **Our own** GPL code *can* be excepted if every contributor consents (Nextcloud model), but ported code in the same crates poisons the binary anyway.
4. Android is unaffected; desktop is unaffected.
5. Reversal cost grows with every merged outside commit. VLC: ~2.5 years.

### 5.5 The LGPL static-linking problem on iOS

- LGPL-2.1 §6 requires the user be able to relink/replace the library (provide object files or a shared-library mechanism); LGPLv3 §4 adds **Installation Information** for "User Products" `[verified: https://www.qt.io/licensing/open-source-lgpl-obligations]`.
- iOS permits embedded dynamic frameworks (so the claim that iOS has no dynamic linking is outdated `[background]`), but a relinked build only runs if the user re-signs it with their own developer account, and Apple's terms still add further restrictions (`LGPL-2.1 §10`) `[background]`. Rust's default is static linking of crates, so **LGPL Rust crates (rawler) are statically linked unless deliberately split out**.
- Weak-copyleft licences with an explicit "distribute the executable under different terms" clause avoid the problem: **MPL-2.0 §3.2, CDDL-1.0 §3.5** `[background]`. This is the principle behind LibRaw-under-CDDL and VLC's MPL iOS app.
- Practical risk reading: huge numbers of iOS apps ship LGPL libraries (FFmpeg etc.) without enforcement `[verified (secondary): https://wiki.qt.io/Licensing-talk-about-mobile-platforms]`; **for a project whose identity is licence-clean, "everyone does it" is not a plan.** Avoid LGPL-3 outright on iOS; treat LGPL-2.1 as counsel-reviewed only.

### 5.6 Options compared

| Option | Ports from darktable/RT/ART | iOS App Store | Contributor friction | Reversal cost | Verdict |
|---|---|---|---|---|---|
| **A. GPL-3.0-or-later everywhere** (current recommendation) | Yes, anywhere | Only region-limited alt-distribution (EU/JP), or after a VLC-style purge | None | Very high (consent of all contributors) | Fine if mobile = Android + desktop forever |
| **B. GPL + App Store exception** (Nextcloud/Signal pattern, GPLv3 §7 additional permission) | Yes, **but ported files cannot carry the exception**; an iOS binary must exclude them | Yes for own code; excluded deps (exiv2, lensfun, ports) must still go | Low if the exception is in the licence **from the first commit** (inbound = outbound); a CLA is unnecessary for this | Medium | Useful as an add-on for the *GPL shells*; does not fix the ported-code problem |
| **C. Dual-licensed core** (GPL + commercial/store licence from an owning entity) | Yes for GPL half only; ports cannot be relicensed | Yes via the commercial grant, for own code only | **Needs a CLA/assignment**, an owning legal entity, and open-core trust | Low once the CLA exists | Wrong for a "free forever" community project unless a foundation holds the rights |
| **D. Apache-2.0/MPL-2.0 everything, clean-room only** | **No** (clean-room from papers); no exiv2 | Yes | None | Low | Safest for mobile; **slows desktop v1** (SCOPE calls ports the largest schedule lever) |
| **E. Split: MPL-2.0 core + GPL-3.0-or-later desktop packs (recommended)** | Yes, in `amb-gpl-pack` crates (desktop only) | Yes (core is MPL; excludes the pack) | Low; DCO; contributors choose which licence zone they touch | Low (boundary exists from day one) | **Recommended** |

### 5.7 Recommendation and reasoning

**Adopt Option E, decided before the first external contribution.**

- **MPL-2.0 for all engine crates and the mobile shells.** Reasons: it is what VLC used to get back on the iOS store and what Firefox iOS ships `[background]`; file-level copyleft keeps improvements open (unlike Apache/MIT, it blocks a closed fork of our engine, which matches "free forever"); MPL-2.0 §3.3 is GPL-compatible, so the desktop build can combine MPL core + GPL packs into a GPL-3.0-or-later whole; it permits Apache/MIT deps (wgpu, ort, rusqlite) and LibRaw-under-CDDL.
- **GPL-3.0-or-later for `amb-gpl-pack-*`** (ported demosaic/denoise/Upright/lens code, exiv2/lensfun C bindings, any code taken from darktable/RT/ART) and optionally for the desktop shells. SPDX header on every file; `cargo-deny` and a CI check forbid `amb-core*` crates from depending on `amb-gpl-*`.
- **The seam is a set of traits with permissive baseline implementations in core** (demosaic, denoise, lens model, metadata). Desktop gets the better GPL implementations as drop-in packs; an iOS/Android build uses the baselines. Baselines can start trivial (bilinear/VNG demosaic, simple luminance NR, lens polynomials from published formulas) and be upgraded by clean-room work before mobile ships `[estimate: 2-4 person-months for clean-room RCD/VNG + basic NR + lens model, unverified]`; this does **not** block desktop v1.
- **Clean-room protocol** for core algorithms: implement from papers/spec, authors who have not read the GPL source for that module, reviewer sign-off recorded in the PR. Algorithms are not copyrightable but implementations are; patents are separate (`[background]`).
- **DCO, not a CLA.** With an MPL core, no relicensing right is needed for the store. If we later want a store *exception* on GPL shell code (Option B) it can be in the licence text from the start. A CLA is only worth its friction if a foundation wants dual-licensing revenue (Option C), which we do not.
- **If the owner prefers Option A anyway:** record explicitly that iOS = EU/JP alt-distribution only (or a purge later), keep the `amb-gpl-*` crate boundary regardless (it is free), and still avoid GPL-only deps in core.
- **Dependency licence map** (enforced by `cargo-deny`): core allows MIT, Apache-2.0, BSD-2/3, MPL-2.0, Unicode, CDDL-1.0 (LibRaw only), zlib, CC0; denies GPL/AGPL/LGPL; packs allow GPL-3.0-or-later and LGPL.
- Residual counsel items (**before 1.0 and before any iOS submission**): CC-BY-SA lensfun DB vs store DRM; Apple's current EULA text; CDDL-1.0 venue clause in LibRaw `[unverified]`; trademark/name; whether Apple will accept an app whose "source of truth" is open source under MPL with a closed-source distribution wrapper (it is routine `[background]`).

---

## 6. Sync between desktop and mobile without our own cloud

### 6.1 Tiers

| Tier | Mechanism | Ships in |
|---|---|---|
| T0 | **XMP sidecars** (`crs:` for Lightroom interop, plus our `amb:` namespace with `amb:rev`, `amb:device`, `amb:original-hash`, `amb:recipe` JSON) beside originals (SCOPE Phase 2) | Phase 2 |
| T1 | **User-chosen folder** carries sidecars, proxies and the op-log (iCloud Drive, Dropbox, Syncthing, Nextcloud/WebDAV mount) | Phase 3 |
| T2 | **Smart-preview proxies** (§6.4) so phones edit without originals | Phase 3-4 |
| T3 | **Catalog-level merge** via per-device append-only op-log (§6.3) | Phase 4+ |

### 6.2 Folder-sync providers (what to expect)

- **Syncthing**: official Android app **discontinued Dec 2024**, community `Syncthing-Fork` continues; iOS only via third-party Möbius Sync `[verified (secondary): https://www.ghacks.net/2024/10/21/syncthing-for-android-is-being-discontinued-but-theres-an-alternative-app-you-can-switch-to/, https://en.wikipedia.org/wiki/Syncthing]`. Good for desktop<->desktop and Android, weak on iOS background sync `[background]`.
- **iCloud Drive**: native on iOS/macOS, Windows via iCloud for Windows, **none on Linux**; "optimise storage" leaves files dataless until read (coordinated access required) `[background]`.
- **Dropbox**: all platforms; iOS File Provider; smart-sync placeholders `[background]`.
- Hazards: placeholders, clock skew and unreliable mtime, tools that rewrite files non-atomically, case-insensitive collisions, conflict-copy names ("file (conflicted copy).xmp"). Mitigations: write temp + atomic rename, content hash in `amb:original-hash`, treat unknown `*conflict*` siblings as input to merge, never trust mtime alone.
- **Never put the live SQLite catalog (or its `-wal`/`-shm`) in a synced folder**; the catalog stays device-local and is rebuilt from sidecars + op-log `[background]`.

### 6.3 Catalog merge: avoid a general CRDT library in v1

- Identity: **content hash of the original** (blake3, as RapidRAW uses `[code]`), not path.
- **One append-only file per device** (`<device-id>.jsonl` in a `ambrotype.sync/` folder): each writer touches only its own file, so dumb folder sync cannot conflict; readers merge all files. Ops carry a **hybrid logical clock**; fields merge as **LWW registers** (rating, flag, label, recipe revision), keywords/collections as **OR-sets**, history/snapshots as append-only lists keyed by (device, seq). Concurrent recipe edits: keep the later, store the other as a named snapshot ("edited on <device>") rather than silently dropping it.
- Libraries exist if we outgrow this: Automerge 3.0 (Rust core, May 2025), Loro 1.x (Rust, movable tree) `[verified (secondary): https://github.com/automerge/automerge, https://www.pkgpulse.com/guides/yjs-vs-automerge-vs-loro-crdt-libraries-2026]`. **cr-sqlite's last push was Oct 2024; maintenance uncertain** `[verified (secondary): https://github.com/vlcn-io/cr-sqlite]` -> avoid.

### 6.4 Smart-preview-style proxy

- Lightroom's smart preview is a **lossy DNG, ~2560 px long edge, ~1/20 of the original**, uploaded instead of originals for mobile editing `[verified (secondary): https://helpx.adobe.com/lightroom-classic/help/lightroom-smart-previews.html; 01-lightroom-feature-inventory.md §previews]`.
- **Proposal `[unverified, design]`: `.fbproxy`** = JPEG XL (lossy, 16-bit-capable, pure-Rust decode with `jxl-oxide`) of the demosaiced, camera-RGB, **linear-light** image with a fixed high-range transfer, 2560 px long edge, plus embedded metadata: camera matrix/profile id, WB multipliers, baseline exposure, lens-correction parameters, orientation, original hash. The mobile engine runs the *same* node graph with `Source::Proxy` (skipping demosaic/CA).
- **Spatial parameters must be resolution-independent** (radius/amount defined relative to long edge or "100% of original pixels") or the proxy preview will not match full-res output. Make this an engine rule now (§8.4).
- Mobile edits produce a recipe in the sidecar; **export** uses the original if reachable (desktop, or full file on the phone) else the proxy, flagged in the UI and metadata.
- DNG as a proxy container would be interoperable but pulls in the DNG SDK; skip.

### 6.5 PhotoKit-sourced assets

No sidecar can sit next to a `PHAsset`; recipes live in the app's own store keyed by `localIdentifier` + content hash. Optional later: write Photos "adjustment data" (`PHContentEditingOutput`) so edits appear in Photos `[background]`.

---

## 7. What a mobile v1 should contain

Reference: Lightroom mobile Loupe panels are Actions, Presets, Crop, Edit (Light, Color, Effects, Detail, Optics, Geometry), Remove, plus Masking; press-and-hold for Before; Undo/Redo; presets/profiles with an Amount slider; export `[repo: adobe-docs/cloud-editing.md §1-13]`.

**Mobile v1 = "edit-and-export companion":**

| Area | In v1 | Notes |
|---|---|---|
| Sources | PhotoKit selection, Files/SAF folder, import-by-copy fallback | RapidRAW-style copy is the floor |
| Library | Grid, ratings/flags, simple filters, sync status | No catalog features beyond that |
| Develop | Profiles, **presets with Amount (including imported Lightroom XMP presets, the product's differentiator)**, Auto, Light, Color (WB, HSL, grading), Effects (vignette, grain), Detail (sharpen, luminance NR), Geometry (crop/rotate/straighten), tone curve | Touch-first sliders and a curve editor; before/after by press-and-hold |
| Masks | Linear, radial, brush (with pressure), **AI Select Subject / Sky** | int8/fp16 SAM-class encoder downloaded on demand ([06 §5.2](06-tech-stack-options.md)); cache masks in the catalog |
| Remove | Spot heal/clone | Phase after v1 if schedule slips |
| Undo/History | Linear undo + snapshots | Matches protocol transactions |
| Export | JPEG/HEIC (and JXL), sRGB/Display P3, resize, share sheet, save to Photos/MediaStore | **Foreground** GPU/CPU export on iPhone; resumable service on Android |
| Sync | Sidecar + proxy + op-log (§6) | |

**Desktop-only (Phase 3+ and beyond mobile v1):** smart collections/stacks/keyword hierarchy/faces, `.lrcat` import, tethering, HDR/pano merge, soft proofing, print, batch/watch-folder export presets, plugin/scripting API, calibration panel, Upright and heavy lens workflow, large AI (inpaint, denoise, super-resolution), `amb-gpl-pack` quality modules, multi-monitor/colour-critical display management, in-camera capture (Lightroom mobile has it; we do not).

---

## 8. "Do this now so mobile stays possible" (Phases 0-2)

### 8.1 Workspace and features (Phase 0)

- [ ] **Engine crates depend on no UI/OS crates.** `cargo-deny` `bans` for `tauri*`, `gtk*`, `winit`, `webkit2gtk`, `objc*`, `windows*`, `jni`, `ndk*` in `amb-core/color/gpu/cpu/presets/catalog/sync/io/ai/viewport`. OS code lives only in `amb-platform-{win,mac,linux,ios,android}` and shell crates. No `cfg(target_os)` inside engine crates.
- [ ] **Feature flags**: `gpu`, `cpu-simd`, `ai` (ort), `mmap`, `desktop-fs-watch`, `heif`, `jxl`, plus the licence-zone features `gpl-pack`, `exiv2`, `lensfun-c`, `libraw`, `rawler`. `default = ["cpu-simd"]`. A `mobile` profile = `gpu + cpu-simd + ai`.
- [ ] **Licence zones in the repo layout** (§5.7): `crates/core/*` (MPL-2.0), `crates/gpl-pack/*` (GPL-3.0-or-later), SPDX headers, DCO bot, CI check that core never depends on `gpl-pack`.
- [ ] **FFI crate from day one**: `amb-ffi` with `crate-type = ["staticlib","cdylib","rlib"]` (as RapidRAW's lib `[code]`), exposing the protocol via UniFFI records/enums. It can be empty at first; its job is to keep the protocol serialisable and coarse-grained. Keep `panic = unwind` for UniFFI's panic catching `[unverified]`.

### 8.2 CI (Phase 0, check-only; Phase 1 add tests)

- [ ] `cargo check --workspace --exclude <shell crates> --target aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (and `x86_64-linux-android` for emulators) on `macos-14/15` and `ubuntu-24.04`; `--no-default-features` and `--features mobile`. Rust tier: these are Tier 2 `[background]`. Use `cargo-ndk`, NDK **r28+** (16 KB page alignment, Play requirement for apps targeting Android 15+ from 2025-11-01 `[verified (secondary): https://www.androidpolice.com/google-play-new-memory-rules-16-kb/]`).
- [ ] Phase 1: run the CPU golden suite on an Android emulator or device farm and iOS simulator; run the GPU suite under lavapipe with the **mobile floor limits** (§8.3). Real-device GPU runs on a small donated device matrix (Mali, Adreno) `[unverified]`.
- [ ] Phase 2: nightly "ORT reduced-op build + model smoke test" for Android arm64 and iOS; track NNAPI deprecation.

### 8.3 GPU limits budgeting (Phase 1)

- [ ] Define `MOBILE_FLOOR: wgpu::Limits` (texture <= 8192, storage binding <= 128 MiB, <= 8 storage buffers/stage, <= 4 bind groups, workgroup <= 256 invocations, <= 16 KiB workgroup memory, `rgba16float` storage only). **All kernels must pass golden tests with `required_limits = MOBILE_FLOOR`** (wgpu enforces it, so this is a free, deterministic check even on desktop CI).
- [ ] Tile planner takes `{limits, ResourceBudget{gpu_bytes, cpu_bytes, thermal}}`; never `adapter.limits()` wholesale; **no whole-image GPU resource > one tile + proxy**; raw mosaic uploaded per tile/strip, never as a single 100 MP buffer.
- [ ] Required wgpu `Features` list kept short and tracked in one file: today only `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` (what RapidRAW needs `[code]`); every other feature needs a fallback; **never require `SHADER_F16`**.
- [ ] OOM/device-loss path: catch, halve tile, retry, fall back to CPU (already in [06 §2.3](06-tech-stack-options.md)); add **surface-loss and device-recreate** handling with no re-decode (re-upload from the tile cache).
- [ ] GPU crash guard + adapter allow/deny list (flag file as RapidRAW does `[code]`).
- [ ] Swapchain/surface behind an own trait (`PresentTarget`), not wgpu types, so macOS layer colour-space and Android surface lifecycle can be handled per platform (also [06 §8.4](06-tech-stack-options.md)).

### 8.4 Engine semantics (Phase 1)

- [ ] **Spatial params resolution-independent** (radius in fractions of long edge or reference pixels) so proxy previews match export.
- [ ] **Deterministic CPU/GPU parity** tolerances (exists) plus **cross-arch determinism** (x86_64 vs aarch64 NEON) tested in golden CI; avoid `fast-math`-like drift; note fp16 storage rounding.
- [ ] Streaming decode and export APIs: `decode_into(&mut dyn TileSink)`, half-size demosaic mode, `Cancel` token, progress callbacks, **resumable tile export checkpoints**.
- [ ] Engine is **suspend/resume safe**: `pause()` flushes caches, `resume()` rebuilds device/surface.
- [ ] **No subprocesses, no downloaded executable code** in the engine (iOS forbids both `[background]`); models are data only.

### 8.5 IO and catalog (Phases 1-2)

- [ ] **No `PathBuf` in public engine APIs.** Use `AssetRef { source_id, locator: opaque, content_hash }` where `locator` can be a file URI, `content://` URI, PhotoKit `localIdentifier` or a security-scoped bookmark blob.
- [ ] `ByteSource` trait (`len`, `read_at`, optional `as_mmap`), `ByteSink` for atomic writes; `SidecarProvider` trait (may say "no sidecar possible here"); permission loss is a normal error value, not a panic.
- [ ] `LibrarySource` trait: `list(page)`, `stat`, `open`, `changes()` (push where available, otherwise reconcile). Folder watcher is one implementation (`desktop-fs-watch`).
- [ ] Catalog schema: library roots by id + relative paths; content-hash identity; UTC + HLC timestamps; `device_id`; SQLite `bundled`, WAL, `busy_timeout`; idempotent migrations; **catalog file never inside a synced folder**.
- [ ] XMP read/write in core with `quick-xml` + `amb:` namespace; metadata behind a `MetadataBackend` trait (exiv2 desktop-only).
- [ ] Preview pyramid and mask caches addressable by content hash + recipe hash, so they can be dropped under memory pressure and rebuilt.

### 8.6 UI protocol (Phase 2, touch-friendly and pixel-free)

- [ ] **Two viewport transports** behind one `Viewport` trait: `NativeSurface(raw_handle, size, scale, colour_space)` and `FrameStream(JPEG/WebP/RGBA over IPC or shared memory)`. RapidRAW's Linux and Android builds prove the second is needed `[code]`; it also serves headless tests and remote sessions.
- [ ] **Semantic input events, not mouse events**: `pan`, `pinch{center, scale}`, `tap`, `long_press`, `stroke{points, pressure, tilt, radius}`, `drag_handle{id, pos}`, `scrub{param, delta}`; no hover-only affordances; before/after = press-and-hold; hit targets >= 44 pt (iOS) / 48 dp (Android) `[background]`.
- [ ] **Param edits as transactions**: `begin(param) / update(value) / commit` so interactive drags coalesce into one undo step and can render at reduced quality while active.
- [ ] Coordinates in **normalised image space with orientation**; overlays (mask outlines, crop grid, clipping) produced by the engine as vector/bitmap layers, not drawn by shells.
- [ ] Layout-agnostic panel model: the protocol exposes a **parameter schema** (id, type, range, default, group, "supports mask") so desktop panels and mobile panels are generated from the same description.
- [ ] Display-profile and HDR headroom come **from the shell** through `DisplayProfileProvider`, with change events (§1.3).

### 8.7 Dependencies and packaging (Phases 1-2)

- [ ] Choose permissive-first for every capability and keep GPL/LGPL ones behind traits: metadata (kamadak-exif/little_exif/quick-xml), lens (own models + lensfun DB), HEIC/AVIF (platform codecs on mobile), JXL (`jxl-oxide`), colour (lcms2/MIT).
- [ ] Raw decode behind a `RawDecoder` trait with `rawler`, `libraw` (per-target licence option, §4) and a future in-house decoder as implementations.
- [ ] `ort`: keep it behind `InferenceBackend`; do not assume `load-dynamic` on iOS; prefer ORT-format models and an operator-reduced build; ship int8/fp16 variants; model downloads are data-only with size/cellular warnings.
- [ ] Plugins: WASM (JIT) and subprocess plugins are desktop-only; Lua (mlua) is the only scripting layer allowed on mobile.
- [ ] Register the Android package name/identity with Google developer verification, reserve the iOS bundle id, and hold the Apple Developer account in the fiscal host's name ([06 §7](06-tech-stack-options.md)).

---

## 9. Risk table

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | GPL-3.0-or-later with ported code blocks the App Store; relicensing later needs consent of all contributors (VLC: ~2.5 years) | High if option A | High | Option E now; crate boundary + licence zones + DCO; counsel review |
| 2 | A single upstream copyright holder objects to GPL code in an iOS binary (VLC precedent) | Med | High | No third-party GPL in the mobile binary, ever (CI-enforced) |
| 3 | LGPL deps (rawler, lensfun, libheif) in an iOS binary | Med | Med-High | Replace/avoid on iOS; LibRaw under CDDL; platform codecs |
| 4 | wgpu per-device bugs on Android (Mali/Adreno; v30 regression), silent Null backend | High | High | Pin versions, device matrix, crash guard, CPU fallback, allowlist |
| 5 | 50-100 MP files exceed phone memory/limits | High | High | Proxy editing, streaming tile export, mobile floor limits, `ResourceBudget` |
| 6 | iPhone cannot run GPU work in the background; exports die on app switch | High | Med | CPU path for background, `BGContinuedProcessingTask` for foreground-initiated work, resumable tiles |
| 7 | Thermal throttling makes sustained export slow | High | Med | Thermal-aware tile size/threads, ADPF/`thermalState` hooks, pause/resume |
| 8 | Tauri mobile gaps (folder picker, native viewport, plugins) | High | Med | Native shells over UniFFI; Tauri mobile only as prototype |
| 9 | Colour-management mismatch (macOS layer colour-space bug, Windows ACM, Wayland client gap) | Med | High | Own output transform, per-OS `PresentTarget`, ACM test matrix, visual calibration targets |
| 10 | HDR output stack divergent (DX12 scRGB not listed in wgpu docs; Linux HDR immature) | Med | Med | Phase 4; DXGI patch layer; feature-detect |
| 11 | Android file access model (no in-place folder library) degrades UX | High | Med | `AssetRef`/`ByteSource`, SAF tree grants, import-by-copy fallback |
| 12 | ORT on mobile: NNAPI deprecated, partial EP coverage, binary size, 16 KB alignment | Med | Med | Reduced-op build, int8 models, Core ML / LiteRT alternatives behind the trait |
| 13 | Sync conflicts and corrupted sidecars via dumb folder sync; SQLite in a synced folder | Med | High | Per-device op-log, atomic writes, device-local catalog, conflict snapshots |
| 14 | Platform raw decoders give a different look (RAW 9 changes with OS) | Med | Med | Own pipeline as the default; platform decoder only as labelled optional source |
| 15 | Google developer verification and F-Droid friction (2026-2027) | Med | Med | Early registration; Play + F-Droid + GitHub APK channels |
| 16 | Apple store terms/EU-JP rules shift again | Med | Med | Keep the licence split so no channel is blocked; review each submission |
| 17 | Team capacity: two mobile UI codebases | Med | Med | Small v1 UI; Compose Multiplatform as a fallback to one UI; defer to Phase 5 |
| 18 | Clean-room baselines cost more than estimated | Med | Med | Start with trivial baselines; upgrade before mobile GA; desktop unaffected |

---

## 10. Open questions for the owner

1. Approve Option E (MPL-2.0 core + GPL-3.0-or-later packs, DCO)? Needed **before the first external PR**.
2. Is a *store-distributed* iOS app a requirement, or is "Android + EU/JP alt-distribution" acceptable? (Changes how much clean-room baseline work is urgent.)
3. Fund counsel for: CC-BY-SA DB vs store DRM, Apple EULA 2026 text, CDDL venue clause.
4. Pre-register Android package/identity and the Apple Developer account under the fiscal host?
5. OK to add a check-only mobile cross-compile CI job and the `MOBILE_FLOOR` golden run in Phase 0-1 (small cost, large optionality)?

## 11. Source list (selected; full URLs inline above)

- RapidRAW clone: `src-tauri/src/android_integration.rs`, `Cargo.toml`, `build.rs`, `lib.rs`, `gpu_processing.rs`, `file_management.rs`, `gen/android/*`, `.github/workflows/build.yml`, `README.md` changelog.
- wgpu: https://docs.rs/wgpu/latest/wgpu/enum.SurfaceColorSpace.html, https://github.com/gfx-rs/wgpu/issues/10263, https://github.com/gfx-rs/wgpu/issues/10013, https://github.com/gfx-rs/wgpu/releases
- Wayland colour management: https://wayland.app/protocols/color-management-v1, https://www.phoronix.com/news/Wayland-CM-HDR-Merged
- Tauri: https://v2.tauri.app/blog/tauri-20/, https://github.com/tauri-apps/tauri/issues/14587, https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html
- UniFFI/native shells: https://github.com/mozilla/uniffi-rs, https://github.com/matrix-org/matrix-rust-sdk, https://1password.com/blog/1password-8-the-story-so-far
- Slint: https://slint.dev/blog/slint-1.17-released; Flutter/wgpu: https://github.com/flowsai/flutter_wgpu_texture/
- Licensing: https://www.fsf.org/blogs/licensing/vlc-enforcement, https://jbkempf.com/blog/How-to-properly-relicense-a-large-open-source-project-part-3/, https://signal.org/blog/license-update/, https://nextcloud.com/blog/nextcloud-ios-app-free-for-download-on-app-store/, https://www.qt.io/licensing/open-source-lgpl-obligations, https://www.libraw.org/about
- App-store regimes: https://www.macrumors.com/2026/08/18/eu-app-store-fee-change/, https://www.apple.com/newsroom/2025/12/apple-announces-changes-to-ios-in-japan/
- Platform: https://developer.apple.com/videos/play/wwdc2026/305/, https://developer.android.com/ndk/guides/neuralnetworks, https://developer.android.com/develop/background-work/services/fgs/timeout, https://onnxruntime.ai/docs/tutorials/mobile/

### Caveats on this document

Many `[verified (secondary)]` items rest on search-result summaries rather than a full read of the page; re-read the primary page before an ADR cites it. Hardware limit figures (§2.1) are device-specific samples, not guarantees. Effort estimates and the `.fbproxy` format are proposals, not measurements.
