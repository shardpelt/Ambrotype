# Open-Source Raw Processing Ecosystem Survey

**Date:** 2026-10-03  
**Purpose:** Comprehensive analysis of open-source raw processing tools, libraries, and licensing to inform Flightbroom architecture decisions.

---

## A. RawTherapee: Architecture & Feature Analysis

### License
**GNU General Public License v3.0 (GPLv3)** – copyleft, all derivatives must use GPLv3.

### Architecture
RawTherapee consists of:
- **rtengine/** (~201,000 lines of C++): Core image processing engine
  - `improcfun.cc` (6,054 lines): Primary processing pipeline
  - `procparams.cc` (5,496 lines) + `procparams.h` (1,564 lines): Parameter definitions for all tools
  - `imagesource.h/.cc`: Raw decoding interface using LibRaw
- **rtgui/** (~174,000 lines): GTK+ GUI, metadata panel, profile management
- **Processing pipeline**: Modular, ordered execution of Demosaic → Color Management → Tools → Export

### Lightroom-Equivalent Tools
| Lightroom | RawTherapee File | Implementation Notes |
|-----------|------------------|----------------------|
| Exposure | `ipexposure.cc` | Tone mapping with curves |
| Whites/Blacks | `iptoneequalizer.cc`, `curves.cc` | Diagonal and flat curves |
| Shadows/Highlights | `ipshadowshighlights.cc` | Midtone-aware lifting/recovery |
| Contrast | Multiple (curves, vibrance) | Via tone curves and local adjustments |
| Clarity/Texture | `iplocalcontrast.cc` | Unsharp mask on multiple scales |
| Dehaze (Haze Removal) | `ipdehaze.cc` + `retinex.cc` | Retinex transmission-based haze removal |
| Local Adjustments | `iplocallab.cc` | Parametric + drawn masks (Locallab) |
| Vibrance | `ipvibrance.cc` | Color saturation enhancement |
| Saturation | Multiple | Per-hue and global saturation |
| Detail (Sharpening) | `ipsharpen.cc`, `ipsharpenedges.cc` | Edge-aware and capture sharpening |
| Noise Reduction | `impulse_denoise.cc` | Impulse/median filtering (limited; no ML) |
| Lens Correction | `perspectivecorrection.cc`, lensfun integration | Distortion, vignette, CA via lensfun |
| Perspective | `perspectivecorrection.cc` | Manual + auto (from ART/darktable) |
| Texture/Grain | `ipgrain.cc` | Film grain simulation |
| Film Simulation | `FilmSimulationParams` (procparams.h) | Lookup tables, tone curves, color rendering |
| Wavelet (Capture Sharpen) | `ipwaveletlevels.cc` | Multi-scale wavelet decomposition |
| Tone Curve | `curves.cc`, `diagonalcurves.cc`, `flatcurves.cc` | RGB, L*, CIECAM02 curves |
| CIECAM02/CAM16 | `cieimage.cc` | Advanced color appearance model |

### Demosaic Algorithms
- Bilinear (`bayer_bilinear_demosaic.cc`)
- VNG4 (`vng4_demosaic_RT.cc`)
- AHD (`ahd_demosaic_RT.cc`)
- EAHD (`eahd_demosaic.cc`)
- HPHD (`hphd_demosaic_RT.cc`)
- AMAZE (`amaze_demosaic_RT.cc`)
- Dual (`dual_demosaic_RT.cc`)
- X-Trans (`xtrans_demosaic.cc`)

### Lightroom XMP Import
- **Finding**: No evidence of direct CRS (Lightroom Camera Raw Settings) XMP tag mapping in rtengine/ or rtgui/
- **Metadata support**: Via `metadata.cc/h` and `exiv2` library bindings
- **User approach**: Manual conversion or third-party tools; RawTherapee uses its own `pp3` sidecar format

### Key Dependencies
- **LibRaw** (LGPL/CDDL dual) – RAW decoding
- **exiv2** (GPL v2 dual) – EXIF/IPTC/XMP handling
- **lensfun** (LGPL-3.0) – Lens profiles and distortion correction

---

## B. ART (Another RawTherapee)

### License
**GNU General Public License v3.0 (GPLv3)** – same as RawTherapee.

### Developer & Philosophy
- Maintained by **Alberto Griggio**
- Philosophy: "RawTherapee minus the bloat plus design vision"
- Removes obscure controls; adds new masking modes

### Key Features & Differences from RawTherapee
1. **Simplified Interface**: Cleaner tool organization; fewer options per tool
2. **Enhanced Metadata**: Better EXIF/IPTC/XMP handling with sidecar file support (.xmp files)
3. **Automatic Perspective Correction**: Adapted from darktable's algorithm
4. **ACES Color Science Support**:
   - **CLF (Color Look-up Tables)** support via OpenColorIO
   - **CTL (Color Transform Language)** scripting for custom color transforms
5. **Local Adjustments**: Multiple masking modes (drawn, parametric, etc.) for non-destructive edits
6. **Pixel Pipeline Compatibility**: Maintains darktable 4.0 compatibility for sidecar settings

### Technology Stack
- C++ (rtengine from RawTherapee)
- OpenColorIO (for ACES CLF LUTs)
- ACES CTL Interpreter (for color scripts)
- GTK+ or Qt GUI (platform-dependent builds)

### Platform Support
Windows, macOS, Linux

### Maturity & Activity
Actively maintained; mature for production use.

---

## C. Libraries & Frameworks

### C.1 Raw Decoding

| Library | License | Language | Purpose | Notes |
|---------|---------|----------|---------|-------|
| **LibRaw** | LGPL-2.1 / CDDL-1.0 (dual) | C++ | Complete RAW decoder | Industry standard; extensive camera support; liberal licensing |
| **rawspeed** | LGPL-2.0 | C++ | Fast RAW decoding | Developed by darktable; SIMD optimizations; ~50 codecs |
| **rawler** | LGPL-2.1 | Rust | RAW format extraction | Type-safe Rust bindings; used by dnglab; modern alternative |
| **rawloader** | LGPL-2.1 | Rust | Minimal RAW loader | Lightweight alternative to rawler |

**Recommendation**: LibRaw for C++ projects (established); rawler for Rust projects (safety + modern).

### C.2 Metadata & EXIF

| Library | License | Language | Purpose |
|---------|---------|----------|---------|
| **exiv2** | GPL-2.0+ (with commercial dual license available) | C++ | Complete EXIF/IPTC/XMP handling; command-line + library |
| **libexif** | LGPL-2.1 | C | Lightweight EXIF reading (no writing/IPTC) |
| **kamadak-exif** | MIT | Rust | Pure Rust EXIF parser (modern, no native deps) |
| **little_exif** | MIT | Rust | Minimal EXIF read/write for Rust |

**Note**: exiv2 is GPL, which propagates to any project using it; Rust alternatives (kamadak-exif, little_exif) are MIT and avoid GPL entanglement.

### C.3 Color Management

| Library | License | Language | Purpose |
|---------|---------|----------|---------|
| **LittleCMS2** | MIT | C | Fast ICC color space transforms; widely used |
| **OpenColorIO** | BSD-3 | C++ | Industry-standard color management (VFX/animation); ACES support |
| **lcms2 Rust bindings** | MIT | Rust | Safe Rust wrappers around LittleCMS2 |
| **qcms** | Mozilla Public License 2.0 | Rust | Mozilla's lightweight color management |
| **moxcms** | Apache-2.0 | Rust | Modern Rust color management library |

**Recommendation**: 
- For ACES support → OpenColorIO (BSD-3, permissive)
- For minimal overhead → LittleCMS2 (MIT)
- For Rust projects → lcms2 bindings (MIT) or moxcms (Apache-2.0)

### C.4 Lens Correction

| Library | License | Database License |
|---------|---------|------------------|
| **lensfun** | LGPL-3.0 | CC-BY-SA (community-maintained lens profiles) |

**Database Coverage**: 
- 2500+ camera/lens combinations (Canon, Nikon, Sony, Pentax, Fujifilm, etc.)
- Community-contributed profiles on GitHub
- Actively maintained; new lenses added regularly

**Use**: Distortion, chromatic aberration, vignetting correction; production-ready.

### C.5 Image I/O

| Library | License | Formats | Notes |
|---------|---------|---------|-------|
| **libjpeg-turbo** | BSD-3 / IJG | JPEG | High-speed JPEG codec |
| **libpng** | PNG License (permissive) | PNG | Standard PNG support |
| **libtiff** | BSD-like | TIFF | Lossless; used by RAW exporters |
| **libheif** | LGPL-2.1 | HEIF/HEIC | Apple-format image support |
| **libavif** | BSD-2 | AVIF | Modern lossy/lossless format (AV1) |
| **libjxl** | Apache-2.0 | JPEG XL | Next-gen format (lossless + lossy) |
| **OpenImageIO** | Apache-2.0 | 100+ formats | VFX-grade image I/O; metadata handling |

**Recommendation**: OpenImageIO (Apache-2.0) for comprehensive format support + metadata; libjpeg-turbo + libpng for individual codecs.

### C.6 Image Processing Frameworks

| Framework | License | Language | Architecture | GPU | Notes |
|-----------|---------|----------|--------------|-----|-------|
| **GEGL/babl** | LGPL-3.0 | C | Tile-based graph; color-space agnostic | Yes (OpenCL optional) | GIMP's processing engine; mature; academic heritage |
| **libvips** | LGPL-2.1 | C | Demand-driven, horizontal threading | GPU via CUDA plugin | High-performance image processing; low memory |
| **Halide** | MIT | Embedded DSL (C++) | Schedule-based optimization; JIT | CPU/GPU | Optimizes for parallelism + memory locality; used in production cameras |
| **vkdt** | MIT (developer assumed) | C + GLSL/Vulkan | Node graph DAG; all GPU | Vulkan | Real-time Vulkan-accelerated darkroom workflow; experimental |
| **OpenCV** | Apache-2.0 | C++ | Algorithmic library (not framework) | GPU (CUDA/OpenCL) | Computer vision focus; less suited for photography |

**Recommendation for Flightbroom**:
- **GEGL/babl** for declarative, node-based architecture (matches darktable/ART model)
- **Halide** for custom optimization of critical kernels (photography-grade image quality)
- **vkdt** (experimental): Vulkan GPU acceleration; reference for node-graph design

### C.7 DNG (Digital Negative) Support

**Adobe DNG SDK v1.4**
- **License**: Non-exclusive, worldwide, royalty-free license for SDK
- **Restrictions**:
  - Must include copyright notices in distributions
  - No trademark use without permission
  - Separately: Adobe grants royalty-free patent rights for "DNG Compliant Implementations"
- **Usage**: Library for reading/writing DNG-spec files; reference implementation
- **Alternative**: dnglab (Rust + rawler) for DNG conversion without Adobe SDK dependency

### C.8 AI/ML Denoising & Segmentation

| Model/Library | License | Purpose | Maturity |
|---------------|---------|---------|----------|
| **SAM2** (Segment Anything Model 2) | Apache-2.0 | Image/video segmentation; object detection for masking | Production-ready (Meta AI) |
| **MobileSAM** | Apache-2.0 | Lightweight SAM variant for mobile/embedded | Recent (2023) |
| **EfficientSAM** | Apache-2.0 | Efficient SAM for real-time segmentation | Recent |
| **SCUNet** | Apache-2.0 | Real-world blind image denoising | Academic (ICCV 2022) |
| **NAFNet** | ? (check original repo) | Simple yet effective denoising baseline | Academic (ECCV 2022) |
| **Restormer** | ? (check original repo) | Transformer-based image restoration | Academic |
| **Noise2Noise** | ? | Raw denoising framework | Academic reference |
| **Depth Anything V2** | Apache-2.0 | Monocular depth estimation for bokeh/focus stacking | Recent (2024) |
| **MiDaS** | MIT or Apache-2.0 (check repo) | Zero-shot depth estimation | Academic |
| **LaMa** | Apache-2.0 | Content-aware inpainting (remove objects) | Production (Samsung Research) |
| **ONNX Runtime** | MIT | Model inference framework (CPU + GPU) | Production-ready (Microsoft) |

**Integration Strategy**:
- Use ONNX for model inference (platform-agnostic)
- Bundle lightweight models (MobileSAM, EfficientSAM) for masking
- Optional: Include denoising models (SCUNet) as advanced feature
- Licensing: All Apache-2.0 or MIT; compatible with permissive project licenses

---

## D. Other Complete Applications

### D.1 darktable

- **License**: GPLv3 + LGPL components (GEGL/babl)
- **Architecture**: 
  - Modular processing pipeline
  - GEGL-based image operations
  - Node-graph compositing (darktable 4.0+)
- **Maturity**: Production-grade; 20+ years active development
- **Strengths**: 
  - Non-destructive workflow with advanced masking
  - High-quality color management (ACES support via OpenColorIO)
  - Extensive export formats
  - Tethering support
- **Weaknesses**: Complex UI; steep learning curve
- **Data**: https://www.darktable.org/, https://github.com/darktable-org/darktable (5.0.0 released Dec 2024)

### D.2 Ansel (darktable Fork)

- **License**: GPLv3
- **Maintainer**: Aurélien Pierre
- **Positioning**: "Darktable minus bloat plus design vision"
- **Key Differences**:
  - Streamlined UI with fewer tools
  - Redesigned tool layout for workflow
  - Pixel-pipeline compatible with darktable 4.0 (sidecars portable)
  - Philosophy: Usability-first; "no hidden options"
- **Maturity**: Alpha/nightly; active development
- **Data**: https://github.com/aurelienpierreeng/ansel

### D.3 digiKam

- **License**: GPL (free + open-source)
- **Focus**: **Digital Asset Management (DAM)**, not RAW processing
- **Key Features**:
  - Photo library management with tagging, collections, virtual albums
  - EXIF/IPTC/GPS metadata search
  - Batch operations
  - Light editing tools (not RAW-specific)
- **Maturity**: Mature; cross-platform (Linux, macOS, Windows since 2009)
- **Complementary to RAW processor**: Useful for post-processing workflow (culling, rating, export)
- **Data**: https://docs.digikam.org/

### D.4 Filmulator

- **License**: GPL v3
- **Approach**: Simulates film development (tone compression, local contrast)
- **Maturity**: Active but not widely used; development slower than alternatives
- **Unique**: Focus on film-like rendering pipeline

### D.5 LightZone

- **License**: GPL v3 (formerly commercial; open-sourced ~2012)
- **UI Innovation**: **Zone System** (Ansel Adams-inspired)
- **Approach**: Localized adjustments via zones (density-based masks)
- **Maturity**: Stable; niche following among fine-art photographers
- **Note**: UI concept valuable for reference; implementation legacy (Java)

### D.6 Photivo

- **License**: GPL v3
- **Focus**: RAW processor + bitmap editor
- **Integration**: GIMP-compatible plugin
- **Maturity**: Older; development minimal
- **Note**: Reference for 16-bit color workflow

### D.7 Shotwell & gThumb

- **License**: LGPL v2.1+
- **Purpose**: Light photo management + viewing
- **RAW Support**: Basic only; not intended for serious processing
- **Use Case**: Desktop file manager integration

### D.8 Siril

- **License**: GPL v3
- **Focus**: Astronomical image processing (stacking, alignment, deep-sky)
- **Not relevant** for general photography workflow

### D.9 RapidRAW (Modern Alternative)

- **License**: **AGPL-3.0** (more restrictive than GPLv3; applies to network use)
- **GitHub**: https://github.com/asimogo/rapidraw (also multiple forks: RapidRoom community build)
- **Stars**: ~10.3k
- **Activity**: Very active; releases every 1-2 months (as of early 2026)

**Positioning**: Modern Lightroom-like alternative; Rust + React + Tauri stack

**Technology Stack**:
- **Backend**: Rust (safety, performance)
- **GUI**: React (responsive, web-like UI)
- **Desktop Framework**: Tauri (lightweight ~30MB binary; cross-platform: Windows, macOS, Linux, Android)
- **GPU Processing**: WGPU + custom WGSL shaders
- **RAW Support**: rawler library (Rust, LGPL-2.1)

**Features** (non-destructive):
- Extensive tool set: Exposure, contrast, highlights/shadows, curves, saturation, color balance, clarity, detail enhancement, noise reduction, white balance, perspective crop
- **AI Masking**: Subject detection (SAM) + foreground detection
- Batch processing, presets, EXIF viewer
- Sidecar `.rrdata` files (JSON format) for non-destructive edits

**Maturity**: Beta → production-grade; rapidly maturing

**Design Lessons for Flightbroom**:
- Modern Rust stack is viable for performance-critical applications
- Tauri provides lightweight, cross-platform deployment
- WGPU enables GPU acceleration without OS-specific code (Vulkan/Metal/DX12 abstraction)
- Web-like React UI appeals to modern users (web designers comfortable with it)
- Sidecar JSON format simpler than binary formats

**CAVEAT**: AGPL-3.0 license means any SaaS deployment or network modifications must share source. **Not compatible** with proprietary commercial tools.

### D.10 Other 2024-2026 Lightroom Alternatives

**GitHub Search Results** ("Lightroom alternative" sorted by stars):
- **Darkroom** (mobile, iOS/Android focused) – less relevant for desktop RAW
- **Adobe-Alternatives repository** (9.6k stars) – curates darktable, ART, RawTherapee, etc.

**Conclusion**: No significant new pure-RAW-processor projects emerged 2024-2026 besides RapidRAW fork ecosystem. Existing projects (darktable, RawTherapee, ART, Ansel) remain dominant.

---

## E. Licensing Compatibility Matrix & Recommendation

### E.1 License Compatibility Chart

| License A | MIT | Apache-2.0 | GPLv2 | GPLv3 | AGPL-3.0 | LGPL-2.1 | LGPL-3.0 |
|-----------|-----|-----------|-------|-------|----------|----------|----------|
| **MIT** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Apache-2.0** | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| **GPLv2** | ✓ | ✗ | ✓ | ✗ | ✗ | ~* | ✗ |
| **GPLv3** | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ | ✓ |
| **AGPL-3.0** | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ | ✓ |
| **LGPL-2.1** | ✓ | ✓ | ~ | ✓ | ✓ | ✓ | ✓** |
| **LGPL-3.0** | ✓ | ✓ | ✗ | ✓ | ✓ | ✓** | ✓ |

**Legend**:
- ✓ = Compatible (can link/use together)
- ✗ = Incompatible (cannot combine)
- ~ = Complex; requires care (GPLv2 + Apache-2.0 problematic due to patent clause)
- * = LGPL is more permissive than GPL; LGPL-2.1 + GPLv3 OK if GPLv3-compatible LGPL

### E.2 Key Rules

1. **Permissive (MIT/Apache-2.0) → Copyleft (GPL/AGPL)**
   - You CAN include MIT/Apache-2.0 code in GPLv3 projects; result is GPLv3
   - You CANNOT include GPLv3 code in MIT/Apache-2.0 projects

2. **Apache-2.0 + GPLv2 = INCOMPATIBLE**
   - Apache-2.0's explicit patent clause conflicts with GPLv2's patent provisions
   - Apache-2.0 + GPLv3 = **Compatible**

3. **AGPL vs GPL**
   - AGPL-3.0 is GPLv3 + "network copyleft" (SaaS loophole closure)
   - Cannot use AGPL code in GPLv3-only project unless dual-licensed

4. **LGPL Flexibility**
   - LGPL-2.1/3.0 allows dynamic linking from proprietary code
   - Upstream (LGPL) libraries remain under LGPL; downstream code can be proprietary
   - BUT: Static linking typically triggers GPL terms

### E.3 Recommended License for Flightbroom

#### Option 1: **Apache-2.0** (Recommended)

**Rationale**:
- ✓ Permissive; allows commercial/proprietary derivatives
- ✓ Compatible with GPLv3 (and thus darktable, RawTherapee, ART, Ansel)
- ✓ Compatible with MIT libraries (Halide, LittleCMS2, lensfun, SAM2, LaMa)
- ✓ Compatible with LGPL libraries (libvips, GEGL/babl, OpenImageIO)
- ✓ Can link RapidRAW code (AGPL-3.0) IF designed as plugin/external tool (not derivative work)
- ✓ Explicit patent clause (useful for VFX/commercial use)
- ✓ Modern standard (OpenImageIO, ONNX, Halide, Google/CNCF projects)

**Compatibility Summary**:
- Can use: LibRaw, rawspeed, rawler (all LGPL/LGPL-2.1) ✓
- Can use: exiv2 (GPL-2.0) if dynamically linked / not distributed as derivative ✓*
- Can use: lensfun (LGPL-3.0) ✓
- Can use: OpenColorIO (BSD-3), LittleCMS2 (MIT) ✓
- Can use: GEGL/babl (LGPL-3.0) if via plugin model ✓
- Can use: Halide (MIT), libvips (LGPL) ✓
- Can use: OpenImageIO (Apache-2.0), libjpeg-turbo (BSD-3) ✓
- Can use: ONNX Runtime (MIT), SAM2 (Apache-2.0), LaMa (Apache-2.0) ✓
- **Cannot** use RapidRAW code directly (AGPL-3.0 → would require AGPL); can reference design patterns

#### Option 2: **GPLv3** (Also Viable)

**Rationale**:
- ✓ Can use all above libraries (more permissive can be used in copyleft)
- ✓ Can link RapidRAW code; result is GPLv3
- ✓ Aligns with darktable, RawTherapee, ART (ecosystem consistency)
- ✗ Restricts derivatives to GPLv3 (limits commercial adoption)
- ✗ Apache-2.0 projects cannot use your code without re-licensing (one-way)

**Use case**: If Flightbroom is community project with no commercial intent.

#### Option 3: **AGPL-3.0** (Not Recommended)

- **Pro**: Strongest "freedom" guarantee; prevents commercial SaaS exploitation
- **Con**: 
  - Makes derivatives must also be AGPL
  - Breaks compatibility with Apache-2.0 projects
  - Deters corporate contributions
  - Network clause adds complexity (when does derivative use trigger sharing?)

**Use case**: Only if Flightbroom explicitly opposes proprietary derivatives.

### E.4 Final Recommendation

**→ Use Apache-2.0**

**Reasoning**:
1. **Maximum compatibility**: Works with entire OSS ecosystem (permissive, GPL-family, AGPL-possible)
2. **Commercial viability**: Allows downstream proprietary derivatives (Lightroom-killer potential)
3. **Industry standard**: Used by OpenImageIO, ACES ecosystem, Google, CNCF
4. **Modern practice**: Clear, explicit patent language (valuable if facing patent litigation)
5. **Ecosystem fit**: RawTherapee/darktable will not block Apache-2.0 projects; users can mix both

**Exception**: If Flightbroom is positioned as "purely community, never commercial," GPLv3 is defensible.

---

## F. Summary: Technology Stack Recommendations

### Foundational Libraries (Locked-In Choices)

| Component | Recommendation | License | Rationale |
|-----------|-----------------|---------|-----------|
| **RAW Decoding** | LibRaw (C++) OR rawler (Rust) | LGPL/MIT | Comprehensive; mature |
| **Metadata** | exiv2 (if C++) OR kamadak-exif (if Rust) | GPL-2.0 / MIT | EXIF/IPTC/XMP support |
| **Color Management** | OpenColorIO (ACES support) + LittleCMS2 (fallback) | BSD-3 + MIT | Industry standard + lightweight |
| **Lens Correction** | lensfun | LGPL-3.0 | Comprehensive database |
| **Image I/O** | OpenImageIO | Apache-2.0 | 100+ formats; metadata |
| **GPU Acceleration** | WGPU (Rust) or Vulkan (C++) | MIT/Apache-2.0 | Cross-platform |
| **Node-Graph Processing** | GEGL (C) or custom (C++/Rust) | LGPL-3.0 | Proven architecture |
| **AI Masking** | SAM2 via ONNX Runtime | Apache-2.0 + MIT | Modern, modular |
| **Denoising (Optional)** | SCUNet weights + ONNX Runtime | Apache-2.0 | High-quality; optional feature |

### Key Reference Implementations

- **Architecture**: Study Ansel (simplified darktable fork), ART (ACES + CTL support)
- **UI/UX**: RapidRAW (modern web-like React UI), darktable (node graph), LightZone (zone system)
- **GPU Pipeline**: vkdt (Vulkan node graph), RapidRAW (WGPU shaders)

---

## Sources

- [RawTherapee Repository](https://github.com/Beep6581/RawTherapee)
- [RawTherapee Wikipedia](https://en.wikipedia.org/wiki/RawTherapee)
- [ART (Another RawTherapee) GitHub](https://github.com/aurelienpierreeng/ansel)
- [ART - LinuxLinks](https://www.linuxlinks.com/art-fork-rawtherapee/)
- [darktable Official](https://www.darktable.org/)
- [darktable GitHub](https://github.com/darktable-org/darktable)
- [Ansel GitHub](https://github.com/aurelienpierreeng/ansel)
- [digiKam Manual](https://docs.digikam.org/)
- [LibRaw Documentation](https://www.libraw.org/docs)
- [RawSpeed GitHub](https://github.com/darktable-org/rawspeed)
- [Rawler Crate](https://crates.io/crates/rawler)
- [exiv2 GitHub](https://github.com/Exiv2/exiv2)
- [lensfun Manual](https://lensfun.github.io/manual/latest/)
- [lensfun GitHub](https://github.com/lensfun/lensfun)
- [OpenColorIO GitHub](https://github.com/AcademySoftwareFoundation/OpenColorIO)
- [LittleCMS Official](https://www.littlecms.com/)
- [GEGL Documentation](https://developer.gimp.org/api/gegl/)
- [babl Official](https://gegl.org/babl/)
- [libvips GitHub](https://github.com/libvips/libvips)
- [OpenImageIO GitHub](https://github.com/AcademySoftwareFoundation/OpenImageIO)
- [OpenImageIO Documentation](https://openimageio.readthedocs.io/)
- [Halide Official](https://halide-lang.org/)
- [vkdt GitHub](https://github.com/hanatos/vkdt)
- [RapidRAW GitHub](https://github.com/asimogo/rapidraw)
- [RapidRAW Review - The Phoblographer](https://www.thephoblographer.com/2025/07/09/rapidraw-an-open-source-alternative-to-adobe-lightroom/)
- [SAM2 Paper](https://arxiv.org/html/2408.00714v1)
- [SAM2 GitHub](https://github.com/facebookresearch/segment-anything-2)
- [LaMa GitHub](https://github.com/ReProduceByYQ/lama-cleaner)
- [ONNX Runtime](https://github.com/microsoft/onnxruntime)
- [Adobe DNG SDK License](https://www.adobe.com/support/downloads/dng/dng_sdk_eula_win.html)
- [License Compatibility Checker - FOSSA](https://fossa.com/resources/devops-tools/license-compatibility-checker)
- [Understanding OSS Licenses - DEV Community](https://dev.to/juanisidoro/open-source-licenses-which-one-should-you-pick-mit-gpl-apache-agpl-and-more-2026-guide-p90)

