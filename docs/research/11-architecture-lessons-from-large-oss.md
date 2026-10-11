# 11 - Architecture Lessons from Large Open-Source Projects

Status: research draft, 2026-10-03. Companion to [06](06-tech-stack-options.md) (stack options), [08](08-cross-platform-and-mobile.md) (platforms, licensing) and [SCOPE](../SCOPE.md) §5, §6, §9. The project owner asked: *how are big projects like this structured; what is best for frameworks and pipelines; what languages; VLC is a major efficient project to look towards; do we write in Rust?*

> **Lead reviewer's note (2026-10-03).** This study was written while SCOPE still planned a future iOS app and an MPL-2.0 core with "gpl-pack" desktop crates. The owner has since **dropped mobile** and SCOPE now recommends **plain GPL-3.0-or-later**. So:
> - Ignore the MPL/GPL zone rules, the `gpl-pack` split, the static-plugin-for-iOS reasoning and the UniFFI items. Item 6 of §4.10 ("stale licence line") is also obsolete: plain GPL is now intended.
> - Everything else stands and has been adopted into SCOPE §9:
>   - the Rust verdict
>   - the crate split
>   - the capability/priority registry
>   - the node-on-tile `Backend` trait
>   - parser fuzzing and decoder sandboxing
>   - deferring runtime plugins
>   - cargo-deny and toolchain pinning
>   - the funding reality check


**Evidence labels.** `[verified: URL]` = read in a source during this pass. `[repo]` = read in a shallow clone under `scratchpad/arch/` (layout and build files only; clone date 2026-10-03, so it is the tip of each default branch on that day). `[background]` = engineering knowledge, re-check before it drives a decision. `[unverified]` = could not confirm, or our own estimate/proposal. Search-result summaries without a page read are marked "(secondary)".

**Reading order.** §0 is the answer. §1 holds the case studies. §2 the cross-cutting patterns. §3 the language decision. §4 the recommended structure and the list of changes to SCOPE/06 (§4.10). §5 is the verification log.

---

## 0. Executive answer

**Short answers to the owner's questions.**

