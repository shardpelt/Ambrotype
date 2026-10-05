# RapidRAW survey (for a Lightroom-preset-compatible open-source editor)

Snapshot: clone at `scratchpad/RapidRAW`, HEAD "Merge pull request #1813" dated 2026-10-02. Paths below are relative to the repo root.
Method: targeted grep/sed. `preset_converter.rs` read in full; the main shader read for the `main()` pipeline, tonal, colour and exposure functions. Other files were skimmed, so treat details marked "not verified" as open.

## 1. Project facts

- **License:** AGPL-3.0-only (`LICENSE`, the unmodified FSF text; README badge, `README.md:11`). There is no CLA and no CONTRIBUTING file. `README.md:974-980` has only a short "Contributing" blurb, and `.github/` holds CODEOWNERS, FUNDING, issue templates, a PR template and workflows. Contributions therefore come in under inbound=outbound AGPL, and the sole author keeps no relicensing right by contract. This should be confirmed with the maintainer.
- **Author:** CyberTimon (Timon Käch), `src-tauri/Cargo.toml:4`. Essentially a solo maintainer with community PRs. The README credits Google AI Studio for help writing image-processing algorithms (`README.md:983`), so code provenance is partly LLM-assisted.
- **Activity:** very high. The README changelog (`README.md:63-142+`) has entries nearly every few days, from 2026-05 through 2026-09-17, and the last merge was 2026-10-02. Recent features include film emulation (Spektrafilm, V-Log scene-referred LUTs), tethering, focus stacking, a liquify tool, culling, multi-root library and Android.
- **Platforms:** Windows, macOS, Linux and Android (`README.md:24`). Tauri 2, with the Linux WebKitGTK/Wayland-NVIDIA workarounds in `Cargo.toml`.
- **Size:**

| Part | Lines |
|---|---|
| Rust (`src-tauri/src`) | about 37.4k, in 45 files; the largest are `file_management.rs` 4.3k, `image_processing.rs` 3.5k, `export_processing.rs` 2.3k, `lib.rs` 2.3k, `gpu_processing.rs` 2.2k, `focus_stacking.rs` 2.2k |
| WGSL (`src-tauri/src/shaders`) | about 2.6k: `shader.wgsl` 1997, `flare.wgsl` 410, `blur.wgsl` 83, `display.wgsl` 77 |
| TS/TSX (`src`) | about 54.8k |

- **Key dependencies (`src-tauri/Cargo.toml`):**
  - Raw decoding is `rawler`, from a CyberTimon fork, `RapidRAW-DngLab` (line 28). It is the dnglab crate.
  - GPU is `wgpu` 29 (line 17, pinned to avoid P3 colour shifts on Apple).
  - ML runtime is `ort` 2.0.0-rc.10 with `load-dynamic` (line 31).
  - Other crates: `image`, `imageproc`, `nalgebra`, `glam`, `rayon`, `kamadak-exif` and `little_exif`, `jxl-oxide`/`jxl-encoder`, `mozjpeg-rs`, `webp`, `image-hdr`, `quick-xml`, `tokenizers` (CLIP tagging) and `gphoto2` (optional tethering).
  - macOS has a `CIRAWFilter` binding (`apple_raw.rs`) for Apple RAW.
  - There is no `lcms2`/ICC crate and no `moxcms`. Colour management is hand-rolled (see section 2).

## 2. Processing pipeline

### Raw to linear (`src-tauri/src/raw_processing.rs`)

- `develop_internal` (109-270) uses `rawler::get_decoder`, then `RawDevelop` with the default steps minus `SRgb` (166-178). That leaves rawler's own black/white-level handling, demosaic (bilinear/"Speed" for fast previews, otherwise rawler's default), white balance from the camera coefficients, and `Calibrate` (camera matrix to XYZ to RGB). The output is linear float.
- Camera colour is therefore whatever rawler's embedded matrices give. These come from the dnglab/rawler camera database, effectively a single-illuminant matrix per camera. No DCP, no dual-illuminant interpolation and no Adobe Camera Matching profile is used (grep for dcp/icc/profile in the Rust sources found nothing relevant).
- Linear data is rescaled to a `u32` range, then clamped to 1.0 for the fast path or 1000.0 for full quality (186-215). The highlight-compression argument is ignored (`_highlight_compression`, line 112). Recent commits add highlight colour reconstruction (`recover_clipped_pixel`, line 61) and README items "Improve RAW highlight recovery" (2026-09-13).
- Non-raw inputs are treated as sRGB and linearised in the shader (`shader.wgsl` main, `is_raw == 0u`).
- Multi-exposure Canon WB neutralisation is handled in `multi_exposure.rs`.

### Working space

- Linear, unbounded scene-referred RGB. The `rawler` Calibrate output is probably sRGB/Rec.709 primaries. The luma constants are Rec.709 (`shader.wgsl:216`).
- Display and export are sRGB only. There is no wide-gamut working space (ProPhoto) and no output ICC selection. Not verified for export, but no ICC crate exists.
- Colour-grading and AgX code references V-Log and other spaces (`gpu_processing.rs`, `shader.wgsl:248,1366-1432`).

### GPU compute (`gpu_processing.rs`, `shaders/shader.wgsl`)

- A single compute pass, `main()` at `shader.wgsl:1701-1997`, with precomputed blur textures for sharpen, tonal (shadows/blacks), clarity, structure and dehaze (`blur.wgsl`), plus a flare texture.
- Large images are processed in tiles (`tile_offset_x`). `scale` is `min(w,h)/1080`, so radii are resolution-relative.
- **Order of operations in `main`:**
  1. CA correction (the `apply_ca_correction` function, line 1336).
  2. sRGB to linear if not raw.
  3. Global slider values plus mask contributions, summed per pixel as `global + Σ(mask_value × influence)`. Local adjustments are therefore additive in slider space, not layered.
  4. Noise reduction (`apply_noise_reduction`, 1148).
  5. Sharpen, then clarity, then structure, then "centre" local contrast.
  6. Linear exposure (`color * 2^EV`, line 599).
  7. Optional glow, halation and flare.
  8. Dehaze.
  9. White balance (a simple per-channel multiplier, 687).
  10. Tonal adjustments: contrast, shadows, whites and blacks (390). Contrast uses a gamma-2.2 pivot, S-curve with strength 2^(1.25·c).
  11. Highlights (478).
  12. Colour calibration (649).
  13. HSL (733).
  14. Global hue shift.
  15. Saturation and vibrance (695).
  16. Colour grading (791).
  17. Per-mask colour grading.
  18. Vignette.
  19. Tone mapper, in one of three modes:
     - `toneMapper: 'basic'`: for raw, linear to sRGB, then a gamma-1.1 lift and a 0.75 smoothstep S-curve (1904-1912); for non-raw, plain sRGB.
     - `agx`: the AgX transform (`agx_full_transform`, 1427).
     - `legacy_tonemap`: ACES-ish fit (a=2.51…) at 1435. The `legacy` mode name was not confirmed against the UI.
  20. Scene-referred LUT, via V-Log then tetrahedral sampling.
  21. "Brightness" filmic curve (606).
  22. Curves in display-referred sRGB (`apply_all_curves`, 1477), then display-referred LUTs.
  23. Grain.
  24. Dither, then clamp to [0,1].

### Per-adjustment notes and differences from Adobe semantics