1. **How are big projects like this structured?** Almost all of them are a *small stable core library* plus *many replaceable parts*, with the product being one of several front-ends over that core: VLC (libvlccore + ~500 modules + Qt/Swift/Kotlin apps), mpv (libmpv), OBS (libobs + 42 plugin dirs + Qt frontend), FFmpeg (libav* + CLIs), Rerun (119 crates, SDKs in 3 languages, native and Wasm viewers), darktable (`libdarktable` + GUI + CLI + MCP server). The projects that suffered most coupled the engine to a UI toolkit (GIMP: GTK3 port took seven years; darktable: 59 pipeline files include GTK). Our SCOPE §9 already has this shape; the evidence says keep it and enforce it mechanically (§2.1, §4.1).
2. **Frameworks and pipelines?** Pipeline = versioned recipe compiled to a plan of nodes with ROI/tiling and a content-hash cache per node (darktable's proven design; GEGL's demand-driven graph; Blender's change-tagged depsgraph); selection of implementations through **capability + priority** registries (VLC's best idea); GPU through *one* abstraction behind a narrow trait (wgpu, as Ruffle/Bevy/Rerun/Zed do for non-Apple targets), with a CPU reference backend and equivalence tests (FFmpeg `checkasm`, GEGL's OpenCL lesson). Plugins: compile-time first; data plugins (presets, LUTs) next; Lua, then WASM; **never a dynamic Rust ABI** (§2.2, §2.3, §2.5, §4.2).
3. **VLC as a model.** Copy its *structure and governance*: libvlc-style small embedding API, static-capable module table, contribs for dependencies, per-platform apps in their own repos, non-profit + no ads + no CLA, a for-profit consultancy paying developers, and the licence lesson (relicensing took years, so decide the split now). Do not copy its *language* choice: VLC's tree now carries an opt-in, experimental Rust module layer (nightly toolchain, default off) while the core stays C + asm; VideoLAN's own AV1 decoder dav1d is C + 250k lines of asm, and the safe-Rust port rav1d is within about 6% of it while reusing that asm (§1.1, §1.2).
4. **Do we write in Rust?** **Yes**, for the engine, catalog, CLI, protocol and tooling. Reasons: raw/DNG/TIFF/MakerNote parsers are the attack surface (LibRaw has 60 CVE IDs incl. 2026 overflows in DNG and X3F decoding; Apple's DNG decoder CVE-2025-43300 was exploited in the wild), wgpu is Rust-native, and Cargo makes the headless-library-plus-CLI structure cheap. Performance parity is demonstrated; SIMD on stable needs a crate (`std::simd` is still nightly). Conditions and the places where C++ is better (Qt glue, Halide kernels, OCIO/OIIO, forks of RawTherapee code) are in §3.10.
5. **Funding.** The NLnet NGI Zero Commons Fund is **closed** (final call 1 June 2026); NLnet's open calls (Restack, CodeSupply) have a **3 Nov 2026** deadline and a doubtful fit for a RAW editor; the Sovereign Tech Fund explicitly does not fund user-facing applications. Fiscal host first, project-specific small grants second, library-first positioning for the big funders (§2.13, §4.9).
6. **What to change in our plan:** 15 items in §4.10; the five that matter most are (a) split `amb-core` into recipe / buffer / graph / nodes / cpu, (b) add a VLC-style capability+priority registry, (c) define the GPU boundary as a node-on-tile `Backend` trait, (d) sandbox LibRaw and fuzz every parser from Phase 1, (e) add a governance/funding section with the corrected grant facts and fix the stale licence line in SCOPE §12.1.

---

## 1. Case studies

### 1.1 VLC / VideoLAN (the owner's reference project)

- **Shape: a small core plus hundreds of plugins.** `libvlccore` is the plugin host and runtime (object model, threads, clocks, input/stream/decoder/output plumbing). Almost every feature is a "module" loaded by capability. Kempf describes VLC as a multimedia framework rather than just a player, with around 500 modules, and says a typical module is about 1,000 lines of code [verified: https://www.heavybit.com/library/podcasts/demuxed/ep-8-videolan-with-jean-baptiste-kempf]. `modules/` in the clone has 38 category directories (access, demux, codec, packetizer, video_filter, video_output, audio_output, stream_out, gui, ...) [repo: vlc/modules].
- **Capability + score selection.** A module declares what it is with `set_capability("video decoder", 10000)` and what it needs with callbacks (`set_callbacks(Open, Close)`); `dav1d.c` registers as a "video decoder" with score 10000 [repo: vlc/modules/codec/dav1d.c:63]. The core asks for a capability (`module_need`), tries candidates in descending score, and the first `Open` that returns success wins. User-set shortcuts (`add_shortcut`) and config can force one. This is the pattern that makes "GPU path first, software fallback second" a policy in data rather than an `if` ladder. Newer filters use typed callbacks (`set_callback_video_filter(Create)`, e.g. `sepia.c`) [repo: vlc/include/vlc_plugin.h, modules/video_filter/sepia.c].
- **Static or dynamic, same source.** `src/modules/bank.c` loads plugins from a `vlc_static_modules[]` table (static build, used for iOS and other no-dlopen targets) or from shared objects on disk, with a plugin cache (`plugins.dat`) so startup does not `dlopen` every file to read its metadata [repo: vlc/src/modules/bank.c, cache.c]. Lesson: design the plugin entry point so it can be linked statically; mobile needs it.
- **ABI is "same version only".** Plugins are built against `include/vlc_*.h` for one VLC version and are not expected to load into another; the cache string embeds the package version [repo: cache.c `CACHE_STRING`]. The stable surface is the separate, small **libvlc** C API (the one apps embed). Two APIs, two stability promises. This is the standard way to avoid freezing internals.
- **Per-platform UIs over one engine.** Desktop UI is Qt (`modules/gui/qt`, which is itself a module), with legacy macOS Obj-C and skins2 modules also in-tree [repo: vlc/modules/gui]. Mobile and other apps live in separate repos: Android app in Kotlin/Java over libvlc, iOS/tvOS app in Swift over MobileVLCKit/TVVLCKit, libVLCSharp in C# [verified: README in clone; https://www.videolan.org/vlc/releases/3.0.0.html]. VLC 3.0 was the first release synchronised between desktop and mobile ports (secondary: https://geoffreymetais.github.io/features/vlc-30/). This is the structure SCOPE §9 already proposes for Ambrotype.
- **Build system.** Autotools is still the release build; Meson is an in-tree "experimental" migration ("should not be used for release builds") that began as GSoC 2018 (about 100 modules ported then) and still coexists with autotools in 2026 [verified: https://videolan.videolan.me/vlc/master/buildsystem.html, https://epir.at/2018/08/13/gsoc-2018/]. Smaller VideoLAN libraries (libbluray, dav1d) already dropped autotools for Meson [verified: https://jbkempf.com/blog/2025/libbluray-1.4.0/ (secondary)]. Lesson: a build-system migration in a big C project takes 8+ years while both systems are maintained; start with the right one.
- **Contribs = vendored dependency builder.** `contrib/src/` has 145 per-library recipes (`rules.mak` + `SHA512SUMS`): each pins a version and tarball URL and builds with the cross-toolchain, e.g. dav1d 1.5.4 via Meson with tests and tools off [repo: vlc/contrib/src/dav1d/rules.mak]. A `need_pkg` check uses the system copy if present. CI uses the same recipes for Windows, Android and Apple targets. This is why VLC ships the same codec versions everywhere, and why it can cross-compile to 10+ targets without distro packaging.
- **Release engineering across many targets.** `extras/ci/gitlab-ci.yml` defines jobs for win32, win64, win64-arm (LLVM), UWP, Debian (autotools and Meson), Snap, Raspbian, macOS x86_64 and arm64, iOS arm64/armv7/simulator, tvOS, watchOS, visionOS (`xros`), and Android arm/arm64/x86/x86_64, plus nightlies and docs [repo: vlc/extras/ci/gitlab-ci.yml]. Packaging scripts per platform live in `extras/package/{win32,macosx,snap,...}`. VLC 3.0 (2018) listed Windows XP-10, macOS 10.7-10.13, iOS 7-11, Android 2.3-8.1, Android TV, Xbox One and BSD [verified: https://www.videolan.org/vlc/releases/3.0.0.html]. The README admits that platforms do not all get the same amount of care, because resources are limited [repo: README.md].
- **Governance.** VideoLAN is a French non-profit (association loi de 1901), constituted 21 April 2009, with Jean-Baptiste Kempf as president; origins are a 1996 student project at Ecole Centrale Paris [verified: https://en.wikipedia.org/wiki/VideoLAN]. Authors keep their copyright (no CLA or assignment) [verified: https://www.networkworld.com/article/744268/opensource-subnet-vlc-license-change-a-lesson-in-perseverance.html]. The README says "VideoLAN is not paying any of them" (the community) [repo: README.md].
- **Funding and "no ads".** Revenue from the player is donations only; Kempf has publicly refused offers to bundle toolbars, ads or tracking, with figures of "tens of millions" cited in press [secondary, not primary: https://mein-mmo.de/en/..., https://en.wikipedia.org/wiki/Jean-Baptiste_Kempf] [unverified: exact amounts]. The developers' money comes from **Videolabs** (founded 2012 by Kempf and Ludovic Fauvet), a for-profit consultancy that sells multimedia engineering to Microsoft, Cisco, Thales and others and is a major VLC contributor; about 27 employees in mid-2026 (secondary: Tracxn/Pitchbook listings in search; [unverified]). This **non-profit brand plus commercial consultancy** split is the working model for "free software with paid maintainers."
- **Licence history (the cautionary tale in SCOPE §6).** Apple pulled VLC from the iOS App Store in January 2011 after a GPL complaint by a VLC developer. VideoLAN relicensed the engine: libVLC and libVLCcore from GPLv2+ to LGPLv2.1+ (announced 21 Dec 2011; every developer agreed except one well-known contributor who never answered, so that code was rewritten), and the modules afterwards. The media player UI stays GPLv2+. The iOS app returned in July 2013 under MPL-2.0 [verified: https://www.videolan.org/press/lgpl-libvlc.html, https://archive.fosdem.org/2013/schedule/event/relicensing_vlc/; secondary: https://en.wikipedia.org/wiki/VLC_media_player]. Because there was no copyright assignment, every author had to be found and asked. SCOPE's "about 2.5 years and about 230 contributors" figure is from earlier research and was not re-verified here [unverified]. Direct lesson for us: choose the split licence and a DCO **now**, because relicensing later is a multi-year project.
- **Performance culture.** Hand-written assembly is first-class: the clone has `.asm`/`.S` under `modules/isa/{arm,aarch64}` (NEON, SVE) and x86 `yadif_x86.asm` [repo]. Hardware decode is default since 3.0 (DXVA2/D3D11, VideoToolbox, MediaCodec, VA-API) [verified: releases/3.0.0.html]. VideoLAN also hosts dav1d, x264 and x265 (see 1.2). The culture can be summed up as a C core, assembly in the hot loops, and a C fallback that is always present.
- **Rust inside VLC (answers "what would VLC choose today").** VLC master (4.0.0 workspace) already has an opt-in Rust layer: a Cargo workspace with `vlcrs-core`, `vlcrs-macros`, `vlcrs-messages`, `vlcrs-plugin`, `vlcrs-utils`, a sys-binding generator, plus one Rust module (`modules/logger/telegraf-rs`), `rust-toolchain.toml` pinned to **nightly**, and a Meson option `rust` described as "Experimental pure Rust VLC modules", default `disabled` [repo: vlc/Cargo.toml, rust-toolchain.toml, meson_options.txt:73, src/rust]. About 3,100 lines of Rust exist [repo, `wc -l`]. The README now says the repo also contains a lot of C++, Obj-C, assembly and Rust [repo]. So the project that maintains the best C+asm codebase in the field chose: **core stays C; new modules may be Rust over the same plugin ABI; opt-in, experimental.** That is "incremental oxidation through the plugin boundary," and it only works because VLC has a plugin boundary. I found no Kempf statement committing VLC to a Rust rewrite [unverified; searched].

### 1.2 FFmpeg and dav1d (performance-critical C plus assembly)

- **FFmpeg layout.** A set of libraries (`libavutil`, `libavcodec`, `libavformat`, `libavfilter`, `libswscale`, `libswresample`, `libavdevice`) plus thin CLIs in `fftools/` (`ffmpeg`, `ffplay`, `ffprobe`). The CLI is a client of the libraries, the same "headless library plus CLI" shape as our plan [repo: FFmpeg/ top level, fftools/].
- **Plugin selection is compile-time.** Codecs, demuxers and filters are registered in static tables and enabled by `configure` flags; there is no runtime plugin ABI. `configure` + `ffbuild` (hand-written shell and make, not CMake or Meson) [repo]. Lesson: a big, fast project can live with compile-time modules; runtime plugins are not required for extensibility.
- **Asm discipline.** FFmpeg writes SIMD as hand-written assembly (x86 via `x86inc`/NASM, Arm NEON) and does not use intrinsics; every asm function has a C reference, and `tests/checkasm` verifies asm against C, with a rule that new asm must come with a checkasm test [repo: libavutil/x86/*.asm, tests/checkasm; verified: https://ffmpeg.org/doxygen/trunk/checkasm_8h_source.html (secondary)]. FFmpeg claims hand asm is around 10-15% faster than intrinsics (secondary: https://news.ycombinator.com/item?id=43140614 [unverified]). The structural pattern (scalar reference, optimised variants, equivalence test per function) is what our CPU/GPU equivalence suite should copy.
- **Test suite.** FATE (`tests/fate`, `tests/ref`, 119 fate makefiles in the clone) compares decoder output checksums against stored reference for every codec, sample files hosted separately [repo]. This is golden-output testing at scale.
- **Fuzzing and funding.** OSS-Fuzz has reported over 400 bugs for ffmpeg; one maintainer tweeted fixing over 2,700 OSS-Fuzz issues (secondary: https://thenewstack.io/ffmpeg-to-google-fund-us-or-stop-sending-bugs/). Germany's Sovereign Tech Fund granted FFmpeg EUR 157,580 for 2024-2025 and reported Coverity defect density falling to under 1/30 of the average for OSS projects over a million lines (secondary: https://www.phoronix.com/news/Germany-STF-For-FFmpeg, https://linuxiac.com/sovereign-tech-fund-backs-ffmpeg-project/). `FUNDING.json` is in the repo root [repo]. FFmpeg also publicly pushed back on large companies sending AI-found bugs without funding fixes (secondary, same New Stack link).
- **dav1d goals (AV1 decoder, 2018).** The stated aims were a small, very fast, very portable, correctly threaded, free-software decoder. It uses C99 plus hand asm (NASM/GAS) with no intrinsics, Meson/ninja, BSD-2 licence chosen deliberately so even closed-source software can use AV1; roughly 1/10 the lines of libaom, 1/3 the binary and 1/4 the memory; funded by the Alliance for Open Media; built jointly by VideoLAN and FFmpeg developers [verified: https://jbkempf.com/blog/Introducing-dav1d/].
- **dav1d size and cost of speed.** About 50k lines of C and 250k lines of assembly after six years [verified: https://www.memorysafety.org/blog/porting-c-to-rust-for-av1/]. Five times more asm than C: that is the "performance culture" in numbers. Threading (frame and tile threads) is part of the design, not an add-on [background].
- **dav1d vs Rust: the best available datapoint for our language question.** Prossimo/ISRG commissioned **rav1d**, a C2Rust transpile of dav1d refactored to safe Rust, **reusing the same hand-written asm**. Result: 3.8% slower right after transpile (bounds checks under 1% of that), improved to "under 6%" slower than C on x86_64 in the later, safer state, with Rust still "almost exactly half of the total run time" even with all asm enabled; cost was 20+ person-months (estimated 7) for 3 developers; funded by AWS, the Sovereign Tech Fund and Alpha-Omega [verified: https://www.memorysafety.org/blog/rav1d-performance-optimization/ (10 Sep 2024), https://www.memorysafety.org/blog/porting-c-to-rust-for-av1/ (9 Sep 2024)]. Reading: Rust gets within single-digit percent of hand-tuned C while keeping the asm; the asm, not the host language, delivers the speed; safe-Rust threading needed a custom `DisjointMut` type because the original shared mutable data across workers.
- **Lesson for us.** Our hot loops are image kernels, not entropy decoders: they are embarrassingly parallel, well suited to auto-vectorisation, `std::arch` intrinsics and GPU compute, and we will not have 250k lines of asm. The rav1d gap is irrelevant to a project that has no asm budget and a GPU path.

### 1.3 Blender (C/C++ core, Python API, GPU abstraction, foundation-funded)

- **Code layout is three tiers.** `source/` (application, with `source/blender/*` modules), `intern/` (Blender-owned libraries kept self-contained: Cycles, GHOST, physics solvers), `extern/` (vendored third-party). Core is C with C++ for heavy libraries and Python for scripting and many UI definitions; CMake plus a "lib" repo of prebuilt dependencies [verified: https://developer.blender.org/docs/features/code_layout/]. The `intern/` rule (a library that could in principle be reused by others, with no dependency on the rest) is a cheap way to enforce module boundaries.
- **GHOST** ("Generic Handy Operating System Toolkit") is Blender's own windowing/input/GL-context layer for Windows, macOS, X11 and Wayland, not SDL or GLFW [verified: same page]. Lesson: a big editor ends up owning its platform shell; for us that role belongs to the replaceable UI shell, not the engine.
- **GPU module = in-house abstraction.** One `GPU` API (batches, shaders, textures, frame buffers) with three backends: OpenGL (legacy), Metal (shipped in 3.5, early 2023), and Vulkan (announced as live in daily builds Oct 2023, still the "future default" in 2025-26) [verified: https://developer.blender.org/docs/features/gpu/, https://code.blender.org/2023/01/introducing-the-blender-metal-viewport/; secondary: https://www.cgchannel.com/2023/10/blenders-new-vulkan-rendering-backend-has-gone-live/]. Shaders are written once in a Blender shading dialect (GLSL-based, with a preprocessor, "BSL") and translated to GLSL, MSL and SPIR-V; the docs say the custom compiler avoids "performance pitfalls of vendor shader compilers" [verified: GPU docs page].
- **Backend migration is slow, and the schedule slips.** Vulkan was meant to replace OpenGL in 4.x; for 5.0 (Nov 2025) it was available but not default because tests failed on Intel drivers, and 5.1 (March 2026) was the next target; whether 5.1 or a later version made it the default is not verified here [verified: https://www.phoronix.com/news/Blender-5.1-Vulkan-Default; [unverified]: final status]. Lesson for us: even with a dedicated GPU team, a second/third backend takes 2-3 years to reach parity; a single portable layer (wgpu) delegates that cost to someone else, but we then inherit its limits (see §3).
- **Cycles and EEVEE are two render engines behind one render-engine API.** Cycles (path tracer, own `intern/cycles` tree, device abstraction for CPU, CUDA, OptiX, HIP, oneAPI, Metal) and EEVEE (real-time, built on the GPU module) plug in as engines selected per scene, with Python-registered engines also possible [verified: code_layout page; background for device list]. This is the capability-selection pattern again: a stable "engine" interface, multiple implementations, chosen by the user.
- **Depsgraph** is a dependency graph that tracks relations between data-blocks (objects, modifiers, materials), tags what changed, and evaluates the affected subgraph on worker threads, working on an evaluated copy of the original data ("copy on write") so editing and evaluation can overlap [verified: code_layout page names it; mechanism details are background]. Same idea as our recipe-to-pipeline graph with change tagging and per-node hash caches.
- **Python API as the extension surface.** The whole UI is defined in Python (`bpy` types) and add-ons use the same API; the cost is that the API is not stable across major versions: point releases are mostly compatible, but 4.x to 5.0 removed deprecated calls and broke many add-ons [secondary: https://developer.blender.org/docs/release_notes/5.0/python_api/, https://developer.blender.org/docs/release_notes/compatibility/]. The compensating structure is the Extensions Platform (extensions.blender.org) with moderation guidelines, hosting add-ons outside the core repo [secondary: https://developer.blender.org/docs/features/extensions/moderation/guidelines/].
- **Release train.** Three releases per year (roughly every four months) plus a yearly **LTS**, which gets two years of bug-fix-only monthly updates and no API changes; e.g. 4.5 LTS (29 Jul 2025) is supported until July 2027 [secondary: https://www.blender.org/download/lts/, https://digitalproduction.com/2025/11/03/blender-4-5-lts-lands/]. Studios depend on LTS; this is the policy to copy once we have users with pipelines.
- **Governance and money.** Blender Foundation (Dutch non-profit, Ton Roosendaal) plus Blender Studio and Blender Institute; engineering is funded mainly through the **Development Fund** (fund.blender.org): individual donors plus corporate memberships at fixed tiers (Platinum EUR 60k/yr, Titanium EUR 120k/yr, Patron EUR 240k/yr). Reports put 2024 revenue near EUR 3.1M with corporate members about 40% and 15+ full-time developers and support staff; Epic Games, Qualcomm and Pico listed at Patron level, and AMD, NVIDIA, Intel at Titanium or above (secondary: https://fund.blender.org/corporate-memberships/, https://www.cgchannel.com/2025/08/wacom-becomes-corporate-patron-of-the-blender-development-fund/) [unverified: exact revenue].
- **Lesson.** Blender is the closest example of "a creative tool that became infrastructure": fixed cadence, LTS for pipelines, in-house GPU abstraction, tiered corporate patronage that buys liaison and listing but not control. It is also GPL, with a C/C++ core and a 30-year legacy; that is not a model for language choice, only for process.

### 1.4 OBS Studio (libobs core, plugins, Qt front-end)

- **Shape: `libobs` + graphics backends + plugins + `frontend`.** Top-level directories: `libobs/` (core: sources, outputs, encoders, audio/video pipelines, hotkeys, data/settings), `libobs-opengl`, `libobs-d3d11`, `libobs-metal`, `libobs-winrt` (graphics backends), `plugins/` (42 directories), and `frontend/` (the Qt app) [repo: obs-studio/]. The UI used to be inside the repo as `UI/` and was renamed `frontend/`, making the library/app split explicit [background; repo shows `frontend`].
- **Plugins are shared libraries with a registered-struct API.** A plugin exports `obs_module_load` (declared via `OBS_DECLARE_MODULE()`) and calls `obs_register_source(&info)`; an `obs_source_info` struct holds an id, a type (input/filter/transition), flags and callbacks for create/destroy/render/properties [repo: libobs/obs-module.h, obs-source.h]. Registration passes `sizeof(struct obs_source_info)` (`obs_register_source_s`), so the struct can **grow without breaking old plugins**: the core knows how much of it the plugin filled in. That is the simplest workable C ABI evolution trick and worth copying if we ever expose a C plugin ABI.
- **API version number.** `LIBOBS_API_MAJOR_VER` is 33 (minor 0) in the current tree; plugins check it at load [repo: libobs/obs-config.h]. Major bumps in OBS have repeatedly required third-party plugin rebuilds, a recurring community pain point [background].
- **Graphics: one API, runtime-loaded backends, own shader language.** The core talks to `gs_*` calls (`libobs/graphics`), backends are selected at startup, and shaders are written in an HLSL-like `.effect` language parsed by `effect-parser.c` and translated per backend [repo: libobs/graphics/effect-parser.c; background for HLSL-likeness]. A third in-house GPU abstraction (after Blender and mpv's `ra`/libplacebo path) built because wgpu did not exist.
- **Capture backends are plugins by platform.** `linux-pipewire`, `linux-v4l2`, `linux-capture` (X11), `mac-capture`, `mac-avcapture`, `mac-syphon`, `win-capture`, `win-dshow`, `win-wasapi`, plus encoders (`obs-nvenc`, `obs-qsv11`, `mac-videotoolbox`, `obs-ffmpeg`, `obs-x264`) [repo: plugins/]. All capture complexity (OS APIs that change yearly) is quarantined behind the source interface, so a breakage in one OS API affects one directory. We have the same need for platform decoders (ImageIO, WIC, Android) and display-profile lookup.
- **Frontend API for plugins.** `frontend/api/obs-frontend-api.h` exposes events and calls (`obs_frontend_*`) so plugins can add docks and menu items without linking Qt [repo: frontend/api]. Separate the plugin-to-core API from the plugin-to-UI API.
- **Build and CI.** CMake 3.28+ with `CMakePresets.json` presets for macOS, Ubuntu, Windows x64 and Windows ARM64 (plus "-ci" variants); GitHub Actions workflows for build, format-check (clang-format), static analysis, Windows signing, PR pulls, scheduled and publish [repo: CMakePresets.json, .github/workflows/]. Dependencies are pre-built by a separate `obs-deps` repo and downloaded by the presets (`dependencies` preset) [repo: CMakePresets.json; background].
- **Governance and money.** The OBS Project is run by a small core team; funding is via Open Collective, Patreon and GitHub Sponsors and corporate sponsors (NVIDIA, Twitch and others) [background; [unverified]: current sponsor list]. OBS Project was set up as a fiscal-hosted project, not a foundation with staff [unverified].
- **Lesson.** OBS proves a C plugin ABI with a Qt UI scales to an ecosystem of hundreds of third-party plugins, and also shows the cost: C ABI + shared libs + 3 OS toolchains means constant plugin rebuilds. If we do plugins, prefer data (recipe JSON, LUTs, Lua) first, WASM second.

### 1.5 Krita and GIMP/GEGL/babl (node-based image graphs, colour, and a UI-toolkit cautionary tale)

- **GEGL is a graph processing library, separate from GIMP.** Directed acyclic graph of operation nodes, demand-driven and cached, with a tiled `GeglBuffer` (`gegl/buffer/*` in the clone), operations as plugins (`operations/`: `common` (129 files), `common-gpl3+` (51 files), `common-cxx`, `core`, `external`, `opencl`, `transform`, `workshop`) [repo: gegl/]. Library is LGPL-3+; the GPL-3+ operations live in their own directory with GPL headers [repo: gegl/operations/common-gpl3+/*.c, gegl/COPYING.LESSER]. **This is exactly the MPL-core plus GPL-pack split we planned, implemented as a folder and a build switch inside one repo, and it has run for years.** Copy the directory naming convention.
- **babl is the pixel-format and colour-conversion library.** It describes pixel formats (model + component type + space) and builds conversion "fish" paths between them, with CPU-specific extensions (`extensions/avx2-int8.c`, `fast-float.c`, `half.c`, `HCY.c`, `CIE.c`) and an ICC reader (`babl-icc.c`) [repo: babl/babl, babl/extensions]. Result: GIMP 3.0 edits in linear float and any ICC RGB space [secondary: https://www.gimp.org/release-notes/gimp-3.0.html]. Lesson: make pixel format and colour space part of the buffer type from day one, in a small crate with its own tests.
- **OpenCL in GEGL stayed second-class.** There is an `operations/opencl` directory of `.cl` kernels alongside CPU operations, i.e. two implementations per operation with CPU as the truth; GIMP never made GPU processing the default path [repo: gegl/operations/opencl; background]. This is the same dual-backend risk as ours: the second backend is where the maintenance cost and divergence bugs live. It is why the CPU/GPU equivalence test in SCOPE §9 is non-optional.
- **GIMP 3.0 is the GTK3 cautionary tale.** GIMP 2.10 shipped in 2018, 3.0 development releases began in 2020, and 3.0.0 shipped on 16 March 2025 after seven years, with the GTK3 port as the central effort (removing all custom GTK2 extensions, planning for GTK4, Python 2 to 3) [verified: https://www.gimp.org/news/2025/03/16/gimp-3-0-released/, https://www.phoronix.com/news/GIMP-GTK3-Finished]. GTK4 already exists, so GIMP is a toolkit generation behind on release day. The UI and the engine (GEGL, babl) were separate libraries and kept shipping improvements during the port; the *application* is what stalled. Lesson: the engine must not be blocked by a UI migration, which is the whole point of our replaceable-shell rule, and we should expect to rewrite the shell at least once in the project's life.
- **Krita is a Qt/C++ app with a tile-based paint device and a node tree.** `KisImage` is the model (a tree of `KisNode`: layers, masks), `KisPaintDevice` is tiled storage that knows its pixel size, the undo system is split (`KisUndoStore` and an adapter) and a "strokes queue" runs painting in worker threads without blocking the UI; colour management through LittleCMS with per-device colour spaces; scripting via embedded Python (PyQt) with a documented Python API, and the Krita 5 to 6 move (Qt5 to Qt6, PyQt5 to PyQt6) breaks Python plugins [secondary: https://deepwiki.com/KDE/krita, https://docs.krita.org/en/untranslatable_pages/strokes_documentation.html, https://krita-artists.org/t/how-does-one-keeps-plugin-supported-for-krita-5-and-6-advice-needed/176705]. Lesson: the toolkit choice leaks into the scripting API; a plugin surface that exposes your UI toolkit has to be re-broken at every toolkit upgrade.
- **Colour management is a library concern in both.** GIMP delegates to babl (formats, ICC reading) and GEGL (linear-light float processing); Krita attaches a colour space to every paint device and uses LittleCMS; neither mixes colour logic into UI code [repo: babl/babl/babl-icc.c; secondary: deepwiki Krita pages]. Our `amb-color` should be a crate with its own tests and no UI, pipeline or GPU dependency.
- **Scripting differs by project.** GIMP 3.0 moved plug-ins to a GObject-introspection-based API with Python 3, Script-Fu and others; Krita embeds PyQt Python [secondary: https://www.gimp.org/release-notes/gimp-3.0.html, krita docs]. Both pay the toolkit-coupling tax in their scripting API. Prefer scripting that talks to the engine protocol, not to the UI toolkit.
- **Krita funding.** Stichting Krita Foundation (Netherlands) funds a few full-time developers through the **Krita Development Fund** (monthly subscribers), donations, sales on Steam/Microsoft Store/Epic and Android stores, and KDE e.V. as the project umbrella; the Foundation supports two volunteers financially under Dutch tax law [secondary: https://krita.org/en/posts/2021/join-the-new-development-fund/, https://fund.krita.org/, https://krita.org/en/donations/]. A key point is that **selling the same free binary in app stores** funds development without relicensing; this is relevant to our iOS/Android plan.

### 1.6 darktable (the pixelpipe reference, and GUI coupling)

- **Pixelpipe = ordered list of image-operation (IOP) modules, each with a fixed C API.** 97 `.c` IOP files under `src/iop` (about 110 entries including `.cc`, helpers and subdirectories) plus 60+ OpenCL `.cl` kernels in `data/kernels`. The module API (`iop_api.h`) has `commit_params`, `process`, `process_cl`, `process_tiling_cl`, `modify_roi_in`, `modify_roi_out`, `distort_mask` and an `introspection_init` for automatically generated parameter UIs [repo: darktable/src/iop/iop_api.h, src/develop/pixelpipe_hb.c, data/kernels].
- **ROI and tiling are first-class in the node API.** Each module can tell the pipe how its output region maps to input (`modify_roi_in/out`), and there is a generic `tiling.c` when an image does not fit in memory or GPU memory [repo: src/develop/tiling.c, iop_api.h]. This is the same "ROI, tiling, per-node hash caching" design as SCOPE §9; the darktable version is the one proven for 100 MP raws.
- **Cache keyed by a hash of the module stack and ROI.** `dt_dev_pixelpipe_cache_hash(roi, ...)` returns a hash covering every module in the stack up to the current one, plus the region of interest, and a code comment warns that correctness depends on the hash, so everything relevant must go into it [repo: src/develop/pixelpipe_cache.h:30,66]. Build content-addressed per-node caches.
- **Params vs data.** `params_t` (what is stored in history/XMP, versioned, stable on disk) is separate from `data_t` (committed, runtime-optimised form built in `commit_params`); the in-tree docs spell this out ("a stack stays valid across module versions") [repo: dev-doc/IOP_Module_API.md, RELEASE_NOTES.md]. This is the recipe-versus-pipeline split we intend; they discovered it, and kept param structs versioned with explicit migration functions.
- **GUI coupling is real.** 59 files in `src/develop` and `src/iop` include GTK headers, because each module owns its GUI code (`gui_init`, bauhaus sliders) in the same file as the pixel code, and `pixelpipe_hb.c` itself includes `<gtk/gtk.h>`; the build requires GTK 3.24.15+ even for `darktable-cli` (which links `lib_darktable`) [repo: grep over src/develop and src/iop; src/CMakeLists.txt:325]. darktable is still GTK3 in 2026, with a GTK4 port a recurring discussion [repo: CMake; [unverified] for the porting status]. The `docs/GUI_Threading.md` file exists because pipe threads and GTK thread share `gui_data` [repo: dev-doc/GUI_Threading.md]. Lesson: put the parameter schema (names, ranges, defaults, groups) in data that the engine owns and the UI reads; do not co-locate widgets with kernels.
- **Lua scripting** (`src/lua`, `docs` for the Lua API): event hooks, image/film/style objects, custom storage targets; no sandbox beyond Lua's own [repo: src/lua; background]. A cheap, popular extensibility layer that does not require ABI work.
- **Lightroom import already exists in-tree** at `src/develop/lightroom.c` (the ~50 imported XMP tags our SCOPE §1 cites) [repo]. **The `darktable-cli` binary and a new `darktable-mcp` JSON-RPC server (`src/mcp`) both link `libdarktable` directly**, which gives them real module introspection, the in-process pixelpipe and native styles and history, with no hand-editing of XMP [repo: src/cli, src/mcp/README.md]. A headless library with several thin front-ends (GUI, CLI, MCP) is the structure we want; darktable arrived at it late and with GTK still entangled.
- **Contributor process artefacts.** In-tree `dev-doc/` (IOP API, pixelpipe architecture, GUI threading), `AGENTS.md`/`CLAUDE.md` for AI-assisted contributions ("every commit must compile on its own" for `git bisect`), release notes, a Doxygen/wiki developer guide [repo]. Releases twice a year (feature releases in June and December, 5.8 is the current tree) [repo: RELEASE_NOTES.md; background for cadence].

### 1.7 mpv / libmpv (embeddable core, thin front-ends)

- **One player core, many front-ends.** mpv ships the CLI/OSC player and `libmpv` (client API in `include/mpv/client.h`, render API in `render.h`); third-party front-ends (IINA on macOS, mpv.net on Windows, Celluloid on GTK, and many Android/iOS wrappers) embed libmpv rather than forking the player [repo: include/mpv; background for front-end names]. This is the cleanest example of a product that is a library, where the UI is someone else's problem.
- **Narrow, string-friendly API.** Commands, properties and options are addressed by name and exchanged as typed nodes or strings; `client.h` tells embedders to prefer `MPV_FORMAT_STRING` to decouple from internal changes [repo: client.h:213-230]. The client API is semantically versioned (`MPV_CLIENT_API_VERSION` 2.5; major bump on C-API breakage, minor on additions) and every change is logged in `DOCS/client-api-changes.rst` and `interface-changes.rst` [repo]. Our JSON command protocol should be name-based like this.
- **Render API instead of owning the window.** An embedder creates an `mpv_render_context` (OpenGL, or a software renderer) and asks it to draw into the host's framebuffer, with an update callback; so libmpv never needs to know the host toolkit [repo: render.h:32-66]. Direct analogue: our engine renders into a host-supplied wgpu texture or CPU buffer.
- **GPU path moved out of tree.** `video/out/gpu` (the old internal `ra` render abstraction) coexists with `gpu_next`, which delegates to **libplacebo** (a separate project by an mpv developer) via `meson.build` (`libplacebo >= 7.360.1`) [repo: video/out/gpu_next, meson.build:30]. Lesson: shader/colour pipelines shared with other projects (libplacebo is also used by VLC and FFmpeg) get better faster than a private one; consider "upstream the generic parts" for ours.
- **Licensing switch.** Default build is GPL-2+; Meson option `-Dgpl=false` produces an LGPL-2.1+ build with GPL-only code (some filters, subtitle bits) compiled out [repo: meson.options:2]. Also a build-time licence split, like GEGL's.
- **Scripting.** Lua and JavaScript scripts shipped in-tree (`player/lua`: OSC, console, context menu, `player/javascript`), same property/command API as external clients, and a JSON IPC socket [repo: player/lua; background for IPC]. The built-in UI (OSC) is itself a script, which keeps the core free of widget code.
- **Ecosystem of front-ends proves the boundary.** Because libmpv is stable and render-API based, independent GUIs exist for macOS (IINA), Windows (mpv.net), GTK (Celluloid) and mobile wrappers, all maintained by other people [background; [unverified] list]. Our CLI and C ABI should make a third-party GUI possible without forking, even if we never see one.
- **Docs for embedders are first-class.** `DOCS/` carries `compatibility.rst`, `client-api-changes.rst`, `interface-changes.rst`, `contribute.md`, `release-policy.md`, `tech-overview.txt` [repo: mpv/DOCS]. Every user-visible rename is logged; that discipline is what lets embedders upgrade.
- **Fuzzers and CI.** `fuzzers/` plus `ci/` scripts for mingw64, msys2, macOS, FreeBSD, OpenBSD, Tumbleweed, old Linux [repo: mpv/ci, mpv/fuzzers]. Governance is a loose volunteer group, no foundation; releases are irregular and a `release-policy.md` documents when [repo: DOCS/release-policy.md; background].

### 1.8 Rust-heavy large projects

**Zed (editor, GPUI).**
- Workspace of **249 crates**, one product (the editor, GPL-3.0-or-later) and a UI framework (`gpui`, Apache-2.0) that is separately licensed in the same repo; platform backends are separate crates: `gpui_macos` (Metal), `gpui_windows`, `gpui_linux`, `gpui_wgpu` (WGSL shaders `shaders.wgsl`, `shaders_webgl.wgsl`) and `gpui_web` [repo: zed/Cargo.toml, crates/gpui*, crates/zed/Cargo.toml:7, crates/gpui/Cargo.toml:9]. The in-tree renderer is moving to **wgpu 29** for non-Apple platforms (and WebGL), which is a strong sign that a serious product team chose wgpu over writing per-API backends [repo: Cargo.toml:942, crates/gpui_wgpu].
- **Extensions are WASM components.** `rust-toolchain.toml` lists `wasm32-wasip2` ("extensions"), `wasmtime 48` and `wasmtime-wasi` are workspace dependencies, and `crates/extension_api` ships a WIT interface directory [repo: rust-toolchain.toml, Cargo.toml:925-937, crates/extension_api/wit]. This is the best precedent for our "WASM plugin ABI later": a large Rust product already runs untrusted third-party code in wasmtime with a WIT-defined host API.
- GPU-rendered UI means Zed owns text shaping, a11y, IME and input per platform; that cost (GPUI has `_accessibility.rs`, `gestures.rs` and `cosmic_text_system.rs`) is why we should not follow it for panels (§3.7).

**Firefox: Stylo and WebRender (incremental oxidation via FFI).**
- Mozilla replaced the CSS style system with **Stylo** (from Servo): about 160,000 lines of C++ replaced by about 85,000 lines of Rust, running style computation in parallel with work-stealing for speed-ups "up to 2 or 4 or even 18 times" on suitable pages [secondary: search summary; primary: https://hacks.mozilla.org/2017/08/inside-a-super-fast-css-engine-quantum-css-aka-stylo/; https://blog.rust-lang.org/2017/11/14/Fearless-Concurrency-In-Firefox-Quantum/]. Two earlier attempts to parallelise styling in C++ had failed [background; stated in the Quantum posts, not re-read].
- **WebRender** is a GPU-based 2D renderer (originally a Servo research project) shipped in Firefox, with reported rendering speed-ups (about 30% in one search summary) [secondary, unverified].
- About 11.7% of Firefox's source was Rust, down from a 12.5% peak in 2020 (secondary: https://news.ycombinator.com/item?id=41796980); that is the realistic outcome of incremental oxidation: islands of Rust where the payoff is largest (parsers, style, graphics, media demuxers), connected by FFI, not a rewrite. Note the share *fell* because C++ also grew.
- Mozilla's FFI approach: `cbindgen` generates C headers from Rust, `bindgen` generates Rust from C, and Gecko's own XPCOM glue lets Rust components appear as XPCOM objects [background]. We will want the same tools for UniFFI-less C APIs.

**Servo.**
- Components are libraries (`components/`: `layout`, `script`, `net`, `fonts`, `paint`, `webgpu`, `media`, `servo` embedder API, `constellation`), the embedder app is `ports/servoshell`, and Stylo, WebRender and `wgpu` are external crates pinned by git revision [repo: servo2/components, ports, Cargo.toml:224-285]. An embedding API with a thin shell is again the VLC/mpv shape.
- Project history: Mozilla stopped funding it in the 2020 layoffs and the Linux Foundation took it [background]; **Igalia** took over maintenance in 2023, then Servo moved to Linux Foundation Europe governance with a Technical Steering Committee; funded by NLnet, Sovereign Tech Fund and 500+ donors (2024); v0.0.1 on 20 Oct 2025 with monthly releases planned [secondary: https://www.igalia.com/2025/10/09/Igalia,-Servo,-and-the-Sovereign-Tech-Fund.html, https://linuxfoundation.eu/newsroom/servo-web-rendering-engine-joins-linux-foundation-europe, https://www.techreviewer.com/developer-news/2025-10-21-servos-v001-release-sparks-new-hope-for-web-engine-diversity/]. Lesson: a Rust project that outlives its original corporate sponsor needs an independent fiscal home and a services firm; the code was reusable (Stylo, WebRender live on in Firefox).

**Rerun (Rust + wgpu + egui; a viewer for large data).** Best structural analogue to a "headless core with many front-ends" in Rust.
- **119 crates** grouped by layer into directories each with a README: `store`, `data_flow`, `utils`, `viewer_support`, `views`, `panels`, `top`, `tests`, `build` [repo: rerun/crates/*]. `top/` holds the end products (`rerun-cli`, `re_viewer`, `re_sdk`, `rerun_c`, web viewer server).
- **Multiple SDKs from one core:** Rust crate, Python (PyO3, in `rerun_py`), C++ (`rerun_cpp` over the `rerun_c` C ABI crate, built with CMake), and a Wasm web viewer from the same viewer crate; data crosses the boundary as Apache Arrow with `.rrd` files as the on-disk form [repo: ARCHITECTURE.md, rerun_cpp, rerun_py]. Same pattern as our "UniFFI/C ABI over an engine."
- **wgpu with a quality tier:** a custom `re_renderer` on wgpu, with WebGPU where available and a WebGL "quality tier" fallback; hot shader reload; rendering tests in `re_renderer_tests` with image snapshots, and UI tests through `egui_kittest` [repo: re_renderer/README.md, tests/re_renderer_tests, TESTING.md].
- **Release train:** a release every two weeks, unfinished work behind feature flags, all crates and SDKs versioned together [repo: RELEASES.md]. Dependency policy with `cargo-deny` across 12 target triples including `wasm32`, `aarch64-apple-darwin` and musl [repo: deny.toml]. Licence: MIT OR Apache-2.0 for most, funded as a company [repo: LICENSE-*].
- **Where it hurts:** egui is immediate-mode, great for a data viewer, less so for a polished photo-editor with complex a11y and text editing; Rerun accepts that [background; reasoning ours].

**Bevy (ECS, plugin architecture).**
- 63 crates in `bevy/crates`, features exposed as `Plugin`s: `trait Plugin { fn build(&self, app: &mut App); fn ready; fn finish; }` and `PluginGroup`s; apps are assembled by `app.add_plugins(...)`. All plugins are **compile-time Rust types**, no runtime loading and no stable ABI; a `bevy_dylib` crate exists only to speed up dev builds [repo: bevy_app/src/plugin.rs, bevy_dylib]. Shaders are WGSL (`.wesl` files, a WGSL extension with imports) on wgpu [repo: bevy_render/src/*.wesl, Cargo.toml]. Rust edition and MSRV (`rust-version 1.96`) are pinned; MIT OR Apache-2.0 [repo: Cargo.toml].
- **Cost:** Bevy's release cadence (roughly every 3-4 months) is explicitly breaking: each release ships migration guides (`_release-content`). It works because the audience accepts it; our engine's recipe format and CLI cannot [background].

**Ruffle (Flash emulator; Rust + wgpu + WebAssembly).**
- Workspace `core`, `swf`, `render` (trait `RenderBackend`, with `canvas`, `webgl` and `wgpu` implementations), `video`, `desktop`, `web`, plus `fuzz` (libFuzzer and AFL targets) and `tests` with **575 golden `output.expected.png` images** in the clone [repo: ruffle/render/src/backend.rs:28, ruffle/fuzz, ruffle/tests/tests]. wgpu 30 [repo: Cargo.toml:62]. Directly relevant: a **trait-based render backend, golden images in-tree, fuzzers for an untrusted-file parser (SWF), and the same crate compiled for desktop and web.**
- Ruffle's reason for Rust was memory safety, since Flash Player's security record was awful [background; the project says so on its site, not re-fetched].

**wezterm (terminal, Lua config, wgpu).**
- ~50 small crates in one repo (`termwiz`, `vtparse`, `wezterm-term`, `wezterm-font`, `mux`, `pty`...), many published separately to crates.io; GUI crate `wezterm-gui` has a `wgpu` dependency (25.x) alongside OpenGL; Lua is the config and scripting language, with `lua-api-crates/` exposing Rust functions to Lua [repo: wezterm/Cargo.toml, wezterm-gui/Cargo.toml:106, lua-api-crates]. Lesson: reusable crates published separately attract other users and contributors; Lua as the user-facing scripting layer matches our 06 §6 choice. Maintenance note: single maintainer; development pace slowed in 2024-25 [unverified].

**Lapce (Rust editor with floem).**
- Workspace of `lapce-app`, `lapce-core`, `lapce-proxy` (remote-dev proxy), `lapce-rpc`; UI is **floem**, its own reactive UI toolkit, pulled by git [repo: lapce/Cargo.toml:79]; plugins are WASI (WASM) with an LSP-like protocol [background]. It demonstrates the main GUI-ecosystem risk: building your own toolkit consumes the core team; floem is still pre-1.0. A GUI toolkit written alongside the app is a second project.

### 1.9 Memory-safety statistics: Android and Microsoft (why the language decision is also a security decision)

- **Microsoft:** about 70% of the CVEs Microsoft assigns each year are memory-safety issues, consistent over at least 12 years (MSRC, BlueHat IL, Feb 2019) [secondary: https://it.slashdot.org/story/19/02/11/2019247/microsoft-70-percent-of-all-security-bugs-are-memory-safety-issues, https://visualstudiomagazine.com/articles/2019/07/18/microsoft-eyes-rust.aspx; MSRC post "We need a safer systems programming language" (2019) not read in full]. Chromium's analysis of critical/high severity bugs since 2015 reached the same "over 70%" (same sources). In 2025 a Microsoft distinguished engineer's LinkedIn post about removing all C and C++ by 2030 was later clarified to be a research/recruiting effort, not a Windows plan [secondary: https://www.xda-developers.com/no-microsoft-isnt-actually-eliminating-c-and-c-from-its-software-rust/]. Do not cite it as policy.
- **Android (Google):** memory-safety vulnerabilities fell from the 70%+ range (2019-2022) to under 20% of Android vulnerabilities in 2025 as new code moved to memory-safe languages; Rust is about 5 million lines in the platform; in Google's estimate, **0.2 memory-safety vulnerabilities per million lines of Rust against about 1,000 per million lines for C/C++, "a more than 1000x reduction"**, based on one near-miss; Rust changes had a **~4x lower rollback rate, ~20% fewer review revisions and ~25% less review time** [verified: https://blog.google/security/rust-in-android-move-fast-fix-things/ (13 Nov 2025)]. Caveat: Android's new code is mostly new components (parsers, drivers, Bluetooth, media) and Google's methodology (young code has fewer bugs; "safe coding" shift) explains part of the effect; treat the 1000x as an upper bound [reasoning ours; Google's own 2024 post argues vulnerability half-life makes new-code safety dominant, not fetched].
- **Relevance to a RAW editor.** The attack surface of Ambrotype is a file parser: DNG/TIFF/EXIF/MakerNote/JPEG-lossless/XMP/ICC/ONNX. See §3.3 for CVEs in precisely these parsers.

---

## 2. Cross-cutting patterns

Each pattern gives: what the case studies do, then **what we take**.

### 2.1 Core library plus thin per-platform front-ends

- Seen in: VLC (libvlc + Qt, VLCKit, Android app), mpv (libmpv + IINA etc.), OBS (libobs + frontend), FFmpeg (libs + `fftools`), Rerun (SDK/viewer crates + Python/C++/Wasm), Servo (components + `servoshell`), darktable (`libdarktable` + GUI + `darktable-cli` + `darktable-mcp`), GIMP (GEGL/babl + GIMP).
- The failure case is darktable and GIMP: the library exists but UI toolkit types leak in (59 darktable pipeline/IOP files include GTK; GIMP 3.0 took seven years because the app, not the engine, was tied to GTK2).
- **Take:** the engine crate graph has no UI, windowing or OS-API dependency (08 §8.1 already enforces this with `cargo-deny` bans), parameters are described by a schema the engine owns, and every front-end (Tauri, CLI, SwiftUI, Compose, a future MCP/JSON-RPC server) talks to it through the same coarse, pixel-free protocol. Add a *second* front-end (the CLI) before the first GUI, as FFmpeg and mpv effectively did.

### 2.2 Plugin and module systems: static versus dynamic, ABI stability, capability-based selection

| Project | Mechanism | Selection | Stability promise |
|---|---|---|---|
| VLC | C modules, dynamic `.so/.dll` or static table, plugin cache | capability string + integer score, first successful `Open` wins | none across versions for plugins; libvlc API stable [§1.1] |
| FFmpeg | compile-time tables, `configure` flags | by codec/filter name, hardware variants chosen at init | libav* APIs with soname bumps; no runtime plugins [§1.2] |
| OBS | shared-lib modules, `obs_module_load`, registered structs | by id; user picks | `LIBOBS_API_VER` major/minor; struct-size trick [§1.4] |
| Blender | Python add-ons + C engines | user picks engine; add-ons by registration | API breaks on major versions; LTS holds API [§1.3] |
| darktable | IOP `.so` modules (compiled in same tree) + Lua | module order fixed by pipeline | `params_t` versioned, migrations [§1.6] |
| GEGL | GObject operation plugins (`.so`) | by operation name | LGPL library, ops split by licence [§1.5] |
| Bevy | compile-time `Plugin` trait | `app.add_plugins` | no ABI; semver per release [§1.8] |
| Zed | WASM components via wasmtime, WIT interface | extension manifest | WIT versioned [§1.8] |

- **Capability + score** (VLC) is the one idea worth stealing wholesale. In Rust it becomes a registry of trait objects: `register::<dyn RawDecoder>(name, priority, probe_fn)`, and the engine asks `probe(bytes)` in priority order. We need exactly this for raw decoders (rawler, LibRaw, platform decoder), colour backends, GPU vs CPU node implementations, AI backends and metadata backends (08 §8.5, 8.7 already propose traits).
- **Static is the default, dynamic is the exception.** FFmpeg and Bevy show that a high-performance project can be extensible entirely at compile time. VLC's static table exists because iOS forbids dlopen of arbitrary code [background: Apple's App Store rules; 08 §8.4 says no downloaded executable code]. Rust has no stable ABI (only the C ABI is stable; `abi_stable` and `stabby` exist but add runtime type checking and complexity) [secondary: https://nullderef.com/blog/plugin-abi-stable/, https://github.com/ZettaScaleLabs/stabby], so a dynamic Rust plugin interface is either C ABI or WASM. 
- **WASM components are now a credible plugin ABI.** WASI 0.3.0 (native async) was released 11 June 2026 and wasmtime 46+ implements it; Zed ships third-party extensions this way today [secondary: https://wasi.dev/releases/wasi-p3, https://zed.dev/blog/zed-decoded-extensions; repo: zed/Cargo.toml]. Costs: code runs roughly 1.1x-3x slower than native (secondary: nullderef link above), GPU access has to be mediated by the host, and wasmtime's JIT is not allowed on iOS [background; Pulley interpreter exists, [unverified]].
- **Data plugins first.** The cheapest extension points in our domain are not code: recipe JSON, `.cube`/HaldCLUT/3D LUTs, camera profile data, lens-correction DB entries, preset files, Lua scripts (darktable and mpv show how popular Lua is for photographers and power users; 06 §6 already says so).
- **Take:** (1) compile-time registry with VLC-style capability and priority for all swappable subsystems; (2) public C ABI only for the embeddable surface (libambrotype), with the OBS struct-size trick and an explicit API version; (3) a `PluginHost` trait so WASM nodes can be added without touching the engine; (4) no dynamic Rust plugins.

### 2.3 Pipelines and graphs

- **VLC:** a stream is a chain of modules (access -> demux -> packetizer -> decoder -> filter chain -> output), each negotiating formats; `vlc_filter.h` is the generic filter interface, with `video_filter`, `video_chroma`, `audio_filter` capabilities [repo: vlc/include/vlc_filter.h, modules/]. Format conversion is inserted automatically when two stages disagree (chroma converters are modules too). Same idea as babl "fish" paths.
- **GEGL:** demand-driven DAG with tiled buffers, dirty-rect invalidation and caching; babl for formats; operations declare their input/output formats and required region [repo: gegl/gegl/buffer, gegl/gegl/operation; secondary: gimp.org/release-notes].
- **darktable:** *linear* ordered pipe (not a general DAG) with ROI back-propagation, a cache keyed by module-stack hash, tiling fallback, CPU/OpenCL implementations per module, and the explicit `commit_params` split between stored parameters and runtime data [repo: src/develop, iop_api.h]. Because photo development is mostly a linear stack plus side branches (masks, histogram, analysis taps), it is simpler and cheaper than a general graph.
- **Blender depsgraph:** change tagging on data, evaluate only what changed, on worker threads against an evaluated copy [verified: code_layout page; mechanism: background].
- **vkdt (reference, from 06):** a full DAG on Vulkan with all processing in shaders; the fastest design but GPU-only and no CPU path.
- **Rerun/Bevy:** Bevy's render graph + ECS schedule, Rerun's query-over-store; not directly applicable.
- **Take (feeds §4.4):** a *recipe* (versioned, serialisable, stored in XMP/catalog) compiles to a *plan* (an ordered pipeline with optional side-branches for masks), the plan is executed per tile/ROI against a backend, and every node's output is cached under `hash(node_version, params, input_hashes, roi, scale)`. Keep it a restricted DAG (single main path + mask/analysis side-branches) rather than a fully general graph: it is easier to cache, tile and reason about, and it matches darktable's proven design. Format and colour space are part of the buffer type (babl lesson).

### 2.4 Job systems and threading

- **FFmpeg:** frame and slice threading inside codecs; `fftools/ffmpeg_sched.c` is a scheduler for the CLI's decode/filter/encode threads [repo]. **dav1d:** frame threads + tile threads; rav1d needed `DisjointMut` to make the sharing safe [§1.2]. **Servo/Stylo:** work-stealing for parallel style [§1.8]. **Krita:** a strokes queue that serialises painting jobs while the UI stays responsive [§1.5]. **Blender:** a task scheduler (TBB-based) for depsgraph and kernels [background]. **darktable:** OpenMP on CPU plus a "pixelpipe worker per image", with GUI/pipe thread sharing via locks (a recurring bug source: `GUI_Threading.md`) [repo].
- **Take:** CPU parallelism through `rayon` inside kernels (data-parallel, tile-based); one render-coordinator thread (async, latest-wins cancellation, per 06 §2.4) that owns the GPU queue and plan; I/O on a separate pool (tokio or `blocking` crates) so a slow disk does not block compute; **no shared mutable state between UI and engine**: UI sends commands, receives immutable snapshots and events (the GUI_Threading problem disappears by construction). Make cancellation first-class (a `Cancel` token threaded through plan execution) since preview and export both need it (08 §8.4).

### 2.5 GPU backend abstraction

| Project | Abstraction | Cost they paid |
|---|---|---|
| Blender | in-house `GPU` module, GLSL-to-MSL/SPIR-V source translation | years per backend; Vulkan default slipped from 4.x to 5.1+ [§1.3] |
| OBS | in-house `gs_*` API, runtime-loaded backends, HLSL-like effect files | backends per OS; Metal added late [§1.4] |
| mpv | internal `ra` layer, now delegating to **libplacebo** | moved shader/colour work to a shared library [§1.7] |
| Zed | `gpui_macos` (Metal) + `gpui_wgpu` (+ Windows/Linux crates) | text, a11y, atlas per platform [§1.8] |
| Ruffle, Bevy, Rerun, wezterm | **wgpu** | accept wgpu limits; Rerun adds a WebGL quality tier [§1.8] |
| GEGL | OpenCL ops alongside CPU ops | second-class path; never default [§1.5] |
| bgfx (not in the clone set) | C++ cross-API rendering library with its own shader compiler | separate shader toolchain [background] |

- Observation: every project that could afford it started with its own abstraction because nothing portable existed; the ones starting after 2020 (Ruffle, Bevy, Rerun, Zed's non-Apple path, wezterm) picked wgpu. None of them is a *compute-heavy photo pipeline*, which is the gap in the evidence: wgpu compute on 100 MP images with fp16/fp32 intermediates is exactly what 06 §8.6 spikes #1 and #2 are meant to measure.
- **Take:** keep wgpu as the only GPU API, but hide it behind a narrow `Backend` trait whose unit is a *node execution on a tile* (not wgpu types), so that (a) the CPU backend implements the same trait, (b) a native Metal/Vulkan/DX12 backend can replace wgpu for hot nodes without touching the pipeline, and (c) tests run the same graph on both. This is mpv's lesson (own thin `ra`, delegate heavy work) and Ruffle's `RenderBackend` trait.

### 2.6 Dependency and "contrib" management

- **VLC contribs** (145 recipes, pinned version and SHA512, cross-built for every target, system copy used if present) is the most complete C answer, at the cost of a large maintenance load [§1.1]. **OBS** keeps a separate `obs-deps` build [§1.4]; **Blender** keeps a `lib/` of prebuilt dependencies [background]; **Servo/Bevy/Rerun/Zed** simply use Cargo plus `Cargo.lock`, `cargo-deny` and sometimes `cargo vendor`; Rerun's `deny.toml` checks licences, duplicates and advisories across 12 targets; VLC itself offers `vendored_rust_deps` for Rust modules [repo: meson_options.txt:83].
- **Take:** Cargo for Rust deps; `cargo-deny` (licences with the MPL/GPL zone rule, bans for desktop crates in core, advisories) and `cargo-vet` or `cargo-audit` in CI; commit `Cargo.lock`; use `cargo vendor` for release tarballs; pin the toolchain (`rust-toolchain.toml`, stable channel; Zed pins an exact stable, VLC pins *nightly*, which we must avoid); for C dependencies we cannot avoid (LibRaw, lcms2, lensfun, exiv2) keep a small `contrib/`-style directory of `-sys` crates with vendored sources and checksum, built with `cc` for every target, like VLC's recipes but 5-10 libraries instead of 145.

### 2.7 Monorepo and workspace layout

- **One repo, many crates, directory per layer:** Rerun (119 crates in nine layer directories, each with a README) and Zed (249 crates) show it scales; Servo/Bevy 40-65 crates [repo].
- **Licence zones as directories:** GEGL (`operations/common` vs `common-gpl3+`), Zed (GPL app vs Apache GPUI, `LICENSE-*` per crate), mpv (`-Dgpl=false`) [§1.5, 1.7, 1.8].
- **Platform shells in separate repos when they are big:** VLC (vlc-android, vlc-ios) [§1.1]. We start in one repo with shells as `apps/*` and split if release cadences diverge.
- **Take:** see §4.1. Crates are small and layered, with `cargo-deny` enforcing the layering rules; do not split into many repos until a team owns each.

### 2.8 CI across platforms

- VLC: 25+ GitLab jobs including nightly and Meson variants; OBS: presets per OS with `-ci` variants, format check, static analysis, signing workflow; mpv: scripts per OS; Rerun: Docker CI images and a pixi environment for reproducible tooling; Ruffle: web and desktop; Zed: Docker and `ci/` scripts [repo].
- **Take:** GitHub Actions matrix (Linux x86_64/aarch64, macOS arm64, Windows x64/arm64) for `fmt`, `clippy`, `cargo nextest`, `cargo-deny`, golden-image tests with lavapipe for the GPU path, cross-checks for iOS/Android targets (check-only, 08 §8.2), nightly fuzz and fidelity runs; reproducible local tooling (`mise` or `pixi` like Rerun) so contributors run the same commands. Real GPU testing needs self-hosted or donated runners [unverified cost].

### 2.9 Fuzzing

- OSS-Fuzz: over 13,000 vulnerabilities and 50,000 bugs across 1,000+ projects as of May 2025, ffmpeg alone over 400 bugs reported; OSS-Fuzz supports C/C++, Rust, Go, Python, Java and others with libFuzzer, AFL++, Honggfuzz and Centipede [secondary: https://arxiv.org/html/2510.16433v1 (via search), https://google.github.io/oss-fuzz/]. FFmpeg's fuzz-driven fix effort and the STF grant produced a Coverity defect density under 1/30 of the OSS average [§1.2]. Ruffle ships `fuzz/` with libFuzzer and AFL targets; mpv has `fuzzers/` (`fuzzer_load.c`, `fuzzer_json.c`) [repo].
- Fuzzing Rust is cheaper (cargo-fuzz wraps libFuzzer) and still finds panics, overflows in `unsafe`, DoS (OOM, infinite loops), which in a desktop app are still bugs [secondary: https://github.com/rust-fuzz/cargo-fuzz].
- **Take:** a fuzz target per parser from day one (TIFF/DNG IFD walker, MakerNote readers, XMP, ICC, DCP, lrtemplate Lua-table parser, catalog import, recipe JSON), seed corpora from raw.pixls.us, run in CI for 60 s per PR and continuously on OSS-Fuzz (apply once there is a public repo; Rust is supported); sandbox any remaining C decoder (§3.3).

### 2.10 Release trains

| Project | Cadence | LTS |
|---|---|---|
| Blender | 3 per year (about 4 months), 1 LTS per year, 2 years of fixes | yes [§1.3] |
| darktable | 2 per year (June, December) [background] | no |
| Rerun | every 2 weeks, feature flags for incomplete work [§1.8] | no |
| Servo | monthly from 2025 [§1.8] | no |
| OBS | ~3 per year [background] | no |
| VLC | 3.0.x patch releases for years; 4.0 in development since the early 2010s [repo: 4.0.0 in Cargo.toml; background] | de-facto |
| mpv | irregular, `release-policy.md` [repo] | no |
| Bevy | every ~3-4 months, breaking [§1.8] | no |

- **Take:** time-based releases with feature flags (Rerun) once there is an alpha; semver for the *recipe schema* and the CLI/C API (breaking changes need a major bump and a migration), and stamp the engine version into every recipe and every fidelity report so renders are reproducible. Offer an LTS only when pipelines depend on us (Blender model).

### 2.11 API and ABI stability policies

- libvlc stable, plugins version-locked (VLC); mpv semantic client-API version + change log, and explicit advice to treat options as unstable (mpv); OBS major/minor API version + growable structs; Blender LTS freezes API; Krita/Blender break Python plugins at toolkit/major bumps; Bevy breaks every release [§1.1, 1.3-1.8].
- **Take:** three tiers, written down: (1) **on-disk formats** (recipe, catalog schema, XMP `amb:` namespace, preset import tables) are forever-compatible with explicit versions and migrations (darktable `params_t` lesson); (2) **embedding API** (C ABI + UniFFI + JSON command protocol) follows semver, with a changelog (mpv style); (3) **Rust crate APIs** inside the workspace are internal (unstable) until we choose to publish specific crates (e.g. `amb-color`, `amb-dng`) to crates.io.

### 2.12 Documentation

- darktable's in-tree `dev-doc/` (IOP API, pixelpipe, GUI threading), Rerun's `ARCHITECTURE.md`, `TESTING.md`, `CODE_STYLE.md`, `RELEASES.md`, per-layer README; VLC's `doc/` (clock, executor) and Hacker Guide on the wiki; Servo Book; OBS Sphinx docs; Blender developer docs; mpv `DOCS/` and `tech-overview.txt` [repo].
- **Take:** `docs/` is versioned with the code; first docs: ARCHITECTURE.md (this doc's §4), a "how to add a node" guide, TESTING.md, and ADRs (SCOPE Phase 0 already plans `docs/decisions/`); rustdoc for crates; an "AGENTS.md/CLAUDE.md" for AI-assisted contributions in the style of darktable's (they exist in darktable, Rerun and Zed trees [repo]).

### 2.13 Governance and funding

| Project | Legal home | Money | Notes |
|---|---|---|---|
| VLC | VideoLAN (French association loi 1901, 2009) | donations; Videolabs (for-profit) pays developers via consulting | no CLA, no ads, refused buyouts [§1.1] |
| FFmpeg | informal; Sovereign Tech Fund grant EUR 157,580 (2024-25) | STF, company contributions | `FUNDING.json` [§1.2] |
| dav1d | VideoLAN | Alliance for Open Media sponsorship | [§1.2] |
| rav1d | ISRG Prossimo | AWS, STF, Alpha-Omega | 20+ person-months [§1.2] |
| Blender | Blender Foundation (NL) | Development Fund, corporate tiers EUR 60k/120k/240k | LTS, 15+ staff [§1.3] |
| Krita | Stichting Krita Foundation + KDE e.V. | Dev Fund subscriptions, store sales | [§1.5] |
| Servo | Linux Foundation Europe, TSC | NLnet, STF, 500+ donors, Igalia | [§1.8] |
| OBS | OBS Project | Open Collective, Patreon, sponsors [background] | |
| darktable | informal project, no foundation [background] | donations | |
| mpv, wezterm | volunteers | - | |

- **Fiscal hosts for a young project:** Open Source Collective (10% flat fee; US legal entity [background]), Open Collective Europe (about 6%), Software in the Public Interest (5% of gross), Software Freedom Conservancy (10%, selective), NumFOCUS (scientific computing; not a fit), Linux Foundation Europe (Servo; no fee data) [secondary: https://docs.oscollective.org/welcome-and-introduction-to-osc/fees, search results listing OC Europe 6% and SPI 5%]. The 06 doc already suggests "OpenCollective/SFC" for signing and Apple fees.
- **Grants as of 3 Oct 2026 (this changes SCOPE's plan, see §4.9):**
  - **NLnet NGI Zero Commons Fund: closed.** The thirteenth and final call closed on 1 June 2026, and the fund takes no more new applications. [verified: https://nlnet.nl/commonsfund/]. NLnet's open programmes are **Restack** and **CodeSupply**, first deadline **3 November 2026, 12:00 CET**, EUR 5,000-50,000 for first-time applicants, EU-funded (Horizon Europe grant 101299072 and 101298846), requiring a European dimension, open licences, no AI-generated projects, and AI-related projects out of scope for Restack unless they already have over 1 million users [verified: https://nlnet.nl/propose/, https://nlnet.nl/restack/, https://nlnet.nl/codesupply/]. CodeSupply is about software-supply-chain metadata and is not a fit. **Restack's fit for a photo editor is uncertain** (its text is about "operational internet commons" and "alternatives to closed online technologies"); a pitch would need to emphasise open standards and interoperability (DNG/XMP/preset formats, a reusable colour and raw library) rather than "a Lightroom clone" [reasoning ours; [unverified] without asking NLnet].
  - **Sovereign Tech Fund** (Sovereign Tech Agency): budget of work must exceed EUR 50,000, awards start at EUR 50,000, OSI/FSF licences; it funds open digital base technologies such as libraries, package managers and protocols and **explicitly does not finance user-facing applications or prototypes**; ~6 months from submission to contract; criteria include prevalence, relevance, vulnerability [verified: https://www.sovereign.tech/programs/fund]. Ambrotype as an app is ineligible; a *separately usable library* (for example a Rust DNG/raw metadata parser or a colour-pipeline library) could become eligible only once other projects depend on it. It also has a **Fellowship** for individual maintainers (2026 round closed 6 April) [secondary: https://www.sovereign.tech/programs/fellowship].
  - **Others:** Alpha-Omega and GitHub Secure Open Source Fund (security-focused; USD 12.5M announced March 2026 via OpenSSF and Alpha-Omega; fits a memory-safe-parsers story) [secondary: https://openssf.org/press-release/2026/03/17/linux-foundation-announces-12-5-million-in-grant-funding-from-leading-organizations-to-advance-open-source-security/]; FUTO (funds Immich and runs grant programmes; details of applying not verified) [secondary: https://futo.org/blog/beyond-donations/]; GitHub Sponsors and Open Collective for individuals; Blender-style corporate memberships need adoption first.
  - **Store sales** (Krita model) are legitimate under MPL/GPL desktop licences and relevant to iOS/Android later [§1.5].

### 2.14 Contributor onboarding

- Documented build (`BUILD.md` in Rerun/Zed; `INSTALL`), one-command setup (pixi, nix, `./build.sh` in darktable), `CONTRIBUTING.md` + CoC, issue labels such as "good first issue" (Servo, Bevy) [repo]. darktable's rule "every commit must compile on its own" and "no unrelated reformatting" lowers review cost. Bevy publishes crate-level docs and examples (`examples/` is a big onboarding surface); Rerun has `examples/` in 3 languages. Small, well-bounded work items (a node kernel, a camera, a lens profile) are how imaging projects recruit: darktable's modules, GEGL's operations, VLC's 1,000-line modules.
- **Take:** "good first kernel" issues; a template crate/node with its golden test; CI that gives a visual diff of golden images on PRs; a Matrix/Discord room; DCO sign-off (SCOPE §6) rather than a CLA so contributors keep copyright (VLC model) while the licence split is already settled.

---

## 3. Language decision for an image-processing app in 2026: Rust, C++, C, Zig, Swift

**Verdict (details and conditions in §3.10): Rust for the engine, catalog, CLI, protocol and tooling. C and assembly only through existing libraries behind traits and sandboxes. C++ only where a C++-only dependency or tool is clearly better (Qt glue, Halide, OCIO/OIIO). Swift and Kotlin for the mobile shells. VLC's structure transfers; VLC's language is a 2001 constraint, not advice.**

### 3.1 Performance parity, SIMD, GPU interop

- **CPU parity is demonstrated.** The best public experiment is rav1d: a safe-Rust AV1 decoder, produced by transpiling dav1d and refactoring, reusing the *same* hand-written asm, ran 3.8% slower right after transpile (bounds checks accounted for under 1% of that) and "under 6%" slower after safety work, with Rust still about half the run time even with all asm enabled [verified: https://www.memorysafety.org/blog/rav1d-performance-optimization/]. Both compile through LLVM, so scalar and auto-vectorised code are on equal footing; Rust's `noalias` on `&mut` helps vectorisation, bounds checks hurt a little and are mostly removable with iterators and `chunks_exact` [background].
- **Our workload is not dav1d's.** Image kernels (point ops, 3x3/5x5 convolutions, separable blurs, curves, LUTs, local Laplacian pyramids) are regular, data-parallel and tile-friendly: they auto-vectorise well, and the GPU path carries the interactive load. The CPU backend's job is a reference oracle, headless CLI/batch, and no-GPU users, so a 5-10% gap would be irrelevant and an order-of-magnitude gap from not using SIMD at all would not.
- **SIMD status (Oct 2026).** `std::simd` (portable SIMD) is still **nightly-only** (tracking issue rust-lang/rust#86656), with incomplete operations and breaking changes; stable `core::arch` intrinsics are usable, and since Rust 1.87 many are callable safely inside `#[target_feature]` functions, but loads/stores remain `unsafe`; the practical stable answer is a crate: **fearless_simd 1.0** (one author's recommendation for image processing in the 2026 survey), `wide`, `pulp`, or `multiversion` for auto-vectorised code with runtime dispatch [verified: https://shnatsel.github.io/state-of-simd-rust-2026/; https://doc.rust-lang.org/unstable-book/library-features/portable-simd.html]. 06 §1.2 already says "`std::simd` still nightly"; that claim is **confirmed**. C++ has no better *shipping* answer on all three compilers either (`<simd>` is C++26, [unverified: compiler coverage]); Highway/xsimd are the third-party equivalents, and FFmpeg/dav1d/VLC prove that for the top 1% of hot loops teams hand-write asm in both languages.
- **GPU interop.** wgpu is a Rust library (Vulkan, Metal, DX12, GL, WebGPU), so GPU code in Rust is the first-class path; C++ would call Vulkan/Metal/D3D directly or use bgfx/Dawn. The gap: sharing textures with OS components (IOSurface, `CAMetalLayer`, DXGI swapchains, Android `SurfaceTexture`) needs `wgpu-hal` or raw handles in either language, and HDR swapchain control is limited in wgpu (06 §3.4); wgpu 29.0.4 added surface colour-space selection, which narrows this [secondary: search summary of wgpu releases; [unverified]]. Shaders are WGSL either way; Blender and OBS needed custom shader translators, wgpu's naga does that for us.
- **Parity verdict:** no meaningful CPU or GPU performance reason to prefer C++. Plan for a late-stage `unsafe`/intrinsics pass on 3-5 hot kernels if profiling demands.

### 3.2 FFI cost to C libraries (LibRaw, lcms2, lensfun, exiv2)

- Rust-to-C calls have no runtime overhead (same ABI; `extern "C"`; no marshalling of POD), and per-call cost is irrelevant at tile granularity. The costs are *build and safety*: `-sys` crates, `bindgen`/`cc` builds per target (the cross-compile pain, §3.4), wrapper-crate quality ("budget to own forks", 06 §1.4), and the fact that **wrapping C does not make the C safe** (see §3.3).
- Real mitigations, in order: (1) prefer a pure-Rust implementation when one is good enough (**rawler**: LGPL-2.1, about 30K lines, 300+ cameras, from dnglab, supports CR3 and JPEG-XL DNG [secondary: https://crates.io/crates/rawler, https://lib.rs/crates/rawler; search summary]; `image`, `kamadak-exif`, `quick-xml`); (2) keep each C library behind a trait, with a feature flag for the licence zone (08 §8.1); (3) **sandbox the C parser**: process isolation, or compile it to WebAssembly and run it in-process through wasm2c/wasmtime as Firefox does for Graphite, Hunspell, Ogg, Expat and Woff2 via RLBox since Firefox 95 [secondary: https://hacks.mozilla.org/2021/12/webassembly-and-back-again-fine-grained-sandboxing-in-firefox-95/ (via search summary)]. Ambrotype can do the same for LibRaw: it has no GPU or threading needs in the decode step.
- **Licence wrinkle for the MPL core:** rawler is LGPL-2.1, and 08 §5.5 notes LGPL static linking is a problem on iOS; LibRaw is used under its CDDL option (SCOPE §6). Neither blocks desktop; both must be checked for the mobile shells before we depend on them in `amb-io` core [reasoning ours; legal check needed].

### 3.3 Memory safety: file parsers are the attack surface

Raw files are untrusted input parsed by large amounts of format-specific C/C++. Evidence:

| Item | What | Source |
|---|---|---|
| LibRaw | **60 CVE IDs** in Debian's tracker (21 in 2018, 13 in 2020, 4 in 2025, 7 in 2026 so far) | [verified: https://security-tracker.debian.org/tracker/source-package/libraw] |
| CVE-2025-43961 / -43962 | out-of-bounds reads in LibRaw's Fujifilm 0xf00c tag parser and Phase One `phase_one_correct`, CVSS 9.1 each, fixed in 0.21.4 | [secondary: https://nvd.nist.gov/vuln/detail/CVE-2025-43962, https://www.strix.ai/cve/CVE-2025-43961] |
| CVE-2026-24660 | heap overflow from 32-bit integer overflow in `x3f_load_huffman()` (Sigma/Foveon X3F), CVSS 8.1, disclosed 7 Apr 2026 | [verified: https://talosintelligence.com/vulnerability_reports/TALOS-2026-2359] |
| CVE-2026-20884 | integer overflow to heap buffer overflow in `deflate_dng_load_raw()` (DNG), fixed in LibRaw 0.22.1; needs a large `max_raw_memory_mb` (about 11 GB) to trigger | [verified: https://access.redhat.com/security/cve/cve-2026-20884] |
| **CVE-2025-43300** | out-of-bounds write in Apple ImageIO/RawCamera's **lossless-JPEG DNG** decoder (TIFF `SamplesPerPixel` disagrees with SOF3 component count); Apple said it may have been used in an extremely sophisticated attack against specific targeted individuals; fixed in iOS/iPadOS 18.6.2 and macOS 15.6.1/14.7.8/13.7.8 | [secondary: https://thehackernews.com/2025/09/apple-backports-fix-for-cve-2025-43300.html, https://www.yorku.ca/uit/infosec/2025/08/22/apple-vulnerability-cve-2025-43300/] |
| CVE-2023-4863 | libwebp heap buffer overflow in Huffman decoding, exploited in the wild (Chrome, Apple BLASTPASS); affected "pretty much every application that handles WebP" | [secondary: https://blog.cloudflare.com/uncovering-the-hidden-webp-vulnerability-cve-2023-4863/, https://www.huntress.com/blog/critical-vulnerability-webp-heap-buffer-overflow-cve-2023-4863] |
| CVE-2023-44398 | out-of-bounds write in Exiv2 0.28.0 `BmffImage::brotliUncompress`, CVSS 8.8, fixed in 0.28.1 (a bug in *new* code) | [secondary: https://cyberstrike.io/cve/CVE-2023-44398/] |
| Exiv2 generally | repeated heap-buffer-overflow advisories in JP2, CRW, BMFF metadata code (e.g. CVE-2021-31292) | [secondary: https://github.com/Exiv2/exiv2/security/advisories/GHSA-jgm9-5fw5-pw9p] |

- Pattern: integer overflow in an allocation size, or a metadata field that disagrees with a stream header. These are the bug classes Rust's bounds checks and checked arithmetic (in debug and, with `overflow-checks`, release) turn into panics or errors. The real-world Android data says the same at scale: memory-safety bugs fell from 70%+ of Android vulnerabilities (2019-2022) to under 20% in 2025 as new code moved to Rust and other safe languages; Google estimates 0.2 per million lines for Rust versus about 1,000 for C/C++ [verified: https://blog.google/security/rust-in-android-move-fast-fix-things/]; Microsoft's long-standing figure is about 70% of CVEs [secondary, §1.9].
- **Caveats (do not oversell):** (1) Rust does not remove logic bugs, panics are DoS (a malformed raw crashing the app is still a bug), and OOM from an attacker-chosen size is not prevented; (2) `unsafe` in SIMD and FFI is where memory bugs can return; (3) Android's numbers partly reflect new-code effects; (4) some of the LibRaw CVEs above need non-default settings. Policy that follows: `#![forbid(unsafe_code)]` in every crate except a short allow-list (`amb-sys-*`, `amb-simd`, `amb-gpu-interop`, `amb-ffi`), `overflow-checks = true` in release for parser crates, fuzz targets for every parser, and sandboxing for C decoders (§3.2). A **photo editor opening files from cards, email and the web is a plausible spear-phishing target**; Apple's ImageIO DNG bug is the proof.
- **Verdict:** memory safety is the strongest single argument for Rust here, and it applies exactly at the boundary of this product (parsers), not the kernels.

### 3.3b Rust-ecosystem state for the crates we named (Oct 2026)

| Crate | State | Risk for us |
|---|---|---|
| `wgpu` | v29.0.0 18 Mar 2026, 29.0.4 later; v30.0.1 in use by Ruffle; Bevy, Zed, Ruffle, Rerun, wezterm all depend on it [repo: Cargo.toml files; secondary: GitHub releases search] | **Breaking changes about every quarter** (e.g. v29 changed `Surface::get_current_texture`); budget a quarterly upgrade; adapter variability (f16, storage-texture formats, buffer limits) is the real risk (06 §2.1, 08 §8.3). WebGPU in browsers: Chromium and Safari 26 yes, stable Firefox not yet [secondary: search summary] |
| `rayon` | de-facto standard data-parallel crate | low |
| `image` | mature, many decoders in pure Rust; not a RAW or colour-managed pipeline | low for I/O of JPEG/PNG/TIFF; do not use as our buffer type |
| `rawler` | LGPL-2.1, 300+ cameras, DNG writing via dnglab; smaller coverage than LibRaw/RawSpeed | camera coverage; licence wrinkle (§3.2) |
| `ort` | **no stable 2.0**: 2.0.0-rc.13 (28 Jul 2026), in RC since 2024; EPs per target (coreml, directml, nnapi) behind features [secondary: crates.io/docs.rs results] | API churn; keep behind `InferenceBackend` (06 §5) |
| `uniffi` | 0.32.0 (30 Jun 2026); used in Firefox mobile and desktop; the project calls itself ready for production use but still a long way from a 1.0 release [verified: https://github.com/mozilla/uniffi-rs; secondary: docs.rs release dates] | pin exact versions; keep the FFI surface coarse |
| `lcms2` crate | wrapper over C lcms2 (MIT) | C dependency on mobile builds; evaluate pure-Rust CMS later [unverified] |
| SIMD crates | `fearless_simd` 1.0, `wide`, `pulp`, `multiversion` | see §3.1 |
| `wasmtime` | 46+ implements WASI 0.3; Zed uses 48 [repo; secondary] | JIT not allowed on iOS |

### 3.4 Build and cross-compile: iOS, Android, Windows on ARM

- Rust: `aarch64-pc-windows-msvc` became **Tier 1** in Rust 1.91 (30 Oct 2025) [secondary: https://blog.rust-lang.org/2025/10/30/Rust-1.91.0/, https://www.phoronix.com/news/Rust-1.91-Released (via search summary)], so Windows on ARM gets full CI-tested support; Apple tvOS/watchOS/visionOS targets were promoted to Tier 2 in Rust 1.95 (16 Apr 2026) [secondary: https://doc.rust-lang.org/beta/releases.html (via search)], and iOS and Android arm64 targets have long been Tier 2 [background]. Cargo makes the Rust part trivial (`cargo build --target ...`); `cargo-ndk`, `cargo-xcode`, and UniFFI's xcframework scripts glue it to Gradle/Xcode. The real cost is **C dependencies** (LibRaw, lcms2, lensfun, exiv2, ONNX Runtime): each needs a per-target build (NDK r28+, 16 KB page alignment, Xcode SDKs); that is VLC's "contrib" burden in miniature (§2.6). Rerun does 12 target triples in `cargo-deny` and builds for Wasm, which shows the Rust-only tail is cheap.
- C++: CMake + vcpkg/Conan; a well-trodden path for Android (NDK) and iOS (Xcode), and where Qt lives, but Windows/ARM64, Linux and macOS universal builds multiply toolchain matrix work (VLC: 25+ CI jobs; OBS: 4 presets families); every dependency needs a recipe.
- Swift: official toolchains for Linux, Windows (x86_64 and arm64) and, since March 2026, a nightly **Android SDK** on swift.org; SwiftUI itself is not ported to Android [secondary: https://forums.swift.org/t/announcing-the-android-workgroup/80666, search summaries]. Good for the iOS shell, premature for a shared engine.
- Zig: excellent cross-compiler (`zig cc`), but the language is pre-1.0 (0.16 in April 2026; JetBrains post "Why Zig isn't 1.0 (yet)", June 2026) [secondary: https://blog.jetbrains.com/blog/2026/06/05/why-zig-isn-t-1-0-yet/]; ecosystem for imaging is tiny.
- **Verdict:** Rust is at least as good as C++ for cross-compiling our own code and better for dependency management; the shared problem is C dependencies, which we minimise.

### 3.5 Contributor pool and hiring

- Stack Overflow Developer Survey 2025 (all respondents): Python 57.9%, **C++ 23.5%, C 22.0%, Rust 14.8%**, Swift 5.4%, Zig 2.1%; professional developers: C++ 21.8%, C 19.1%, Rust 14.5%, Swift 5.7%, Zig 1.9% [verified: https://survey.stackoverflow.co/2025/technology]. Rust is the **most admired** language (72.4% of its users want to keep using it, the ninth year running); Zig 64.2% [secondary: https://survey.stackoverflow.co/2025/technology via search summaries].
- Reading: the C++ pool is about 1.6x larger than the Rust pool in the general population, and larger still among graphics and imaging specialists (darktable, GIMP, Krita, digiKam, RawTherapee are C/C++; [reasoning ours]). But the contributor we need is a *volunteer* who wants to work on a photo editor, and Rust's admiration rate and the "modern OSS" halo (Zed, Servo, Bevy, rav1d) help recruiting; RapidRAW (Rust + Tauri + wgpu, about 9k stars per SCOPE §1) is evidence there is an audience. Kernel work in WGSL and recipe/preset work in data files are accessible to non-Rust developers (06 §1.3 mitigation).
- Hiring/grants angle: funders like STF and Alpha-Omega explicitly fund memory-safety work (rav1d), so Rust is a funding asset as well as a technical choice [§1.2, §2.13].

### 3.6 Compile times

- Rust compiles slower than C and about on par with heavy C++ (templates); monomorphisation, proc macros, and big dependency trees (wgpu/naga, egui, tauri) drive clean builds. Mitigations that are standard in the projects we cloned: workspace split into many crates (Rerun 119, Zed 249), `cargo nextest`, `sccache`, `mold`/`lld`, `cargo check`, pinned toolchain, dev profile tweaks (`opt-level = 1` for deps, `debug = "line-tables-only"`), and `dylib` dev feature (Bevy's `bevy_dylib`) [repo]. Upstream: the parallel front-end gives about 20-30% faster builds and the Cranelift backend about 20% lower codegen time (about 5% total on clean builds of Zed/Tauri) with "Relink, don't rebuild" and the Wild linker on the 2026 roadmap [secondary: https://rust-lang.github.io/rust-project-goals/2026/roadmap-fast-builds.html, https://kobzol.github.io/rust/2026/09/30/stf-august-september-2026.html (via search)].
- Our estimate for a mid-size workspace (about 20 crates, wgpu, rayon, uniffi): clean release build 5-12 minutes on a laptop, 5-20 s incremental for a leaf-crate edit; [unverified, to be measured in Phase 0 and put in CI dashboards]. **This is the cost we accept.** Keep kernels in small leaf crates so edit-test loops stay short; shaders hot-reload (Rerun does) so tuning needs no Rust rebuild.

### 3.7 GUI ecosystem risk (Rust has no mature desktop GUI), and why the plan survives it

- Evidence: Lapce built its own toolkit (floem) and is still pre-1.0; Zed built GPUI and owns text shaping, IME and accessibility per platform; Rerun uses egui (immediate mode; fine for a data viewer); wezterm draws its own UI; Servo/Tauri-class shells use webviews [§1.8].
- Our plan already avoids the trap: the UI shell is *replaceable* and *not Rust-bound* (Tauri/web panels or Qt on desktop; SwiftUI and Compose on mobile; 06 §3.5, 08 §3.8). Rust is the engine language, not the widget language. The GIMP/darktable experience (§1.5, §1.6) says the expensive mistake would be binding the engine to a toolkit.
- Residual risk: the Tauri-native-surface viewport (06 §3.3) needs platform code per OS; Qt via `cxx-qt` brings C++ back in. Both stay confined to the shell directory (§4.1). Do not adopt GPUI/egui/iced for the shipping shell (06 §3.5 agrees).

### 3.8 Other candidates in one paragraph each

- **C:** the VLC/FFmpeg/darktable language; best for ABI stability and embedding, lowest-level control, no ergonomics, no safety. Right for the *public embedding ABI* (a small C header, cbindgen-generated from Rust), wrong for new engine code.
- **C++:** the realistic runner-up; Qt, LibRaw, lensfun, exiv2, OIIO, OCIO, Halide, Krita, digiKam, RawTherapee all live there. Wins if the team is C++-first and picks Qt, or if the engine is to be forked from RawTherapee/ART code (C++/GPL). Loses on memory safety, dependency and cross-compilation friction, and on being a worse fit for wgpu.
- **Zig:** pre-1.0 (0.16, April 2026), tiny ecosystem; admired (64%) and a great C/C++ cross-compiler, but not a foundation for a multi-year product.
- **Swift:** first-class on Apple (Metal, Core ML, PhotoKit) and the right language for the iOS shell; Android SDK is new (nightly SDK March 2026), no wgpu, no rayon, smaller contributor pool (5.4%). Use it for the shell, not the engine.
- **C#/Avalonia, Flutter/Dart, Kotlin Multiplatform:** not engine languages (06 §1.2); Kotlin is the Compose shell language on Android.

### 3.9 What VLC would choose today (evidence, not speculation)

- VLC's own tree answers it: the README says the main codebase is written in C but the repository also holds a lot of C++, Obj-C, assembly and Rust; VLC 4.0's tree has an **opt-in, experimental Rust module layer** (`vlcrs-*` crates, a Rust logger module, nightly toolchain pin, Meson `rust` feature default disabled) [repo: §1.1]. VideoLAN also hosts rav1d's parent dav1d (C + asm) and its own libraries remain C; rav1d itself is a separate, Prossimo-funded Rust port, not a VideoLAN project [verified: memorysafety.org posts].
- So VideoLAN's practical answer is: **keep the proven C+asm core; allow new modules in Rust at the plugin boundary; do not rewrite.** That is *consistent* with Rust for us, because we have no C core to protect: a greenfield project in 2026 with parsers as the attack surface sits on the other side of the same trade-off. I found no public statement by Kempf or VideoLAN committing to or against a Rust rewrite [unverified].
- Transferable *structure* from VLC (more important than language): capability/priority module selection, static-capable plugin table, a small stable embedding API (libvlc) distinct from internal APIs, per-platform front-ends in their own repos, contribs for cross-built dependencies, a non-profit with no ads, no CLA, and a for-profit services company that pays developers.

### 3.10 Verdict, conditions, and where C++ would be better

**Decision: Rust.** Specifically:

1. **Rust** for `amb-core`, `amb-color`, `amb-io`, `amb-gpu`, `amb-cpu`, `amb-presets`, `amb-catalog`, `amb-cli`, `amb-ffi` and the app service layer; WGSL for GPU kernels; stable toolchain pinned in `rust-toolchain.toml` (never nightly in the product; VLC's nightly pin is acceptable only because its Rust is experimental).
2. **C/C++/asm only via existing libraries:** LibRaw (optional, sandboxed), lcms2, lensfun, exiv2 (desktop GPL pack), ONNX Runtime; each behind a trait, each with a pure-Rust or platform alternative on the roadmap. No new hand-written C. Asm/intrinsics later, per hot kernel, with a Rust reference and a checkasm-style equivalence test (FFmpeg model) [§1.2].
3. **Shells in the native language of the platform:** TypeScript (Tauri panels) or QML/C++ (Qt); Swift/SwiftUI; Kotlin/Compose.

**Conditions under which the verdict holds (each is checkable in Phase 0):**
- The viewport/GPU spike (06 §8.6 #1, #2) shows wgpu meets the tiled 100 MP budget on Metal and DX12 with acceptable fp16/fp32 handling. If wgpu fails on Metal or DX12, keep Rust and add a native backend for that API behind the `Backend` trait; do not switch language.
- At least two contributors comfortable with Rust at kickoff (a project with zero Rust experience should prototype before committing). RapidRAW's existence shows the pool is not empty.
- Build times stay under a budget we set (e.g. 15 min clean release on CI, 30 s leaf incremental); if not, restructure crates before blaming the language.
- `unsafe` stays confined to the allow-list.

**Where C++ would be the better choice (be honest about these):**
- **If Qt is the chosen shell and the team is C++-first:** write the UI glue and viewport item in C++/QML; use `cxx-qt` only if its maintenance is acceptable (06 §3.5 path B).
- **If the engine were to be a fork of RawTherapee/ART/darktable code:** staying C/C++ avoids a translation layer; SCOPE already rejected forking as a base (§5 option C), and the gpl-pack ports can be done per-algorithm instead.
- **Halide for a few kernels:** Halide's algorithm/schedule split powers Adobe's local Laplacian (clarity/tone mapping in Photoshop and Lightroom) [secondary: https://cacm.acm.org/research/halide/]; if Highlights/Shadows or Texture fitting needs Adobe-like local Laplacian performance on CPU, an AOT-compiled Halide kernel consumed over a C ABI is an escape hatch. It adds a C++ toolchain and a second GPU path, so only if the Rust CPU kernel fails the budget.
- **Colour pipelines for film-style workflows (OCIO) or OIIO I/O:** C++ libraries; optional plugins, not core.
- **Apple-only fast paths** (Metal Performance Shaders, Core ML Neural Engine): called from Swift/Obj-C or through `ort`'s CoreML provider, not from the core.

### 3.11 What this means for 06 §1

06 §1.3 reaches the same recommendation; changes this section suggests are small:
- Update the "`std::simd` nightly" note with the 2026 crate recommendation (§3.1).
- Add the CVE evidence (§3.3) to the memory-safety rationale, and add the **sandboxing option for LibRaw** (§3.2).
- Add the rawler LGPL/iOS note (§3.2).
- Add the `ort` RC status and `uniffi` pre-1.0 status to the risk table (§3.3b).
- 06 §8.5's licence paragraph ("GPL-3.0-or-later is the pragmatic choice") is already superseded by SCOPE §6; leave a pointer.

---

## 4. Recommended structure for Ambrotype

This section is a recommendation layered on the existing plan (SCOPE §5, §6, §9; 06 §8; 08 §8). §4.10 lists every place it differs.

### 4.1 Repository and workspace layout

One Git repository, one Cargo workspace, directories per **layer and licence zone** (Rerun's layering, GEGL's and Zed's licence directories, 08 §8.1's zones).

```
ambrotype/
  Cargo.toml  Cargo.lock  rust-toolchain.toml  deny.toml  clippy.toml
  LICENSES/ (MPL-2.0, GPL-3.0-or-later, CC-BY-SA data licence)  REUSE.toml  SECURITY.md  CONTRIBUTING.md (DCO)
  docs/            ARCHITECTURE.md  TESTING.md  RELEASES.md  decisions/ (ADRs)  research/  adobe-docs/
  crates/
    core/          MPL-2.0   headless engine; no UI, no OS APIs, no GPL deps
    gpl-pack/      GPL-3.0-or-later   desktop-only ported algorithms and GPL bindings
    sys/           per-library   thin -sys / vendored C (LibRaw CDDL option, lcms2 MIT, ...)
    platform/      MPL-2.0   amb-platform-{win,mac,linux,ios,android}: display profile, file access
  apps/
    cli/           MPL-2.0   `amb` binary (the reference front-end)
    desktop/       GPL-3.0-or-later (combined work)   Tauri 2 or Qt shell + gpl-pack linked in
    ios/  android/ MPL-2.0   native shells over amb-ffi (start here as folders; may become repos, VLC-style)
  contrib/         vendored C sources + SHA-256 + build recipes for the few unavoidable C libs
  data/            lensfun DB (CC-BY-SA, separate package), fitted tables, test charts
  shaders/         WGSL sources (hot-reload in dev), referenced by amb-gpu
  fuzz/            cargo-fuzz targets, seed corpora pointers
  xtask/           build/release/test automation in Rust (cargo xtask ...)
  tests/           golden images (CC0 inputs only), recipes, fidelity configs (private refs stay out)
```

**Crates** (names follow SCOPE/06/08 where they exist; new ones are marked *new*):

| Crate | Zone | Responsibility |
|---|---|---|
| `amb-recipe` *(split from amb-core)* | core, MPL | Versioned recipe data model, `crs:` tag registry, parameter **schema** (id, type, range, default, group, mask support), migrations. No pixels, no I/O. Used by catalog, CLI, FFI, UI. |
| `amb-buffer` *new* | core | Typed tile buffers (pixel format + colour space in the type, babl lesson), ROI/rect math, tile planner inputs. |
| `amb-graph` *(split from amb-core)* | core | Recipe-to-plan compiler, ROI back-propagation, cache keys, scheduler, cancellation, the `Backend` trait and the **capability registry** (§4.2). |
| `amb-nodes` *new* | core | Node specs (params, halo, formats, fusable flag) and their registration; one module per node (`exposure`, `curves`, `hsl`, `grading`, ...). |
| `amb-cpu` | core | CPU backend and **reference kernels** (rayon + SIMD crate); test oracle and headless renderer. |
| `amb-gpu` | core | wgpu backend, WGSL kernels, tile/limit planner (`MOBILE_FLOOR`), `PresentTarget` trait. Only crate that names `wgpu`. |
| `amb-color` | core | Working-space maths, camera matrices, DCP apply, WB, curves, ICC via a `ColorEngine` trait (lcms2 adapter in `sys`). |
| `amb-io` | core | TIFF/DNG/EXIF/XMP/ICC parsing (own, safe, fuzzed), `RawDecoder` trait and registry; adapters for rawler and LibRaw. |
| `amb-presets` | core | XMP / `.lrtemplate` / DCP importers, fitted tables, fidelity report (SCOPE Phase 1). `amb-lrcat` *(was amb-lrimport)* for catalog import. |
| `amb-catalog` | core | SQLite catalog, sidecars, preview pyramid, sync. |
| `amb-ai` | core (feature) | `InferenceBackend` trait, `ort` adapter, model manifest and licence gate. |
| `amb-protocol` *new* | core | Commands, events, parameter schema DTOs, JSON schema; the only thing shells see; versioned like mpv's client API. |
| `amb-app` | core | Service layer: jobs, history/undo, settings, Lua host (later), plugin host trait. |
| `amb-ffi` | core | C ABI (cbindgen) and UniFFI bindings over `amb-protocol` + `amb-app`. Like libvlc. |
| `amb-testkit` *new* | core, dev-only | Golden-image harness, CPU/GPU equivalence runner, fidelity runner, fuzz helpers. |
| `amb-gpl-*` | gpl-pack, GPL-3.0+ | `amb-gpl-demosaic` (AMaZE port), `amb-gpl-denoise`, `amb-gpl-upright`, `amb-exiv2`, `amb-lensfun`. Implement core traits; linked only by `apps/desktop`. |
| `amb-sys-libraw`, `amb-sys-lcms2`, ... | sys | Vendored C with checksums, built via `cc` for every target; LibRaw runs behind the sandbox adapter (§4.6). |

**Rules enforced in CI:** (1) `cargo-deny` bans `tauri*`, `gtk*`, `winit`, `objc*`, `windows*`, `jni`, `ndk*` and any `amb-gpl-*` from `crates/core/*` (08 §8.1; SCOPE §6 gate); (2) SPDX headers per file and a `reuse lint` check; (3) `#![forbid(unsafe_code)]` everywhere except `amb-sys-*`, `amb-ffi`, `amb-gpu` (interop module), `amb-cpu` (SIMD module); (4) `wgpu` only in `amb-gpu`; (5) no `cfg(target_os)` in core crates.

### 4.2 Module and plugin strategy

1. **Compile-time registry with VLC-style capability and priority** (the one VLC idea to copy exactly). In `amb-graph`:
   ```rust
   registry.register(Capability::RawDecoder, "rawler", priority = 100, probe = rawler_probe, open = rawler_open);
   registry.register(Capability::RawDecoder, "libraw-sandboxed", 50, libraw_probe, libraw_open);
   registry.register(Capability::NodeImpl { node: "exposure", backend: Gpu }, "wgsl", 100, ...);
   ```
   Queries are `registry.best(cap, &context)` where the context carries file bytes, adapter limits, feature flags, user override; the first candidate whose `probe` accepts wins, falling back down the priority list (VLC's `module_need`). Users and tests can force a choice (`--decoder=libraw`, `--backend=cpu`). Registration is an explicit `register_all()` in the app crate, not linker magic, so mobile and static builds are trivial (VLC's static table lesson).
2. **Pipeline nodes are compile-time**: one module per node in `amb-nodes`, one CPU and one WGSL implementation, one golden test, one equivalence test, one parameter-schema entry. This is "how to add a node" (darktable IOP guide, GEGL operations, VLC modules: the unit contributors work on).
3. **Data plugins in v1** (no code): preset files, recipes, `.cube`/Hald LUTs, camera profile data, lens DB entries, fitted tables, styles. Everything importable at runtime from the user's folder.
4. **Scripting later**: Lua (mlua) on desktop for import/export hooks, batch rules and preset logic (Phase 4, per SCOPE; 06 §6 said "v1" which is too early, see §4.10); the same JSON command protocol exposed on a local socket for external scripts (mpv model), no embedded Python.
5. **Runtime code plugins later still, WASM only**: a `PluginHost` trait in `amb-app` reserves the seam; when needed, WASM components with a WIT interface for tile-in/tile-out nodes, exporters and model adapters, hosted by wasmtime (Zed precedent, §1.8), desktop-only (JIT, 08 §8.7), the host mediating all GPU access. **No dynamic Rust plugins** (no stable ABI) and **no public C plugin ABI** until there is demand; if one is ever added, use the OBS struct-size growth trick and an explicit `AMB_API_VERSION`.
6. **Embedding ABI (the "libvlc")**: `amb-ffi` exports a small C header and UniFFI bindings over `amb-protocol`; this is the *only* binary-stable surface (§2.11).

### 4.3 Pipeline graph design notes

- **Recipe -> plan -> execution.** A recipe is versioned data (JSON/CBOR; stored in XMP/catalog). `amb-graph` compiles it into a plan: a single main path in darktable-like fixed order (decode -> mosaic -> WB/black -> demosaic -> input colour -> exposure/tone -> HSL/grading -> local adjustments -> lens/geometry -> detail -> output transform; 06 §2.4) with side branches for masks and analysis (histogram, clipping, colour picker). A *restricted DAG*, not a free graph (§2.3).
- **Node contract:** `{id, version, params: Hash + typed, input formats, output format, roi_in(roi_out), halo, fusable, backends}`. Node `version` is part of every cache key, and the plan records `engine_version`, so any render is reproducible from (recipe, engine version).
- **Caching:** key = `hash(node_version, params, input_keys, roi, scale)` (darktable's rule); GPU texture LRU, spill to RAM, then disk for expensive nodes (demosaic, denoise) per 06 §2.4. Content-addressed so previews are droppable under memory pressure (08 §8.5).
- **Fusion:** point-wise nodes fuse into one generated or hand-written kernel; neighbourhood nodes are separate passes with halos computed by ROI back-propagation.
- **Tiling is not optional:** every node must run on a tile with halo under `MOBILE_FLOOR`; the planner picks tile size from `{limits, budget}`; full-frame buffers exist only as proxies (08 §8.3).
- **Parameters resolution-independent** (fractions of long edge) so proxy previews match export (08 §8.4).
- **Formats and colour space in the buffer type** (babl lesson), conversions inserted by the planner as explicit nodes, never implicit.
- **Cancellation and progressive rendering:** latest-wins, draft pass then refinement (06 §2.4); a `Cancel` token threaded through every node invocation.
- **Graph introspection from day one:** `amb explain recipe.json` prints the plan, cache keys and per-node timings (GEGL has `gegl-dot`; darktable has pipe debug; use it for contributors and bug reports).

### 4.4 Threading and job system

- **Roles:** one *render coordinator* (owns plan and GPU queue); a `rayon` pool for CPU kernels and decode; an I/O pool for file reads and sidecar writes; a *catalog writer* thread (single writer, 06 §4.1); an *inference* thread per execution provider; UI threads never touch engine state.
- **Message passing, not shared state:** the UI sends commands (`amb-protocol`) and receives events and immutable snapshots, which removes the darktable "GUI_Threading" class of bug by design (§1.6).
- **Async only at the edges:** tokio (or `smol`) in `amb-app` for watchers, network (model download), and IPC; the engine crates are synchronous with `rayon` and channels, so they build for mobile and WASM-hosted tests without a runtime. [judgement; 06 §8.1 draws tokio and rayon together in the service layer, which this keeps.]
- **Budgets:** `ResourceBudget{gpu_bytes, cpu_bytes, thermal}` is an input to the planner (08 §8.3), not a global.
- **Lesson from dav1d/rav1d:** if we ever share mutable tile buffers across threads, encapsulate it in one audited type (`DisjointMut`-style) rather than spreading `unsafe` [§1.2].

### 4.5 The GPU abstraction boundary

```
 amb-graph  --calls-->  trait Backend {
                           caps() -> BackendCaps            // limits, features, tile sizes, f16?
                           prepare(plan) -> PreparedPlan    // compile pipelines, warm caches
                           run_node(inv: &NodeInvocation, ins: &[TileRef], out: TileMut, cancel) -> Result
                           upload / readback / evict
                        }
 amb-cpu  : impl Backend      (reference; rayon + SIMD)
 amb-gpu  : impl Backend      (wgpu + WGSL; the only crate that names wgpu types)
 future   : amb-gpu-metal / amb-gpu-d3d12 / amb-gpu-vulkan  (native fast paths, if wgpu limits bite)
 amb-gpu::present : trait PresentTarget  (native surface, colour space, HDR headroom); shells implement it
```

- The unit of abstraction is **a node on a tile** (not a draw call), the lesson from mpv (own thin layer, delegate the rest) and Ruffle's `RenderBackend` trait [§1.7, §1.8].
- wgpu types never appear in `amb-graph`, `amb-protocol`, FFI or shells. That keeps the "replace wgpu for one backend" option cheap and makes the CPU/GPU equivalence test trivial: same plan, two `Backend`s.
- Shader sources are WGSL files in `shaders/`, hot-reloadable in dev (Rerun), with a small preprocessor for includes and constants; fp16 behind a feature check with an fp32 fallback (08 §8.3: never *require* `SHADER_F16`).
- Budget for **one wgpu upgrade per quarter** (breaking releases; §3.3b) in a dedicated branch.

### 4.6 Platform front-end strategy (VLC style)

- `amb-ffi` plays the role of **libvlc**: small, coarse, versioned, with an `AssetRef`/`ByteSource` abstraction instead of paths (08 §8.5) and a pixel-free protocol (08 §8.6). The CLI (`apps/cli`) is the *reference front-end* that exercises every command and is the driver for tests, like `ffmpeg`/`mpv`.
- **Desktop shell**: Tauri 2 with a native wgpu viewport (default) or Qt 6, decided by the Phase 0 spike (SCOPE §5, 06 §3.5). Expect to **replace** this shell at least once in the project's life (GIMP's GTK port, Krita's Qt5-to-Qt6 move, Lapce's own-toolkit detour all took years); the protocol and the schema-generated panels are what survive.
- **Mobile**: SwiftUI and Compose over UniFFI (08 §3.7), each implementing `DisplayProfileProvider`, `PresentTarget`, and `AssetRef` providers (PhotoKit, SAF). VLC's mobile apps live in separate repos with their own release cadences; we start them as folders in the monorepo and split only if cadence or contributors demand it.
- **Conformance**: record and replay protocol scripts (JSON command logs) against every shell and the CLI, comparing exported pixels to goldens; VLC's synchronized 3.0 release across desktop and mobile shows it is possible but costly [§1.1].
- **Licence consequence** (SCOPE §6): desktop app = GPL-3.0-or-later combined work; mobile shells = MPL core only. No GPL crate may be reachable from `apps/ios` or `apps/android` (CI dependency gate).
- **Parsers that touch untrusted bytes** run in `amb-io` safe Rust where possible; **LibRaw, if enabled, runs sandboxed**: out-of-process on desktop (a helper binary speaking a tiny length-prefixed protocol over a pipe, killed on timeout, memory-limited), and compiled to WASM with wasmtime/wasm2c where processes are not an option (RLBox precedent, §3.2). The registry's priority puts rawler first and falls back to sandboxed LibRaw on probe failure (needs Phase 0 coverage numbers on a camera sample set).

### 4.7 Build and CI matrix; testing pyramid; release cadence

**Build.** Cargo for everything; `xtask` for packaging; `rust-toolchain.toml` pinned to a stable release, updated monthly in a bot PR; MSRV = pinned stable. `contrib/` holds C sources with SHA-256 and `cc` build scripts (VLC contribs in miniature: LibRaw, lcms2, lensfun, exiv2 desktop-only, ONNX Runtime via `ort`'s download or a pinned build). Mobile builds via `cargo-ndk` and an xcframework script. Reproducible dev environment with `mise` or `pixi` (Rerun).

**CI matrix** (GitHub Actions; extend with self-hosted GPU runners when donated):

| Lane | When | What |
|---|---|---|
| Fast | every PR | `fmt`, `clippy -D warnings`, `cargo nextest` (CPU) on Linux x86_64, `cargo-deny` (licences, bans, advisories), licence-zone gate, `reuse lint`, docs build |
| Platform | every PR | build and CPU tests on macOS arm64, Windows x64, Windows arm64 (native Tier 1 since Rust 1.91), Linux aarch64 |
| GPU-soft | every PR | GPU path under **lavapipe** with `required_limits = MOBILE_FLOOR`; CPU/GPU equivalence for all nodes |
| Mobile check | every PR | `cargo check` for `aarch64-apple-ios(-sim)`, `aarch64-linux-android`, `x86_64-linux-android`, core only (08 §8.2); Phase 1: run CPU goldens on simulator/emulator |
| GPU-real | nightly | Metal, DX12, Vulkan (NVIDIA/AMD/Intel/Apple) on donated or self-hosted runners; failures create issues [unverified: availability and cost] |
| Fuzz | PR (60 s) + nightly (30 min/target) + OSS-Fuzz | all parser targets (§2.9); crashes become regression tests |
| Perf | nightly | `criterion` benchmarks for kernels and full recipes; regression threshold bot; memory peak |
| Fidelity | nightly | preset fidelity dashboard against private reference renders (SCOPE Phase 1 exit criteria: median dE00 < 2 for top-20 tags) |
| Release | tag | signed Windows (SignPath Foundation), notarised macOS universal DMG, Flatpak + AppImage, MSIX; SBOM (`cargo-cyclonedx`); checksum + provenance |

**Testing pyramid** (bottom is largest and fastest):
1. **Unit and property tests** (`proptest`) for maths, parsers, schema migrations.
2. **Node tests**: each node has (a) analytic expectation tests, (b) a golden image, (c) CPU/GPU equivalence within epsilon (FFmpeg `checkasm` model; per-channel epsilon, not bit equality, 06 §2.2).
3. **Golden recipes**: end-to-end CLI renders on CC0 raws compared against stored output with a perceptual metric (Ruffle has 575 golden PNGs; Rerun uses snapshot tests; FFmpeg FATE) [§1.2, §1.8]; goldens updated only with an explicit `--bless` command and a visual-diff artifact on the PR.
4. **Fuzzing** of every parser and of recipe/XMP/Lua-table inputs.
5. **Preset fidelity suite** against Lightroom reference renders (private inputs; public metrics).
6. **Integration and protocol conformance** across shells; catalog upgrade tests (open catalogs from every released schema version).
7. **UI end-to-end** minimal, per shell.

**Release cadence.**
- *Phase 1 (library + CLI):* tagged `0.x` releases whenever the fidelity dashboard moves; semver for the recipe schema and CLI from `0.1`, with migrations.
- *Phase 2 (app):* nightly builds and a **monthly alpha** (Servo, Rerun), feature flags for unfinished work.
- *v1:* **two stable releases per year** (darktable's rhythm; Blender's three is too heavy for a small team) plus patch releases; consider an **LTS** only when studios or pipelines depend on us (Blender model: 2 years of fixes, no API change).
- *Compatibility promise* in `docs/RELEASES.md`: on-disk formats forever with migrations; embedding API semver with changelog; internal crate APIs unstable (§2.11).

### 4.8 Governance and funding model

- **Legal home:** a **fiscal host** first, a foundation later if warranted. Candidates (fees from §2.13): Software in the Public Interest (5% of gross), Open Collective Europe (about 6%), Open Source Collective (10%), Software Freedom Conservancy (10%, selective). Because grants (NLnet) have a European dimension and the Apple Developer account, SignPath and trademark must sit with an entity, prefer an **EU fiscal host** (Open Collective Europe) unless the maintainers are US-based [reasoning ours; [unverified] on host acceptance criteria and timelines].
- **Rules of the road (copy VideoLAN's trust capital):** no ads, no telemetry by default, no CLA, **DCO sign-off** (contributors keep copyright; licence split decided *now* because VLC's relicensing took years: SCOPE §1 says about 2.5 years and about 230 contributors; unverified here), written Code of Conduct, `SECURITY.md` with a disclosure address (essential for a parser-heavy product), trademark policy for "Ambrotype" after the trademark check.
- **Money model, in priority order:** (1) donations and sponsorship via the host (Open Collective, GitHub Sponsors); (2) **project grants for specific deliverables**, see §4.9; (3) **corporate memberships** on a Blender-style ladder once there are users (do not promise board seats; sell listing and a liaison, not control); (4) **store sales** of the same free binary on Mac App Store, Microsoft Store, Play Store, iOS App Store (Krita model, §1.5; relevant because MPL core + MPL shells are store-compatible); (5) optional later: a for-profit services entity à la Videolabs, with a clear firewall, if full-time maintainers are wanted [§1.1].
- **Decision rights:** a small steering group of maintainers; ADRs for architectural decisions; the fiscal host holds money and assets only.

### 4.9 First steps (next 90 days) and the grant reality check

1. **Week 0-2 (decisions):** accept this language verdict as ADR-0001; record the licence zones as ADR-0002; run the name/trademark check (SCOPE §12.6); create the public repo skeleton with `LICENSES/`, DCO bot, `deny.toml`, `rust-toolchain.toml`, CI Fast lane, empty crates with their zone and `forbid(unsafe_code)`.
2. **Week 1-4 (fiscal host):** apply to Open Collective Europe or SPI (Open Source Collective as fallback); once approved, reserve Apple bundle ids and the Apple Developer account in the host's name (08 §8.7), apply to SignPath Foundation.
3. **NLnet (go/no-go by mid-October):** the NGI Zero **Commons Fund is closed** (final call 1 June 2026) [verified: https://nlnet.nl/commonsfund/]; current NLnet calls are **Restack** and **CodeSupply**, deadline **3 Nov 2026 12:00 CET**, EUR 5k-50k, EU funded, "AI-related projects out of scope" for Restack unless over 1M users [verified: https://nlnet.nl/propose/, https://nlnet.nl/restack/]. Restack's text is about internet commons and open alternatives; **whether a RAW editor qualifies is uncertain**. Do not pitch "a Lightroom clone". A candidate pitch: *an open, memory-safe, fuzzed library and conformance suite for DNG/XMP/DCP/preset interoperability and a colour pipeline, usable by any open-source photo tool* [proposal, ours]. Ask NLnet (they invite contact before applying [background]) whether it fits before writing the application; a 5-50k grant for 3-6 months of one person's work on `amb-io` + `amb-presets` + the test corpus is the right scale. If Restack does not fit, skip this round rather than force it.
4. **Sovereign Tech Fund:** **not now.** It does not fund "user-facing applications ... or prototypes", needs budgets above EUR 50,000, and scores prevalence and vulnerability, i.e. libraries other projects already depend on [verified: https://www.sovereign.tech/programs/fund]. Revisit after a library (for example `amb-io`'s DNG/TIFF parser or `amb-color`) has external users; the Fellowship (individual maintainers) is a separate route [secondary: https://www.sovereign.tech/programs/fellowship].
5. **Security-funding angle:** Alpha-Omega and the GitHub Secure Open Source Fund (USD 12.5M announced March 2026) favour memory-safety and supply-chain work; a "memory-safe RAW/DNG parsing and fuzzing" story fits better than an app pitch [secondary: https://openssf.org/press-release/2026/03/17/linux-foundation-announces-12-5-million-in-grant-funding-from-leading-organizations-to-advance-open-source-security/; [unverified] eligibility].
6. **Month 2-3 (technical):** the Phase 0 spikes (06 §8.6) plus three new ones: (a) **registry + Backend trait** with two nodes on CPU and wgpu and the equivalence test (proves §4.2/§4.5); (b) **LibRaw sandbox** prototype and rawler camera-coverage comparison on a raw.pixls.us sample set; (c) **fuzz targets** for a TIFF/DNG IFD walker and an XMP parser, running in CI.
7. **Budget the first year realistically:** Apple Developer USD 99 per year; fiscal-host fees 5-10% of income; no cost for SignPath Foundation (OSS programme); real GPU CI runners are the main unfunded gap [unverified costs].

### 4.10 Reconciliation: what I would change in SCOPE and 06 (and why)

**Keep (the evidence supports them):** Rust; headless engine + CLI first; recipe as versioned data; wgpu with a CPU reference backend and equivalence tests; replaceable pixel-free UI shell; SQLite catalog with a single writer; MPL core + GPL desktop pack with a CI gate and DCO; native SwiftUI/Compose shells over UniFFI; permissive-only model manifest; fuzzing and golden images in the testing plan.

**Change or add:**

| # | Where | Change | Why (evidence) |
|---|---|---|---|
| 1 | 06 §8.1 crate list, SCOPE §9, Phase 1 | Split `amb-core` into `amb-recipe` (data, schema), `amb-buffer`, `amb-graph` (plan, cache, registry, `Backend`), `amb-nodes`, `amb-cpu`; add `amb-protocol` and `amb-testkit`; move CPU kernels out of `amb-core`. | darktable/GIMP show engine and schema must be usable without the pipeline or UI; 06 puts recipe, graph and CPU kernels in one crate, which forces every consumer (catalog, FFI, CLI) to build the kernels and makes leaf rebuilds slow (§3.6). |
| 2 | 06 §8, 08 §8.5/8.7 | Add an explicit **capability + priority registry** (VLC `set_capability` + `module_need`) for decoders, node implementations per backend, metadata, colour and AI backends, with user/test overrides. | 06/08 propose traits but no selection policy; VLC proves capability/priority selection scales to 500 modules and static builds (§1.1, §2.2). |
| 3 | 08 §8.3, 06 §2 | Define the GPU boundary as `Backend::run_node(node, tiles)` with wgpu confined to `amb-gpu`; allow native backends later. | mpv, Ruffle, Blender/OBS experience; keeps wgpu risk (quarterly breakage, adapter variance) contained (§2.5, §3.3b). |
| 4 | 06 §6 vs SCOPE §8 Phase 4 | Reconcile plugins: **no runtime code plugins before v1**; v1 = data plugins (presets, LUTs, recipes); Lua in Phase 4; WASM (WIT/wasmtime, desktop-only) after; **no dynamic Rust or public C plugin ABI**. | 06 says "Lua v1", SCOPE says Phase 4; Rust has no stable ABI; Zed shows WASM works; OBS shows C ABI is a maintenance tax (§2.2). |
| 5 | 06 §1.4, 08 §4 | **LibRaw behind a sandbox** (out-of-process on desktop, WASM where not) and `rawler` as registry default only after a coverage test; flag rawler's LGPL-2.1 against the iOS static-linking concern. | LibRaw has 60 CVE IDs and 2026 DNG/X3F overflows; Apple's DNG decoder bug was exploited in the wild (CVE-2025-43300); RLBox precedent (§3.2, §3.3). |
| 6 | SCOPE §9 (Testing), 06 §8.6 | Make **fuzz targets and OSS-Fuzz onboarding a Phase 1 deliverable**, plus `overflow-checks` and `forbid(unsafe_code)` policy and `SECURITY.md`. | Rust does not stop panics/OOM; parsers are the attack surface; FFmpeg/OSS-Fuzz track record (§2.9). |
| 7 | SCOPE (no funding section) | Add a **Governance and funding** section: fiscal host, DCO/no CLA, no ads/telemetry, grants **with the corrected NLnet and STF facts**: NGI Zero Commons Fund is closed; Restack deadline 3 Nov 2026, fit uncertain; STF excludes user-facing apps. | Owner's brief assumed NLnet is open; it is not for the Commons Fund (§4.9). |
| 8 | SCOPE §12.1 | Update "Licence: GPL-3.0-or-later (recommended) vs permissive" to the **split-licence decision** in §6; same for 06 §8.5. | Internal inconsistency: §6 recommends the MPL/GPL split, §12.1 and 06 §8.5 still recommend plain GPL. |
| 9 | 06 §1.2 | Update SIMD note: `std::simd` still nightly (confirmed), recommend `fearless_simd`/`wide`/`multiversion`; add `core::arch` guidance. | §3.1. |
| 10 | 06 §5, §8.4 | Add status flags: `ort` has no stable 2.0 (rc.13, Jul 2026); `uniffi` is 0.32 pre-1.0; wgpu breaks quarterly; pin versions and keep adapters thin. | §3.3b. |
| 11 | 06 §7, SCOPE Phase 2 | Add a **release and compatibility policy** (`docs/RELEASES.md`): three stability tiers; engine version stamped into recipes; monthly alpha, two stable per year, LTS later. | mpv client-API, Blender LTS, Rerun feature flags, darktable `params_t` migrations (§2.10, §2.11). |
| 12 | SCOPE Phase 0 | Add: ADR-0001 language, ADR-0002 licence zones, public skeleton repo with DCO/`deny.toml`/CI Fast lane, fiscal-host application, `SECURITY.md`, `rust-toolchain.toml` (stable). | Cheapest moment to fix irreversible items (§4.9). |
| 13 | 06 §3 / SCOPE §5 | No change to the Tauri-vs-Qt gate, but state that **the shell will be replaced at least once**, keep panels generated from the engine's parameter schema, and give the viewport a mpv-style "render into host-provided surface" contract. | GIMP 3.0 took 7 years for GTK3; darktable still GTK3; mpv render API (§1.5, §1.7). |
| 14 | 06 §8.1 ("tokio + rayon") | Keep tokio out of engine crates (service layer only); engine = rayon + channels + `Cancel`. | Mobile/WASM-hosted tests, simpler reasoning; dav1d/Stylo parallelism are data-parallel (§4.4) [judgement]. |
| 15 | 08 §8.1 | Confirm and extend the licence-zone layout with `crates/sys`, SPDX/REUSE checks and per-directory `LICENSE` (GEGL style). | GEGL ships exactly this split (§1.5). |

### 4.11 ASCII overview

```
 FRONT-ENDS (replaceable)                         one protocol, no pixels over it
 +-----------+ +-------------+ +-----------+ +--------------+ +--------------+
 | apps/cli  | | apps/desktop| | apps/ios  | | apps/android | | scripts / MCP|
 | `amb`     | | Tauri or Qt | | SwiftUI   | | Compose      | | local socket |
 | (reference| | GPL-3.0+    | | MPL-2.0   | | MPL-2.0      | | JSON cmds    |
 |  driver)  | | + gpl-pack  | |           | |              | |              |
 +-----+-----+ +------+------+ +-----+-----+ +------+-------+ +------+-------+
       |              |  PresentTarget, DisplayProfileProvider, AssetRef  |
       +--------------+------+--------------------+---------------------+
                              v
                  +-------------------------+      libvlc-like: stable, small
                  | amb-ffi  (C ABI+UniFFI) |      semver + changelog
                  | amb-protocol (DTOs,     |
                  |   param schema, JSON)   |
                  +------------+------------+
                               v
 crates/core (MPL-2.0)  --------------------------------------------------------
  +-----------+  +----------------------+  +--------------+  +---------------+
  | amb-app   |  | amb-graph            |  | amb-catalog  |  | amb-ai        |
  | jobs,undo |->| plan, ROI, cache keys|  | SQLite, XMP  |  | InferenceBack-|
  | settings, |  | scheduler, Cancel    |  | previews,    |  | end (ort)     |
  | Lua(later)|  | REGISTRY (capability |  | sync         |  +---------------+
  +-----------+  |  + priority, VLC)    |  +--------------+
                 | trait Backend        |
                 +-----+-------+--------+      +----------------------------+
                       |       |               | amb-recipe  (versioned     |
         +-------------+       +-----------+   |  data, crs registry,       |
         v                                 v   |  schema, migrations)       |
  +---------------+                +---------------+  amb-nodes (specs)      |
  | amb-cpu       |  equivalence   | amb-gpu       |  amb-buffer (types)     |
  | reference     |<-- tests ----->| wgpu + WGSL   |  amb-color amb-presets  |
  | rayon + SIMD  |   (CI, lavapipe| (only crate   |  amb-io (RawDecoder reg)|
  +---------------+   MOBILE_FLOOR)| naming wgpu)  +-------------+-----------+
                                   +---------------+             |
                                                   untrusted bytes v
 crates/sys + gpl-pack  ------------------------+----------------------------------
  | amb-sys-libraw (SANDBOXED: process / WASM)  |  amb-gpl-demosaic/denoise/upright |
  | amb-sys-lcms2   rawler adapter (LGPL check) |  amb-exiv2  amb-lensfun  (desktop)|
  +----------------------------------------------+-----------------------------------+
 CI gates: licence-zone ban list | forbid(unsafe) allow-list | fuzz per parser | goldens
```

---

## 5. Verification log and open items

**Verified in primary sources or in the cloned repos during this pass** (all dated 2026-10-03 unless noted): VLC tree layout, `set_capability`, static module table, plugin cache, Rust workspace and Meson `rust` option, CI job list, contrib recipes; mpv client API versioning, render API, `gpl` option, libplacebo dependency; OBS module API, struct-size registration, plugin list, presets and workflows; darktable IOP API, pixelpipe cache hash, GTK includes, `lightroom.c`, MCP binary, `dev-doc`; GEGL/babl layout and per-directory licence headers; Rerun, Zed, Bevy, Ruffle, wezterm, Lapce, Servo layouts and dependencies; libVLC LGPL press release (21 Dec 2011); dav1d design post; rav1d performance and cost posts; Google Android Rust post (13 Nov 2025); Stack Overflow 2025 usage numbers; NLnet Commons Fund closure, Restack and CodeSupply calls; STF eligibility; Rust 1.91 Tier 1 for Windows ARM64 (summary only); Talos and Red Hat LibRaw CVE pages; Debian LibRaw CVE count.

**Secondary only (search summaries, or pages not opened):** Blender Development Fund figures and LTS schedule; Blender Python API breakage; Krita Foundation structure; VLC relicensing details beyond the press release; Videolabs headcount; refused-offer amounts; Firefox Rust share and Stylo line counts; Microsoft 70% figure; OSS-Fuzz totals; wgpu release dates; ort/uniffi release states; Apple CVE-2025-43300 details; exiv2 and libwebp CVEs; OpenCollective/SPI fees; Servo governance history; WASI 0.3 and wasmtime status; Swift Android SDK; Zig status; Rust compile-time roadmap.

**Not found / explicitly unverified:** any VideoLAN or Kempf statement committing VLC to Rust beyond what the repo shows; the SCOPE figure of "about 2.5 years and about 230 contributors" for the VLC relicensing (not reproduced from a primary source here); current OBS sponsor list and legal structure; the final status of Blender's Vulkan default; whether Restack would accept a RAW-editor proposal; hosting-fee and acceptance criteria of EU fiscal hosts; real-GPU CI availability and cost; build-time numbers in §3.6 (estimates).

**Method note.** The repos were shallow-cloned (`--depth 1`) and only directory layout, build files, public headers and licence headers were inspected; no code was copied. Clone location: `scratchpad/arch/` (not committed).