| Adjustment | Implementation | Divergence from Adobe |
|---|---|---|
| Exposure | Linear `2^(EV)`. Slider is divided by `SCALES.exposure = 0.8` in `image_processing.rs:1676`, so slider +1 ≈ +1.25 EV. | Adobe is EV, but with a highlight shoulder and a different baseline tone curve. |
| Contrast | Gamma-2.2 symmetric power S-curve about 0.5, bypassed above 1.0 (390-470). | Adobe pivots differently and is not symmetric in linear light. |
| Highlights | Spatial. Uses a local luma pyramid with a pivot at 0.10 (478-598). | Adobe's PV2012 is also local, but the curves differ and no matching has been done. |
| Shadows / Blacks | Luma power-law lift with a local-detail correction (400-450), `SCALES` shadows 120, blacks 40. | Reworked 2026-06-29. Not Adobe's maths. |
| Whites | A linear gain, `1/(1 - 0.25·w)` (400-410). | Adobe's whites move a white point inside a tone curve. |
| Clarity / Texture | Blur-based local contrast (824-884); Clarity scale 125, Texture is mapped to `structure`. | Different kernels from Adobe; Texture's frequency band is not matched. |
| Dehaze | Blur-based (1107), scale 750. | Not a dark-channel prior, as far as the 1107-1147 range shows. |
| Vibrance / Saturation | HSV-based with a skin-hue dampener at 25 degrees (695-731). | Different from Adobe's. |
| HSL | 8 ranges. `HSL_RANGES` at 186, influence at 298. `hue` multiplier is 0.3, sat and lum divide by 100 (`image_processing.rs:1707-1709`). | Colour-space and falloff differ from Adobe's. |
| Colour grading | 3-way plus global, sat/lum divided by 500, balance by 200 (`SCALES`). | Adobe's blending and balance are not replicated. |
| Curves | 16-point Hermite splines on sRGB-encoded data, with a parametric-curve mode added in the model (`INITIAL_ADJUSTMENTS`). | Adobe's PV2012 curve acts in a different space. |
| Sharpening | Multi-scale, with detail and masking (`apply_sharpen`, 920). | Different from Adobe's radius, detail and masking. |
| Noise reduction | Luma and colour NR in the shader (1148). The AI denoise is separate (NIND UNet). | Not Adobe's NR. |
| Vignette | Post-crop, power-shaped ellipse. Positive amount blends to white. | Not Adobe's PostCropVignette maths. |
| Grain | Gradient noise masked by luma, plus size and roughness. | Different texture from Adobe's. |
| White balance | Per-channel multipliers, temp ×0.2/0.05/-0.2, tint ×0.25 (687-693). The UI uses -100..100 offsets, **not Kelvin**. | Not a true chromatic-adaptation transform. |

- The multi-mask approach sums slider values per pixel before the tone chain runs, a simplification of Lightroom, where each local adjustment is its own layer.
- The tone chain runs in linear light, with the contrast, curves and filmic steps at display-referred points. There is no Adobe-style baseline tone curve or profile. The "basic" tone mapper is homemade, so unedited raw output will not match Adobe Color, Adobe Standard, or camera JPEG look.

## 3. Lightroom preset converter (`src-tauri/src/preset_converter.rs`, 349 lines)

**Entry:** `convert_xmp_to_preset(&str) -> Preset` (93). It is called from `file_management.rs:3165-3196`, where `parse_preset_file` accepts `.xmp` and `.lrtemplate`. The importer UI lists `rrpreset, xmp, lrtemplate` (`src/components/panel/right/PresetsPanel.tsx:1038`).

**How it parses:** it joins lines and extracts `crs:Key="value"` attributes with one regex (`96`). There is no XML parser, so attributes in element form are missed. Curves are read from `<crs:Name><rdf:Seq><rdf:li>x, y</rdf:li>` blocks (44-91). The preset name comes from `<crs:Name>…<rdf:Alt>` (38-42), falling back to "Imported Preset".

**.lrtemplate support** is a regex hack: `s.xmp = "(.*)"`, then `\"` is replaced by `"` (`file_management.rs:3181-3190`). That works only for lrtemplate files that embed an XMP string. Real `.lrtemplate` files use a Lua table (`s = { id=…, value = { settings = { ToneCurvePV2012 = {…}, … } } }`) and are not parsed. If the regex fails, the file is passed through as raw text and the converter finds no attributes, giving an empty preset. This was not tested against sample files.

### Mapping table (from `preset_converter.rs:107-138` unless noted)

| crs: key | RapidRAW key | Transform |
|---|---|---|
| Exposure2012 | exposure | none (the shader scales by 1/0.8) |
| Contrast2012 | contrast | none |
| Highlights2012 | highlights | none |
| Whites2012 | whites | none |
| Blacks2012 | blacks | none |
| Shadows2012 | shadows | `min(v×1.5, 100)` (153-156). Negative values are not clamped at -100 (extra positives clipped only). |
| Clarity2012 | clarity | none |
| Dehaze | dehaze | none |
| Vibrance | vibrance | none |
| Saturation | saturation | none |
| Texture | structure | none |
| Sharpness | sharpness | `v/150×100`, clamped 0..100 (158-164) |
| SharpenRadius | sharpenRadius | none (key not seen in `INITIAL_ADJUSTMENTS` or SCALES, so it is likely a no-op) |
| SharpenDetail | sharpenDetail | none (likely a no-op, same reason) |
| SharpenEdgeMasking | sharpenMasking | none (likely a no-op, same reason) |
| LuminanceSmoothing | lumaNoiseReduction | none |
| ColorNoiseReduction | colorNoiseReduction | none |
| ColorNoiseReductionDetail | colorNoiseDetail | none (likely a no-op) |
| ColorNoiseReductionSmoothness | colorNoiseSmoothness | none (likely a no-op) |
| ChromaticAberrationRedCyan / BlueYellow | chromaticAberration* | none (these are defringe amounts in Adobe, not the same quantity as the shader's CA offsets) |
| PostCropVignetteAmount / Midpoint / Feather / Roundness | vignette* | none |
| GrainAmount / GrainSize / GrainFrequency | grainAmount / grainSize / grainRoughness | none (Adobe Frequency is not roughness) |
| ColorGradeBlending | colorGrading.blending | none |
| Temperature (+ AsShotTemperature) | temperature | mired-delta: `-(1e6/T - 1e6/T_asshot)/150×100`, clamped ±100; as-shot defaults to 5500 K (166-178) |
| Tint | tint | `v/150×100`, clamped ±100 |
| HueAdjustment{Red..Magenta} | hsl.{reds..magentas}.hue | `×0.75` (202) |
| SaturationAdjustment* | hsl.*.saturation | none |
| LuminanceAdjustment* | hsl.*.luminance | none |
| SplitToningShadowHue / Saturation | colorGrading.shadows.hue / saturation | none |
| SplitToningHighlightHue / Saturation | colorGrading.highlights.hue / saturation | none |
| SplitToningBalance | colorGrading.balance | none |
| ColorGradeMidtoneHue / Sat / Lum | colorGrading.midtones.* | none |
| ColorGradeShadowLum / HighlightLum | colorGrading.shadows / highlights.luminance | none |
| ColorGradeGlobalHue / Sat / Lum | colorGrading.global.* | none |
| ToneCurvePV2012 (+Red/Green/Blue) | curves.luma / red / green / blue | points copied as `{x,y}` in 0-255. On the luma curve only, shadow lifts below x=64 are dampened by 0.8→1.0 (60-77) |

**Missing from the table:** ToneCurve (non-PV2012 legacy), ParametricShadows/Darks/Lights/Highlights/Split points, Brightness, ConvertToGrayscale, GrayMixer*, Calibration (`ShadowTint`, `RedHue`, `RedSaturation`, …), all `LensProfile*`/`Lens*` keys, Defringe, `ProcessVersion`, `CameraProfile`, `Look`/`LookTable`, `RGBTable`/`Table_*` (look LUTs), `ToneMapStrength`, `OverrideLook`, `HDREditMode`, `Sharpness` mask extras, `LocalContrast`, `EnableCalibration`, crop and `Gradient`/`Radial`/`Correction` masks, `AutoTone`, and `WhiteBalance` as shot or "Auto".

**What happens to each ignored tag:**
- **White balance:** only if `Temperature` is present. A preset with `WhiteBalance="As Shot"` and no Temperature sets nothing. A Temperature with no `AsShotTemperature` uses 5500 K as the reference, which is wrong for most images. Note Lightroom stores relative `Temperature`/`Tint` only for raw files in some presets and absolute values for others, and this code treats it as absolute against an as-shot reference.
- **Camera profile / Look / LUTs (`Look`, `RGBTable`):** ignored. This is a major loss. Most commercial presets embed a profile table, the colour-look carrier.
- **Masks and crop:** `include_masks: false` and `include_crop_transform: false` are always set (`341-348`), and the XMP mask elements are not parsed.
- **Colour calibration:** ignored, although the shader has `apply_color_calibration` (649). It is not mapped from crs: keys.
- **Parametric tone curve:** ignored. The app has a parametric curve model, but there is no mapping.
- **B&W:** `ConvertToGrayscale` and `GrayMixer*` ignored.
- **Process version:** the mapping assumes PV2012 or newer; PV2003/2010 keys are not handled.

### Fidelity assessment

- Numerically it is a **best-effort slider remap**, not a model-matched reimplementation. Three arbitrary factors (×1.5 shadows, ×0.75 hue, curve shadow dampening) show that the fit was done by eye.
- The inconsistencies are structural: Adobe's PV2012 maths for highlights, shadows, whites, blacks, contrast, clarity, texture, dehaze, vibrance, saturation and HSL cannot be reproduced by rescaling a differently-constructed pipeline.
- The base rendering also differs (no Adobe baseline tone curve, no camera profile, different tone mapper, no wide-gamut working space), so even identical slider values give a different image.
- Expected result: recognisable mood for simple presets (exposure, contrast, curve, split-toning, and HSL tweaks). Presets that rely on profiles/LUTs, calibration, local masks, strong HSL grading, or the shadows/blacks/whites interplay will deviate visibly. I would not estimate a numeric colour error without test images, but it is likely well above a visible difference threshold (dE greater than 3) for many commercial presets.
- There is no test suite or reference comparison for the converter. Neither the file `preset_converter.rs` nor the repo has a `tests` module for it, apart from `bench/`, which holds replay and perf tooling.

## 4. Adjustment parameter model

Defined in TS: `src/utils/adjustments.ts:515` (`INITIAL_ADJUSTMENTS`). It is stored as a JSON blob and parsed by `image_processing.rs` straight from `serde_json::Value` (`adjustments["x"].as_f64()`). Presets are `{ presets: [ {id, name, adjustments, includeMasks?, includeCropTransform?, presetType?} | {folder} ] }` (`file_management.rs:206-244`, `.rrpreset`). The `style` type is the preset type used by the converter. Section visibility is a field (`sectionVisibility`).

| Group | Fields | Range / notes |
|---|---|---|
| Tone | exposure, brightness, contrast, highlights, shadows, whites, blacks | sliders about -100..100 (exposure in the same slider scale; `SCALES` 0.8-120) |
| Presence | clarity, structure, dehaze, centré (sic) | -100..100 |
| Colour | temperature, tint, saturation, vibrance, hue | -100..100 |
| HSL | hsl.{reds, oranges, yellows, greens, aquas, blues, purples, magentas}.{hue, saturation, luminance} | -100..100; 8 bands |
| Colour grading | colorGrading.{shadows, midtones, highlights, global}.{hue 0-360, saturation, luminance}, blending, balance | sat/lum 0..100 |
| Calibration | colorCalibration.{red,green,blue}{Hue,Saturation}, shadowsTint | |
| Curves | curves / pointCurves {luma, red, green, blue}: ≤16 points (0-255), parametricCurve, curveMode | |
| Detail | sharpness, sharpnessThreshold (default 15), lumaNoiseReduction, colorNoiseReduction | |
| Effects | vignette{Amount, Midpoint 50, Feather 50, Roundness 0}, grain{Amount, Size 25, Roughness 50}, glowAmount, halationAmount, flareAmount | |
| LUT | lutPath, lutName, lutData, lutSize, lutIntensity (100), lutIsSceneReferred | .cube/.3dl/.png/.jpg/.tiff LUTs (`lut_processing.rs`) |
| Tone mapper | toneMapper `'basic'`, and others (AgX) | per-image |
| Geometry | transform{Distortion, Vertical, Horizontal, Rotate, Aspect, Scale, XOffset, YOffset}, crop, rotation, orientationSteps, flipHorizontal/Vertical, guidedPerspective | |
| Lens | lensCorrectionMode, lensMaker, lensModel, lens{Distortion,Vignette,Tca}{Amount,Enabled}, lensDistortionParams | |
| CA | chromaticAberrationRedCyan, chromaticAberrationBlueYellow | the shader divides by 10000, so these are offsets |
| AI / retouch | masks[], aiPatches[], lensBlur{…} | |

The model is **flat and has no Adobe-native semantics**: no process version, camera profile, or look.

## 5. Masks and AI

- **Mask types** (`mask_generation.rs:1257-1321`): radial, linear, brush, flow, colour range, luminance range, `all`, plus the `ai-subject`, `ai-foreground`, `ai-sky`, `ai-depth` and `quick-eraser` types. Clone, heal, liquify and retouch are brush-type tools. Sub-masks can be additive, subtractive or intersect (`SubMaskMode`, 21; applied at 1352-1364). Each mask has its own adjustment set (`MaskAdjustments` in the shader, line 120).
- **Models** (`ai_processing.rs:22-59`), downloaded on demand from Hugging Face `CyberTimon/RapidRAW-Models` with SHA-256 verification for at least some (`CLIP_MODEL_SHA256`, line 47; `download_and_verify_model`, 408). They are stored in the app-data `models/` dir (309) and run through ONNX Runtime (`ort`, `load-dynamic`, so the user or installer must supply the runtime library):

| Purpose | Model | Upstream licence (from general knowledge, not checked in-repo) |
|---|---|---|
| Subject / click segmentation | SAM ViT-B encoder and decoder (README also credits SAM 2) | Apache-2.0 |
| Foreground | U²-Net (`u2net.onnx`) | Apache-2.0 |
| Sky | `skyseg-u2net.onnx` | not known; its origin needs checking |
| Depth | Depth Anything V2 Small | Apache-2.0 (the Small variant) |
| Inpainting | LaMa fp16 | Apache-2.0 |
| Denoise | NIND UNet (`nind_denoise_utnet_684.onnx`) | not verified; the NIND dataset licence needs checking |
| Tagging / search | CLIP (+ tokenizer) | MIT (OpenAI CLIP); the exact export is unverified |

  The licences on the RapidRAW-hosted re-exports have not been verified and must be audited before reuse.
- **Generative inpainting:** an optional cloud path (`ai_connector.rs:193 process_cloud_inpainting`, `CloudInpaintRequest` with prompt and mask) sends images to a remote service. It is separate from the local LaMa.
- **Other computational tools:** AI lens blur / bokeh (`lens_blur.rs`, driven by the depth map), AI denoising (`denoising.rs`), HDR merge with deghosting (`hdr_deghosting.rs`), panorama (`panorama_stitching.rs`), focus stacking (`focus_stacking.rs`), negative conversion (`negative_conversion.rs`), culling (`culling.rs`), tethering (`camera_tethering.rs`, `tethering` feature).

## 6. Catalog / library model

- **No database.** The library is folder-based and sidecar-driven: multi-root folders, custom albums and per-folder icons (README 2026-05). Edits live in a JSON sidecar `<filename>.rrdata` next to the image, or `<filename>.<id>.rrdata` for virtual copies (`file_management.rs:387-416`, `parse_virtual_path`, with `?vc=<id>` for the copy).
- **Sidecar content:** `ImageMetadata { version, rating, adjustments, tags, exif }` (`image_processing.rs:~55-70`). Writes go through a debounced queue (`file_management.rs:135-200`).
- **Separate EXIF cache:** `.rrexif` sidecar (`exif_processing.rs`, `write_rrexif_sidecar`), plus folder-level EXIF caching.
- **XMP interop:** an optional `enable_xmp_sync` setting (`app_settings.rs:516`, default Some(true) at 642) reads **rating and colour label** (and tags) from `.xmp` (`sync_metadata_from_xmp`, `file_management.rs:~4150-4190`) and can write them back (`tagging.rs:441-594`). It does **not** read or write `crs:` develop settings, so develop settings do not round-trip with Lightroom.
- **Library features:** star ratings, colour tags, AI tagging via CLIP, filter queries, sorting, culling view. There are no collections beyond albums, no smart collections, no keyword hierarchy and no import workflow beyond ingest from tethering. There is no face or map module.
- **Consequence:** simple and portable, but without an index there is no fast cross-folder search at scale, and sidecars are RapidRAW-private.

## 7. Lens correction and camera profiles

- **Lensfun database, embedded** (`src-tauri/lensfun_db/*.xml`; loaded at `lens_correction.rs:553-627`, with an Android `include_dir!` path at line 11). It was updated 2026-08-07 (README:95). The database is parsed in Rust from XML. Distortion models and TCA/vignetting coefficients (`lens_dist_k1..k3`, `tca_vr/vb`, `vig_k1..k3`, `GeometryParams` in `image_processing.rs`) feed the geometry pass. Lensfun data is CC-BY-SA 3.0, which brings attribution and share-alike terms for the bundled XML (to be confirmed).
- Distortion has a manual `×2.5` scaling (`image_processing.rs:613,708,860,1092`). The lens vignette amount is multiplied by 0.8 (739).
- **Adobe lens profiles (`.lcp`):** not supported, so presets with `LensProfileEnable` do nothing special.
- **Camera profiles:** **no DCP, no ICC input profiles**, no "Adobe Standard/Color/Landscape/Portrait" look and no camera-matching profiles. Raw colour comes from rawler's matrices. Presets that set `CameraProfile` therefore cannot reproduce the intended base look.
- LUTs: `.cube`, `.3dl` and image LUTs supported (`lut_processing.rs`, 723 lines), tetrahedral interpolation (`shader.wgsl:1505`). This is the closest hook for emulating Adobe profile tables, but there is no converter for `RGBTable`/`Table_*`.

## 8. Assessment for our project

### (a) Strengths
- A working cross-platform stack (Tauri, Rust, wgpu) with a compute-shader pipeline and live preview, plus Android.
- Very active, with rapid feature velocity and a large contributor base, with a large amount of functionality: AI masks, retouch, HDR, panorama, culling, tethering, film LUTs.
- Rawler for raw decoding, an embedded Lensfun database, JSON sidecars, and a preset import UI and data model already in place.
- Clean AGPL licence with no CLA, and no vendor lock-in on models (downloaded on demand).
- The preset converter gives us a place to start; the `crs:` key list at lines 107-138 is a useful checklist.

### (b) Weaknesses and gaps against our goal
1. **Preset fidelity is not a design goal.** The shader maths is its own (sections 2 and 3), so faithful reproduction would mean rewriting the core shader chain (tone, HSL, clarity/texture/dehaze, curves, vignette, grain, white balance) to match Adobe PV2012+ numerically. That is most of the pipeline.
2. **Colour science.** No camera profiles (DCP/ICC), no Adobe baseline tone curve or profile tables, no wide-gamut working space or output ICC, WB is multiplier-based and not Kelvin or chromatic-adaptation, and the base tone mapper is a hand-tuned curve. Look carry-over (`Look`, `RGBTable`) is entirely missing.
3. **Converter limits.** Regex parsing, no real lrtemplate (Lua) parser, no mask or calibration or parametric curve or B&W handling, and key-scaling constants that appear tuned by eye. No tests and no reference dataset.
4. **Semantics.** Flat slider-sum mask model, `centré` and other custom controls, and ad-hoc `SCALES` constants spread between Rust and TS.
5. **DAM.** No catalog database, no import or collections or keyword hierarchy, no `crs:` XMP read/write, and limited interoperability with Lightroom catalogs or sidecars.
6. **Code health / provenance.** Solo-maintainer project with heavy churn (the shader's tonal functions were rewritten in the last 3 months), large files (`file_management.rs` is 4.3k lines, with `main()` in the shader being about 300 lines of inlined slider plumbing), partly LLM-written algorithms with unclear attribution, and a pinned fork of rawler.
7. **ML licence and runtime.** Several models have unverified licences, and ONNX runtime is dynamically loaded. The cloud generative path is a proprietary service dependency.

### (c) Options

| Option | Pros | Cons |
|---|---|---|
| **Contribute upstream** | Cheapest start, shared maintenance, immediate access to all UI and tools | Our headline feature (exact Adobe maths) conflicts with the project's tuned look and a moving shader. Upstream may not accept a parallel "Adobe-compatible" renderer. Rebasing a large fork on a solo maintainer's churn is costly. Control sits with one person. |
| **Fork** (AGPL) | Reuse everything, free to replace the pipeline and add a compat layer | Our code is AGPL either way, which is fine for an open-source product, and we inherit about 37k lines of Rust plus 55k of TS that we did not design. Divergence in the shader would make upstream merges impractical within months. |
| **Build new, learn from it** | Clean architecture around a documented, testable Adobe-compat colour model. Free licence choice (e.g. permissive core) if we write it ourselves. | Highest cost: we must build raw pipeline integration, UI and DAM from scratch. |

### Recommendation

**Build the colour/preset engine new, as a separate library, and do not base the product on a fork of RapidRAW's shader. Take RapidRAW opportunistically (AGPL-compatible parts) and track it as a UI/feature reference.**

Reasoning:
1. The differentiator lives in the part of RapidRAW that would be rewritten anyway (the processing chain, the converter, the working space, camera profiles). Forking buys us the shell, not the hard part.
2. A numerically specified, test-driven Adobe-compat engine (reference renders and dE metrics against Lightroom exports, a real XMP and lrtemplate parser, DCP and profile support, ProPhoto working space) can be a standalone crate with a clean licence. It could then be offered to RapidRAW as an optional renderer, or used in our own app.
3. If time-to-first-demo matters, a **fork is the pragmatic fallback**. In that case treat the shader and converter as replaceable modules from day one: keep upstream's UI, library, AI and export, and swap the compute chain behind the existing `adjustments` JSON by adding a "compat mode" and process-version field.
4. Before committing either way: (i) ask the maintainer whether a compat renderer would be welcome, (ii) run the converter on a dozen commercial presets and compare to Lightroom exports to quantify the gap (we have not done this), (iii) audit model and Lensfun licences.

### Caveats
- Not read in full: `shader.wgsl` beyond the functions cited, `image_processing.rs`, `gpu_processing.rs`, and the TS UI. Slider-range claims come from `SCALES` and `INITIAL_ADJUSTMENTS`, not from the slider components.
- Model licences are from general knowledge, not checked in the repository.
- Fidelity conclusions are from code reading only, with no rendered comparison.
