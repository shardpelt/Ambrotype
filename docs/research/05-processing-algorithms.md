# 05 - Processing algorithms: how ACR/Lightroom works and what open algorithms approximate it

Status: research notes, 2026-10-03. Confidence tags: **[SDK]** = verified by reading Adobe DNG SDK source; **[PUB]** = in a published paper/Adobe doc; **[INF]** = community inference, unverified; **[LOC]** = verified present in local clone of darktable/RawTherapee (grep only).
Difficulty: easy / medium / hard = effort to get within ~ΔE2000 < 2 of Lightroom on typical images.

**Core caveat.** Adobe's ACR/Lightroom pipeline is closed source. Only the DNG SDK (a *baseline* reference renderer) and a handful of papers/talks are public. Everything past `DNG baseline` (PV2012 tone controls, Clarity, Texture, Dehaze, Color Grading, denoise, Upright, masking) must be treated as black-box behaviour to be fitted empirically (see Fidelity strategy). Items marked [INF] are hypotheses to test, not facts.

---

## 0. Public sources worth knowing

| Source | URL | What it gives |
|---|---|---|
| Adobe DNG SDK download page | https://www.adobe.com/support/downloads/dng/dng_sdk.html | Full C++ source (dng_render.cpp, dng_temperature.cpp, dng_spline.cpp, dng_hue_sat_map.cpp, dng_camera_profile.cpp, dng_color_spec.cpp ...) |
| DNG SDK EULA | https://www.adobe.com/support/downloads/dng/dng_sdk_eula_win.html | Per a search summary, the licence is non-exclusive, worldwide and royalty-free, and covers using, copying, making derivative works from, distributing and sublicensing the SDK for any purpose (read the actual text + patent clause with counsel before copying code; ScanCode tracks it as `adobe-dng-sdk`: https://scancode-licensedb.aboutcode.org/adobe-dng-sdk.html). File headers say the file may be used, modified and distributed only under the terms of the Adobe licence agreement that accompanies it. Not GPL-compatible by default -> treat as a separate-license reference; safest is **re-implement from the DNG spec + SDK behaviour** rather than paste. |
| DNG SDK mirrors on GitHub (convenience) | https://github.com/hfiguiere/dng_sdk , https://github.com/aizvorski/dng_sdk | Mirror source (local read: raw.githubusercontent.com/hfiguiere/dng_sdk/master/dng_sdk/source/*.cpp). Same license applies. |
| DNG specification 1.6/1.7 | https://helpx.adobe.com/camera-raw/digital-negative.html | Tag definitions: ColorMatrix, CameraCalibration, ForwardMatrix, ProfileHueSatMap, ProfileLookTable, ProfileToneCurve, BaselineExposure, etc. Public, usable for clean-room implementation. |
| Paris, Hasinoff, Kautz - Local Laplacian Filters: Edge-aware Image Processing with a Laplacian Pyramid (paper), SIGGRAPH 2011 / CACM 2015 | https://people.csail.mit.edu/sparis/publi/2011/siggraph/ | Algorithm widely cited as the basis of PV2012 Shadows/Highlights and likely Clarity. Adobe-affiliated author (Hasinoff was at Adobe/MIT; Kautz UCL). **[PUB: paper; PV2012 link is INF-from-Adobe-hints]** |
| He, Sun, Tang - Single Image Haze Removal Using Dark Channel Prior, CVPR 2009 / PAMI 2011 | https://kaiminghe.github.io/publications/pami10dehaze.pdf | Dehaze candidate. (Microsoft patent US 8,340,461 reported lapsed per search result - verify before relying.) |
| He, Sun, Tang - Guided Image Filtering | https://kaiminghe.github.io/publications/pami12guidedfilter.pdf | Edge-preserving base/detail for Texture/Clarity/NR. |
| Chen, Paris, Durand - Real-time edge-aware image processing with the bilateral grid, SIGGRAPH 2007 | https://people.csail.mit.edu/sparis/publi/2007/siggraph/ | Fast local tone/contrast. |
| Luminous Landscape, article Tonal Adjustments in the Age of Lightroom 4 | https://luminous-landscape.com/tonal-adjustments-in-the-age-of-lightroom-4/ | Eric Chan commentary: PV2012 originated from trying edge-aware algorithms that were too slow. |
| DPReview article Extreme contrast edits in Lightroom 4 and ACR 7 | https://www.dpreview.com/articles/1205103502/extreme-contrast-edits-in-lightroom-4-and-acr-7 | Behaviour description. |
| Adobe: Dehaze/Vignette/Grain | https://helpx.adobe.com/camera-raw/desktop/edit-and-enhance-images/tone-and-color/vignette-grain-effects-camera-raw.html | Official control descriptions. (helpx basic-adjustments pages returned HTTP 403 to WebFetch from this environment; fetch manually.) |
| Adobe: Lightroom Classic Develop options | https://helpx.adobe.com/lightroom-classic/help/develop-module-options.html | Official control descriptions, PV2012 notes. |
| DPReview: ACR 9.2 local dehaze | https://www.dpreview.com/articles/2826215761/adobe-camera-raw-9-2-adds-local-dehaze | Dehaze history (global in 9.1, local in 9.2). |
| Adobe Lens Profile Creator guide | https://www.adobe.com/special/photoshop/camera_raw/lensprofile_creator/lensprofile_creator_userguide.pdf | Profile model overview. |
| Lensfun LCP converter | https://github.com/lensfun/lensfun/blob/master/docs/man/lensfun-convert-lcp.1.rst and https://raw.githubusercontent.com/lensfun/lensfun/master/apps/lensfun-convert-lcp | Existing open LCP parser (notes 3 schema variants). |

Patents: I did not find (in this pass) a reliable, specific Adobe patent number for PV2012 Highlights/Shadows or Clarity. A search summary stated Adobe has patents on learned-model/per-pixel-bound dehaze variants - **unverified; do a Google Patents assignee search ("Adobe Inc." + "dehaze", "tone mapping", "Laplacian pyramid", "lens profile") before finalizing** and have counsel do FTO review. A patent search is also relevant to *our* risk: published algorithms (local Laplacian, guided filter) were academically published, but Adobe patents may cover specific implementations.

---

## 1. Raw pipeline basics

### 1.1 Stages (ordering as in the DNG spec processing model)
1. Decode, linearization table (LinearizationTable tag), black level (BlackLevel, per-CFA-channel + optional per-row/col repeat patterns), white level (WhiteLevel), ActiveArea/crop. DNG semantic: `lin = (raw - black) / (white - black)`; clamp *after* WB so highlights can be reconstructed.
2. Opcode lists (GainMap/lens shading, WarpRectilinear, FixVignetteRadial, bad-pixel fix) - OpcodeList1/2/3. Darktable has parser: `src/common/dng_opcode.c` [LOC].
3. White balance multipliers (AsShotNeutral or from user Temp/Tint).
4. Highlight reconstruction/clipping handling, demosaic (Adobe's own, undisclosed; see below).
5. Camera RGB -> XYZ(D50) via ColorMatrix1/2 (interpolated on CCT) or ForwardMatrix1/2; -> linear ProPhoto (ROMM) working space.
6. DCP HueSatMap (HSV, in ProPhoto linear, with encode/decode curve), exposure/black ramp, LookTable, tone curve, to output space. See 1.3.

Adobe's internal working space for ACR/LR is a ProPhoto-primaries space (D50 white); the DNG SDK renders into `dng_space_ProPhoto` (linear) as intermediate **[SDK: dng_render.cpp ~L916-960, `fCameraToRGB = dng_space_ProPhoto::Get().MatrixFromPCS() * ...`]**. LR's Develop histogram and the on-screen preview use sRGB-primaries with the **ProPhoto/"Melissa RGB"** (ProPhoto primaries + sRGB TRC) - that is the *display/histogram* space in Lightroom, not necessarily the processing space **[PUB: Adobe forum statements; INF as to exact internal use]**. Implication: all sliders' *numerical* behaviour (e.g. Saturation, Vibrance, HSL) is defined in a wide gamut, so gamut clipping rarely occurs in the pipeline; a clone should also work in ProPhoto/linear-wide-gamut internally, and only clip at output.

### 1.2 Demosaic
- Adobe algorithm unpublished. Known behaviour: adaptive, relatively soft at pixel level vs. RCD/AMaZE; Enhance Details (2019) is a CNN demosaic (closed).
- Candidates: RCD (ratio corrected demosaic; fast, good, GPL darktable `src/iop/demosaicing/rcd.c` [LOC]; RawTherapee `rtengine/rcd_demosaic.cc` [LOC]), AMaZE (`amaze.cc` [LOC], GPL, high quality), LMMSE (`lmmse.c` [LOC], noise-robust at high ISO), PPG/VNG, Markesteijn for X-Trans (darktable `markesteijn.c` in demosaicing dir), dual (`dual_demosaic_RT.cc` [LOC]).
- Difficulty: **easy-medium** for plausible output; matching Adobe pixel-level texture is **hard** and low priority (ΔE impact negligible; differences are in micro-contrast).
- Recommendation: RCD default (fast), AMaZE option; LMMSE for high ISO. Pre-demosaic CA correction option: `cacorrect.c` [LOC]/`CA_correct_RT.cc` [LOC].

### 1.3 Highlight recovery / reconstruction
- Adobe: raw WB multiplies channels; where one channel clips, ACR reconstructs from unclipped channels (the Highlights/Whites sliders with negative values pull back, and "Exposure" negative reveals data above 1.0 pre-WB headroom - ACR keeps >1.0 float headroom). Details closed.
- Candidates: darktable `highlights.c` [LOC] (modes: clip, LCh, inpaint opposed, segmentation-based, guided laplacians); RawTherapee `hilite_recon.cc` [LOC] (Color Propagation, Blend, Luminance recovery). "Inpaint opposed" (Hanno Schwalm) is a good modern choice; segments-based also.
- Difficulty: **medium**. Fidelity metric: hue of near-clipped areas (sky, skin highlights). Adobe tends toward neutral/desaturated clipped highlights with slight magenta/yellow shift depending on camera; fit by sweep on overexposed charts.
- Key design point for fidelity: keep **per-channel unclipped linear data above 1.0** until the tone curve; DNG SDK does this via "supportOverrange" with linear slope extension above 1.0 (`fToneCurveSlopeExtension`) **[SDK dng_render.cpp L1066-1077]**.

### 1.4 Camera matrix -> PCS; DCP processing order [SDK dng_render.cpp, verified]
From reading `dng_render.cpp` (`dng_render_task::ProcessArea`, lines ~1810-2030):
1. Camera RGB (WB-applied) -> linear ProPhoto via `fCameraToRGB`.
2. **HueSatMap** (ProfileHueSatMapData, `fHueSatMap`, chosen for the white point via `profile.HueSatMapForWhite(xy)` which interpolates between illuminant-1/2 tables by CCT-inverse) applied in HSV with encode/decode 1D tables (`fHueSatMapEncode/Decode`, used when ProfileHueSatMapEncoding = sRGB gamma).
3. **Exposure + shadows ramp** (`dng_function_exposure_ramp`, see Exposure).
4. **LookTable** (ProfileLookTable, `fLookTable`) - same HSV-map structure, applied *after* exposure.
5. **Tone curve**: profile ProfileToneCurve if present (else default `dng_tone_curve_acr3_default`, a 1024-ish point table baked in the source at L200-470 - the classic ACR 3 default curve), applied **per RGB channel** (`fToneCurve` 1D table), with linear slope extension for >1.0. Note `fRGBTables` (ProfileGainTableMap / RGBTables in DNG 1.6) are also applied after.
6. Linear ProPhoto -> output space matrix, then output gamma (sRGB TRC function `dng_function_GammaEncode_sRGB`) -> 8/16-bit.
- Order summary: `WB -> camera->ProPhoto -> HueSatMap -> [exposure/black ramp] -> LookTable -> tone curve (per-channel) -> output`.
- HSV map semantics (DNG spec ch. 6): table indexed by hue (0-360) x sat x (optional) val; each entry = (hue shift degrees, sat scale, val scale); trilinear interpolation with hue wrap; applied in HSV computed from linear (or encoded) RGB. RawTherapee ports this: `rtengine/dcp.cc` `DCPProfile::makeHueSatMap` L1943 and the HueSatMap application routine at L2016, whose comment says it was ported from Adobe's reference implementation [LOC]. Reuse its structure.
- Purchased presets (XMP) don't contain DCPs, but **they reference profiles** (`crs:CameraProfile` e.g. `Adobe Color`, `Adobe Standard`, `Camera Vivid`, or embedded creative LUT `crs:Look` / `.cube`-style `Look` tables in XMP). Adobe Color/Standard DCPs ship with ACR/DNG Converter; you can't redistribute them, but a user can supply them from their own install (they are DCP files, an open-spec format). Newer `Adobe Color/Monochrome` + `Creative profiles` use ProfileLookTable / RGBTables (1.6) - supportable from the spec.
- Difficulty: **medium** (spec is public; the SDK is the oracle). Highest-value item for fidelity because *every* preset renders through the profile.

---

## 2. White balance: Temp / Tint

Official: Temperature in Kelvin for raw files (scale -100..+100 relative for JPEG/TIFF); Tint -150..+150 green-magenta.

**Exact implementation [SDK dng_temperature.cpp]** (Robertson's method, 1968, Wyszecki & Stiles table p.228, 31 isotherms in mired):
- `kTintScale = -3000.0`. Tint is the signed distance in CIE 1960 (u,v) from the Planckian locus along the isotherm normal, times -3000 (so Tint +10 = 0.0033 uv-units toward... sign per code `offset = tint / kTintScale`).
- Table `kTempTable[31]` of {r (mired = 1e6/T), u, v, t (isotherm slope)}: r = 0,10,...,100,125,...,600; e.g. {0,0.18006,0.26352,-0.24341}, {100,0.19032,0.29326,-0.47888}, {200,0.21142?...} -> copy the full table from the SDK (values for r=0..600 in lines 38-70 of dng_temperature.cpp; they are factual data from a published textbook, not copyrightable creative expression, but verify).
- **Forward (T,tint -> xy)**, `dng_temperature` -> `LegacyGetXY`: `r = 1e6/T; offset = tint/kTintScale;` find index with `r < table[i+1].r` (clamp i<=29); `f = (r1 - r)/(r1 - r0)`; `u = u0*f + u1*(1-f)`, `v` likewise; then `uu1 = 1; vv1 = t0; len = hypot...; uu2 = 1; vv2 = t1`; interpolate the two unit isotherm directions with the same `f`, renormalize to (uu3, vv3), then `u += uu3*offset; v += vv3*offset` (isotherm vectors are (1, t) normalised, rotated per Robertson; follow the SDK exactly, including sign convention) and convert (u,v)->xy: `x = 1.5u/(u - 4v + 2)`, `y = v/(u - 4v + 2)`... (xy = (3u/(2u-8v+4), 2v/(2u-8v+4))).
- **Inverse (xy -> T, tint)**: loop over isotherms computing signed distance `dm = (v - v_i) - t_i (u - u_i)` / sqrt(1+t_i^2); find sign change between i-1 and i; interpolate to get `r`, `T = 1e6/r`; tint = -dist * kTintScale (distance to locus at interpolated point). Same code in RawTherapee `colortemp.cc` L4210 ("implementation of Robertson's method") [LOC] - GPL port to compare.
- **New in SDK 1.7.x (observed in the mirrored file)**: extended range with `kExtendedMinTemperature=1500`, blend 1667..2000 K using SmoothStep, max 50000 K, tint -150..150, LUT 101x61 at 5-step, built from a 5-nm CIE 1931 2-deg table and (presumably) spectral Planckian integration. Old table-based method (`LegacyGetXY`) is the classic behavior ACR users have. For fidelity against current Lightroom, use extended path below 2000 K only if sweeps show a difference; otherwise Robertson table.
- **From (xy) to camera multipliers [SDK dng_color_spec.cpp, DNG spec ch. 6]**: for a given white xy -> if profile has ForwardMatrix: `CameraToPCS = FM * D^-1 * (AB*CC)^-1`-style with `D` solved so camera neutral maps to white xy (iterative: guess neutral -> xy via interpolated ColorMatrix (interpolation weight by 1/T between the two calibration illuminants) -> new neutral, repeat until convergence). This is the crux: **Kelvin in LR is "camera-calibrated"** - the same Kelvin number gives different multipliers per camera profile. You need the DCP/DNG ColorMatrix1/2 + CalibrationIlluminant1/2 for each camera (available in DNG files; for raw files you need a camera database - Adobe's `dcraw`/`libraw` or RawTherapee's `camconst`/dcp). Without ColorMatrix you cannot reproduce Adobe's Kelvin->multiplier mapping.
- For non-raw: LR uses a relative -100..+100 style slider (Temp) that applies a fixed WB-like adaptation in a ProPhoto-like space (approximately a chromatic adaptation toward the Planckian locus point mapped from Kelvin 2000..50000 K relative scale) **[INF]**; calibrate empirically.
- Difficulty: **easy** (raw, given matrices) / medium (JPEG/TIFF).
- Presets contain `crs:Temperature`/`crs:Tint` often **as relative offsets baked from the author's camera**; presets usually leave WB alone (users complain when they change it), so low priority except for "As Shot".

---

## 3. Exposure, Contrast

- Official: Exposure -5..+5 EV (PV2012: affects midtones to highlights mostly; Whites/Blacks set endpoints). Contrast: raises or lowers overall image contrast, with the main effect on midtones.
- Known/DNG baseline [SDK]: exposure is applied in **scene-linear** as `2^EV` gain with BaselineExposure (+ profile BaselineExposureOffset) added; for *negative* exposure beyond baseline the SDK `fakes it by darkening the tone curve` (`dng_function_exposure_tone`: for x<=0.25 `x*fSlope`, else quadratic `(a*x+b)*x+c` - a soft-knee shoulder so gain-down doesn't clip) - i.e. `white = 1/2^max(0,exposure)`; ramp from `black` to `white`. `black = Shadows(5.0 default) * ShadowScale * Stage3Gain * 0.001`; `dng_function_exposure_ramp` is a linear ramp `(x-black)*slope` with quadratic rounding of radius `min(0.5*minBlack, (1/16)/slope)` at the toe **[SDK L68-140, L1022-1045]**. This is the *DNG default `Shadows=5`* and exposure model; ACR's PV2012 is more complex.
- ACR PV2012 Exposure: a gain in linear with a soft highlight roll-off (positive exposure compresses highlights more gently than a hard clip; negative exposure reveals unclipped headroom) **[INF]**. Contrast is believed to be an S-curve (pivot near mid-gray, parametric in some log/perceptual domain) applied before/with the tone curve; amount-dependent shape. In PV2012, Contrast pivot ~ midtones of the **preview-gamma** domain **[INF]**.
- Candidates: darktable `exposure.c` [LOC] (linear gain + black level correction), filmic `filmicrgb.c`/`sigmoid.c`/`agx.c` [LOC] for shoulder shape, RT `ipcontrast`... Fit: `out = shoulder(2^EV * x)` then contrast as sigmoid/power around pivot in log2 domain.
- Difficulty: Exposure **easy** (gain is exact; calibrate the shoulder); Contrast **medium**.

---

## 4. Highlights / Shadows / Whites / Blacks (PV2012)

- Official: Highlights - adjusts bright areas (drag left to darken them and recover highlight detail); Shadows - brightens dark areas and recovers detail; Whites - sets white clipping point; Blacks - sets black clipping point.
- Known [PUB/INF]: PV2012 tone controls are **locally adaptive (edge-aware)** in at least Highlights/Shadows; Adobe says Eric Chan et al. tried slow edge-aware algorithms and developed a fast version (the Local Laplacian Filters paper is said to describe part of the technology behind Shadows/Highlights - per the Luminous Landscape piece above and community consensus). Behaviour: Shadows +100 lifts dark local regions without flattening highlights, with halo-free behaviour -> consistent with local Laplacian/"detail & base remap" in log-luminance. Whites/Blacks behave as global levels (end-point shifts of the global curve) with some adaptive component **[INF]**.
- Candidate algorithms:
  1. **Local Laplacian filter** (Paris 2011) with remapping function r(i)= g + sign(i-g)*sigma*((|i-g|/sigma)^alpha), tone-mapping/detail-boost variant, on log-luminance; fast version in Aubry et al. 2014 `Fast Local Laplacian Filters` (https://people.csail.mit.edu/sparis/publi/2011/siggraph/ , https://hal.science/hal-00958092). Darktable implementation: `src/common/locallaplacian.c` (+ OpenCL `locallaplaciancl.c`) [LOC], used by `src/iop/bilat.c` (`local contrast`) [LOC] and `shadhi`? (shadhi uses gaussian/bilateral).
  2. Base/detail with guided filter/bilateral: `src/common/guided_filter.c`, `bilateral.c` (bilateral grid) [LOC]. RT: `ipshadowshighlights.cc` [LOC] (Lab, gaussian-based), `tmo_fattal02.cc` [LOC], `dirpyr_equalizer.cc` [LOC].
  3. Darktable `toneequal.c` [LOC] (guided-filter-based luminance masks + per-EV-band exposure) - closest in spirit to "Shadows/Highlights as masked exposure gain in linear", and it's easier to tune than LLF.
- Fit approach: sweep each slider on a synthetic ramp-with-gradient target (see harness) and fit remap parameters (sigma, alpha, pivot) for LLF in log domain. Because Adobe likely mixes adaptive and global terms, fit *amount-dependent* parameters per slider value (LUT of fitted params at -100,-50,0,+50,+100, interpolate).
- Difficulty: **hard**. This is the single biggest fidelity risk for presets (nearly all presets set Highlights/Shadows/Whites/Blacks).

---

## 5. Texture, Clarity

- Official: Clarity raises local contrast by boosting contrast at the edges of midtones (mid-frequency; large radius). Texture enhances or smooths medium-sized details such as skin pores or bark; it affects colour and tone differently from Clarity, targets fairly fine detail without touching small-scale noise, and positive values sharpen details while negative values smooth them yet keep large edges (Adobe 2019 launch blog: https://blog.adobe.com/en/publish/2019/05/14/lightroom-classic-2019-texture ; verify exact wording).
- Known/inferred: Clarity PV2012 = local-contrast enhancement through the LLF-style edge-aware pyramid (strong halos suppressed), midtone-weighted; PV2010 Clarity was unsharp-mask-like with large radius (about radius 100px-ish, midtone weight) **[INF]**. Texture = band-pass edge-aware detail scaling at scale between Sharpening and Clarity; negative values do a protected blur (edge-aware smoothing). Possibly same LLF with different remap parameters or a 2-3 band wavelet/guided-filter decomposition **[INF]**.
- Candidates: darktable `bilat.c` (local contrast via bilateral/LLF) [LOC]; `diffuse.c` (diffuse or sharpen, wavelet-based `local contrast` presets) [LOC]; RT `ipwavelet.cc` (contrast by level, `Local contrast`), `ipsharpenedges.cc` [LOC]; guided filter detail enhancement (He et al.). Texture: guided filter base (r~4-8 px at 24MP scale-normalized, eps small) + detail*gain, optionally masked by edge strength to avoid noise amplification.
- Resolution-dependence: ACR scales radii by image size (a percentage of long side) - preview vs. export consistency. Must implement **scale-relative radii**; calibrate with different-resolution renders of the same chart.
- Difficulty: Clarity **medium-hard**; Texture **hard** (small perceptual deltas but must be reasonable).

---

## 6. Dehaze

- Official: Dehaze decreases or increases haze; positive removes haze, negative adds haze (like fog). Global in ACR 9.1; local in 9.2 (DPReview link above).
- Known/inferred: dark-channel-prior style: estimate atmospheric light A and transmission t(x) = 1 - w*min_c(min_Ω(I_c/A_c)), refine (guided filter/soft matting), recover J=(I-A)/max(t,t0)+A; negative = blend toward A. Adobe adds contrast/saturation/tone shaping in addition (observed: dehaze darkens and increases saturation, contrast in shadows) **[INF]**. Search summary noted Adobe patents on learned/bounded variants - unverified.
- Open: darktable `hazeremoval.c` [LOC] (explicitly He et al. + guided filter: `common/guided_filter.h`), RT `ipdehaze.cc` (`ImProcFunctions::dehaze` L308 and `dehazeloc` L457) [LOC] - also dark channel + guided filter with "strength" and "depth" outputs.
- Fit: Dehaze behaviour in LR strongly depends on image content (A estimate); calibrate the extra tone/saturation coupling by sweeping Dehaze on a haze-free chart (shows pure side-effect curve) and on foggy images.
- Difficulty: **medium** for a plausible result, **hard** for fidelity.

---

## 7. Vibrance vs Saturation

- Official: Vibrance boosts low-saturation colours more than high-saturation ones, limits clipping of strongly saturated colours and protects skin tones; Saturation uniform for all colors.
- Known/inferred [INF]: both operate in a ProPhoto-based chroma domain (HSL-ish/ Lab-like) - Saturation approx linear scaling of chroma (`C' = C*(1+s)`, s in -1..1 where -100 = fully desaturated to luminance-equal gray); Vibrance = chroma gain weighted by `(1 - C/Cmax)` with skin-hue protection (hue window ~ 20-60 deg orange/red in Lab/HSV), with saturating compression at high end.
- Open candidates: darktable `vibrance.c` (legacy) / `velvia.c` [LOC], `colorbalancergb.c` [LOC] (has vibrance and saturation with chroma-dependent gains in JzAzBz/Ych - modern, hue-preserving), RT `ipvibrance.cc` [LOC] (Lab-based, skin-protect, "pastel/saturated" thresholds).
- Fit: sweep on a saturation gradient chart (patches of constant hue with chroma 0..max): measure ΔC vs C for each slider value to recover the weighting function; fit.
- Difficulty: **easy-medium** (global per-pixel; well-constrained by chart sweeps).

---

## 8. Tone curves (parametric + point)

- Official: Point curve (RGB + R,G,B channels); Parametric (Highlights, Lights, Darks, Shadows + 3 region split sliders); PV2012 default point-curve domain is **after** the PV2012 tone controls, in gamma-encoded space.
- Known [INF/SDK]:
  - Point curve: Adobe curves are monotone *cubic spline* through control points; DNG SDK `dng_spline_solver` computes the one curve that is continuous in value, first and second derivative and has zero second derivative at the end points - i.e. a **natural cubic spline** (strictly increasing X required) **[SDK dng_spline.cpp, verified]**; evaluate piecewise cubic, clamp output to [0,1]. ACR's point curve in LR UI is believed to be the same family (tiny overshoot sometimes visible between very close points -> consistent with a natural spline w/o monotonicity constraint) **[INF]**.
  - Domain: **RGB curve is applied per-channel on gamma-encoded values** (so it shifts saturation: S-curve increases saturation) - in LR the `RGB` curve applies to each channel with sRGB-like gamma (`ProPhoto gamma 1.8 vs sRGB` uncertain). DNG baseline applies tone curve per channel too (ACR3 default curve) [SDK]. LR UI curve x-axis is the *encoded* value of the preview/Melissa space (sRGB TRC) **[PUB: Adobe forum answers; INF in detail]**. Calibrate the TRC with a gray ramp + a 3-point curve.
  - Parametric curve: four sliders add/subtract at region centres, region boundaries (default 25/50/75 %) set the crossing points; implemented as a smooth bump basis summed over the identity - formula undocumented. **[INF]** Fit with measured ramp responses (sweep each slider -100/+100, 11-point ramp, read output curve; fit Hermite bumps whose supports follow the split values).
- Open: darktable `rgbcurve.c`, `tonecurve.c`, `common/curve_tools.c` (cubic/monotone/catmull-rom, `splines.cpp`) [LOC]; RT `curves.cc`/`flatcurves.cc` [LOC] (cubic spline, "ControlPoint" curves); `dng_spline.cpp` as reference for natural spline equations.
- Difficulty: point curve **easy** (once domain/TRC calibrated); parametric **medium**.

---

## 9. HSL (Hue / Saturation / Luminance, 8 bands) and Color Mixer

- Official: 8 bands (red, orange, yellow, green, aqua, blue, purple, magenta), H/S/L per band.
- Known/inferred [INF]: weights are smooth overlapping bumps in **hue angle** of an HSV/HSL-like hue measured from the (ProPhoto-linear -> sRGB-gamma?) RGB; Adobe's hue-band centres are ~ red 0, orange 30, yellow 60 (some say ~ 52), green 120, aqua 180, blue 240, purple 270, magenta 300 (some sources) with overlapping triangular or cosine-squared window; Hue shift is a rotation by up to ±~30° (Hue slider ±100 -> ±~ (band spacing)); Saturation per band ±100 scaling of chroma; Luminance per band scales luminance weighted by saturation (lower impact in gray areas). **Do not hardcode; measure.**
- Measure: render a hue wheel chart (e.g. HSV hue sweep at constant S,V in ProPhoto) with each band slider at ±100, extract weight vs hue from the response (Δhue, ΔC/C, ΔL).
- Candidates: darktable `colorzones.c` (selectable by L, C, h with spline curves) and `colorequal.c` (new, color equalizer in Ych) [LOC]; RT `improcfun.cc` `ImProcFunctions::chromiLuminanceCurve/HSVEqualizer` (HSV equalizer) [LOC].
- Difficulty: **medium** (fits well to a parametric bump model; also needs correct working space, since Adobe's hue is in a wide-gamut space).

---

## 10. Color Grading (3-way: Shadows/Midtones/Highlights + Global; Blending, Balance)

- Official (LR 10 / ACR 13, 2020): replaces Split Toning; three wheels each with Hue, Saturation, Luminance; Blending 0-100 (overlap of regions), Balance -100..+100 (shifts boundaries), plus Global. (Adobe: https://helpx.adobe.com/lightroom-classic/help/color-grading.html - fetch to confirm).
- Known/inferred [INF]: luminance-masked tinting; each wheel adds a tint (hue,sat) and luminance offset weighted by smooth masks of luminance (shadows weight ~ 1-smoothstep, highlights ~ smoothstep, midtones ~ bump), with Blending controlling mask overlap and Balance moving the crossover luminance. Tint applied in a chroma plane with sat as strength (not a pure RGB multiply). Older Split Toning: highlights hue/sat and shadows hue/sat with balance - simple overlay tint at luma threshold (RT `splittoning.c`-like: darktable `splittoning.c` [LOC]).
- Candidates: darktable `colorbalancergb.c` (4-way: shadows/mids/highlights/global in Ych/DT UCS, masks with fulcrum/contrast) [LOC] and `colorcorrection.c`; `colorbalance.c`; RT `ipsoftlight.cc` no; RT "Color Toning" in `improcfun.cc` (`ImProcFunctions::colorToningLabGrid`...) [LOC].
- Difficulty: **medium** (parameter-space small; Adobe's mask shape & blend curve fit via sweeps of each wheel on a gray ramp + color patches).

---

## 11. Calibration panel (Camera Calibration)

- Official: Shadows Tint; Red/Green/Blue Primary Hue and Saturation (PV2012+).
- Known [INF]: Primary adjustments modify the camera->working space matrix columns: hue rotates the primary's chromaticity around white in a (ProPhoto-ish) xy/uv space; saturation scales distance from white. Shadows tint adds a green-magenta shift weighted to shadows. Equivalent to editing `ForwardMatrix` before the HueSatMap **[INF]**; test: with a neutral gray ramp and the pure R primary slider, neutrals stay neutral (matrix rows normalized) - verify.
- Implementation: 3x3 matrix modification post-WB in camera RGB; calibrate each of 6 sliders by measuring change in patch colors for a ColorChecker and solving for per-slider matrix derivative.
- Difficulty: **medium**. Presets use it occasionally (teal & orange looks); fit with a 24-patch chart sweep.

---

## 12. Sharpening (Detail panel)

- Official: Amount, Radius (0.5-3), Detail (0-100), Masking (0-100); PV2012; luminance-only (applied in Lab-like L, Lightroom treats raw sharpening via capture-sharpening); per Adobe/Chan, lower Detail means halo suppression (USM-like) and higher Detail means deconvolution-like behaviour (Detail > 25 adds an iterative deconvolution-like term) - Eric Chan's Camera Raw sharpening talk/notes; LR: Alt-drag shows masks.
- Known/inferred: radius -> Gaussian sigma at 1:1; Amount scales detail added; Masking: edge mask = thresholded gradient magnitude (Gaussian-blurred), smooth step applied so flat regions aren't sharpened; Detail: mixes USM (halo-suppressed) with a Richardson-Lucy-like deconvolution **[INF, partly PUB via Adobe forum posts by Eric Chan: https://www.adobe.com/products/photoshop/camera-raw/ no stable URL; search "Eric Chan Detail slider deconvolution"]**.
- Open: darktable `sharpen.c` (USM, Lab L), `diffuse.c` ("sharpen demosaicing"), `lowpass`; RT `ipsharpen.cc` (USM + RL deconvolution - `deconvsharpening`) and `ipsharpenedges.cc` [LOC]. RT's `ipsharpen.cc` already has both USM and RL; good starting point.
- Difficulty: **medium** (Amount/Radius easy; Detail & Masking curves hard but low ΔE impact). Presets rarely rely on it (but they do set Sharpening for look).

---

## 13. Noise reduction

- Official: Luminance (Amount, Detail, Contrast), Color (Amount, Detail, Smoothness); Denoise AI (2023): raw-based neural denoise, produces new DNG (closed).
- Known: PV2012 NR is a multiscale (wavelet?) edge-preserving NR in YCbCr-like opponent space; details undisclosed **[INF]**.
- Open: darktable `denoiseprofile.c` (Poisson-Gaussian variance stabilising + NL-means or wavelets/ "nlmeans_core.c") [LOC], `nlmeans.c` [LOC], `cfa_linedn`; RT `ipwavelet.cc` denoise [LOC], `impulse_denoise.cc`, `cplx_wavelet_*` [LOC]. NL-means (Buades 2005) and BM3D are literature standards.
- Fidelity relevance: presets set NR amounts (e.g. luminance 20-40). Matching exactly is **hard** and low perceptual ΔE; plan "good enough, no obvious differences".
- Denoise AI: no open equivalent matching. Candidates (separate licensing): NAFNet/Restormer-style raw denoisers, or "RawDenoise" nets; plan as optional ML module later. Difficulty: **hard** (ML data + licensing).

---

## 14. Lens corrections (profile, CA, defringe)

- Official: Profile corrections (Enable, Remove CA, Lens Profile + distortion/vignetting amounts), Manual (distortion, defringe, vignette).
- **LCP format**: XML (RDF/XMP-style) files with camera/lens identification (`stCamera:Make/Model/Lens...`), `stCamera:PerspectiveModel` with `RectilinearWarpModel`: `FocalLengthX/Y`, `ImageXCenter/YCenter`, `RadialDistortParam1..3` (r' = r(1 + k1 r^2 + k2 r^4 + k3 r^6) normalized by focal length), `TangentialDistortParam1/2`, `ResidualMeanError`; vignetting model `VignetteModelParam1..3` (`gain = 1 + k1 r^2 + k2 r^4 + k3 r^6` with r normalized), per-focal-length/aperture/distance interpolation, chromatic aberration via separate R/B `RadialDistortParam` sets (`ChromaticRedGreenModel`, `ChromaticBlueGreenModel`). Documented through the Lens Profile Creator guide (https://www.adobe.com/special/photoshop/camera_raw/lensprofile_creator/lensprofile_creator_userguide.pdf) and Adobe's LCP spec in the Lens Profile Creator download; third-party parsers exist (lensfun `lensfun-convert-lcp`, https://raw.githubusercontent.com/lensfun/lensfun/master/apps/lensfun-convert-lcp ; RT `rtengine/lcp.cc` [LOC]; darktable via lensfun, `lens.cc` [LOC]). Lensfun notes Adobe changed the schema ~twice (three variants).
- Licensing: LCPs bundled in Adobe products are Adobe content - **do not redistribute**. Users can point the app at their own Adobe lens-profile folder (as RawTherapee does). LCPs the user makes themselves or community-contributed (Adobe's Lens Profile Downloader contributions are licensed to Adobe nonexclusively) have unclear redistribution; Lensfun database (CC BY-SA 3.0) is the redistributable alternative.
- Open algorithms: RT `lcp.cc` (LCP parser + apply: distortion, CA, vignette) [LOC]; darktable `lens.cc` (lensfun, modifier chain) [LOC]; Hugin/panotools models. CA: `cacorrect.c` (raw auto CA, Rotation & shift) [LOC], `cacorrectrgb.c` [LOC] (post-demosaic), RT `CA_correct_RT.cc` [LOC]. Defringe: Adobe defringe (Purple/Green amount + hue range via color-edge detection, desaturating fringe pixels near high-contrast edges) **[INF]**; open: darktable defringe (removed; `defringe` in older versions; `retouch`?) ; RT `defringe` in `improcfun.cc`/`ipwavelet` [LOC] ("Defringe" module based on chromatic edge mask).
- Difficulty: profile corrections **medium** (parse, apply; fidelity good if the profile is used); defringe **medium-hard**; auto-profile matching by EXIF - easy but the DB is the problem.

---

## 15. Upright / Geometry

- Official: Upright modes Auto, Level, Vertical, Full, Guided; Transform (Vertical, Horizontal, Rotate, Aspect, Scale, X/Y offset). Upright uses automatic detection of lines to fit a homography.
- Known [PUB/INF]: automatic detection of dominant near-vertical/near-horizontal line segments (edge/gradient based, LSD-like), optimises distortion-free homography so that lines become vertical/horizontal; "Full" adds both; result is a perspective transform composed with lens correction (lens profile needed for best result; Guided uses lines drawn by the user). Adobe's exact optimization unpublished **[INF]**; Adobe's stated support at https://helpx.adobe.com/lightroom-classic/help/guided-upright-perspective-correction.html (verify).
- Open: darktable `ashift.c` [LOC] (Line Segment Detector `ashift_lsd.c` [LOC], Nelder-Mead `ashift_nmsimplex.c` [LOC] optimising lens shift/rotation/shear with RANSAC on lines; modes "auto/vertical/horizontal/both") - this is explicitly an Upright analogue; RT `perspectivecorrection.cc` [LOC] (auto perspective).
- Because presets rarely include Upright (they're often in XMP `crs:PerspectiveUpright`, `PerspectiveVertical`... values, **which are post-solve parameters**), a renderer reading XMP can apply the *numerical* transform without re-solving, if we can reproduce Adobe's parameter->homography mapping (Vertical/Horizontal/Rotate/Aspect/Scale/X/Y are documented in behaviour; derive by measurement). Note: XMP also stores the `PerspectiveUpright` mode only; the solved homography is re-derived by LR/ACR at load - so stored `PerspectiveVertical` etc. may be zero for auto modes. Plan: emulate via ashift.
- Difficulty: manual transform **medium**; automatic Upright **hard** (match results approximately).

---

## 16. Effects: Vignette (post-crop), Grain, Dehaze (see 6)

- Vignette (Effects): Amount, Midpoint, Roundness, Feather, Highlights (Style: Highlight Priority / Color Priority / Paint Overlay). Post-crop => centred on cropped frame, radial function of normalised elliptical distance. Highlight Priority = applied in linear (exposure-like, with highlight-protecting tone shaping); Paint Overlay = blend toward black/white in gamma space; Color Priority similar to Highlight Priority but preserves colour (chroma ratio) **[INF; Adobe: https://helpx.adobe.com/camera-raw/desktop/edit-and-enhance-images/tone-and-color/vignette-grain-effects-camera-raw.html]**. Fit radial profile as smoothstep-like function of `(r - midpoint)/feather`. Candidates: darktable `vignette.c` [LOC] (scale/falloff/brightness/saturation; dithering), RT `vignette`(`improcfun.cc`). Difficulty: **easy-medium**.
- Grain: Amount, Size, Roughness. Official: adds film-like grain; synthetic, spatially-correlated noise with size-dependent blur and roughness controlling amplitude variation; weight by luminance (more in midtones) **[INF]**. Random seed per image in LR (stored? `crs:GrainSeed` exists in newer XMP; check). Candidates: darktable `grain.c` [LOC] (Perlin-like grain, ISO-based) , RT `ipgrain.cc` [LOC]. Exact match impossible without RNG; match statistics (power spectrum, amplitude vs luminance) only. Difficulty: **easy** (statistical), **impossible** (pixel-exact).

---

## 17. Masks (Linear / Radial gradients, Brush, Range masks), AI masks, Lens Blur

- Linear/Radial gradient: official Feather 0-100 (radial) and linear with start/mid/end; fall-off function undocumented; ACR stores `crs:CorrectionMasks` XMP (`CorrectionAmount`, `ZeroX/Y, FullX/Y` for linear; `Top/Left/Bottom/Right/Angle/Midpoint/Roundness/Feather/Flipped` for radial) - values in normalised crop-space coordinates **[PUB: XMP is parseable]**.
- Known/inferred: linear gradient: ramp from 0 at `Zero` to 1 at `Full` line with smooth (probably smoothstep / cubic) interpolation; radial: elliptical distance d with Feather F (0 = hard edge, 100 = fully soft: alpha = smoothstep over d in [1-F, 1] or similar) **[INF]**. Calibrate by rendering exposure +100 through a mask on a flat gray and reading the alpha profile (easy and exact).
- Range masks (Luminance/Color/Depth): luminance range min/max with smoothness; Color range via sample + range amount (distance in Lab). Fit by measurement.
- Brush: flow/feather/density; brush-stroke dab accumulation model; fidelity approximates.
- AI masks (Subject, Sky, Background, People, Objects, Select Landscape): closed neural networks (cloud, Adobe Sensei / `ImageSeg`). Open alternatives: Segment Anything (Meta SAM/SAM 2, Apache-2.0), U²-Net, MODNet, sky segmentation models; as a mask source produce a soft alpha; fidelity in *boundary* only. Presets (non-AI-mask) typically avoid AI masks unless they are presets that include masks (Adobe's newer preset format can store AI mask definitions as parametric requests, e.g. Sky, that re-run segmentation) - we can implement type recognition + our own model.
- Lens Blur (LR 2023): depth estimation (monocular depth net, e.g. Depth Anything-like open models) + defocus rendering with bokeh shape. Open: Depth Anything V2 (Apache-2.0 small, CC-BY-NC for large?), MiDaS; rendering: scatter-as-gather bokeh via disc kernel with depth-dependent radius. Difficulty: **hard**, low priority.
- Denoise AI: see §13.
- Difficulty summary: gradients **easy**, range masks **medium**, brush **medium**, AI masks **hard** (approximate boundaries), lens blur **hard**.

---

## 18. Fidelity strategy: calibration harness

### 18.1 Principles
- Treat Lightroom as an oracle: render controlled inputs with controlled settings and fit **our** parametric models (not copy code, not decompile).
- Work in scene-linear where possible: use synthetic **DNGs** (or linear TIFF -> DNG via DNG SDK `dng_validate`-like utility) so we control exactly the input RGB/CFA. Best: a custom synthetic DNG writer with `LinearRaw` photometric (3-channel, already demosaiced) and a known ColorMatrix (identity-ish w.r.t. ProPhoto) + Adobe "Linear" profile or a custom DCP with identity HueSatMap and linear tone curve. This removes demosaic, matrix and curve confounders.
- Export from LR as **16-bit TIFF in ProPhoto RGB (or Display P3/AdobeRGB), gamma 1.8** or sRGB; known TRC. Use 16-bit for fitting accuracy; disable sharpening on export, disable resize.

### 18.2 Test targets (all as synthetic DNG/TIFF + physical shot)
1. **Gray ramp** (256 or 1024 steps, scene-linear, 0.0001..16 so over-range tested) -> for Exposure, Contrast, Whites, Blacks, tone curve, parametric curve, Highlights/Shadows global component.
2. **Edge-bearing test patterns**: gray ramp with local content (low/high contrast checker squares inside bright and dark regions, blurred step edges, sinusoid gratings at multiple frequencies) -> Shadows/Highlights adaptivity, Clarity, Texture, Dehaze, sharpening MTF.
3. **Hue/Saturation wheel chart**: HSV sweep (360 hues x 16 S x 8 V) in ProPhoto linear -> Vibrance, Saturation, HSL, Calibration, Color Grading.
4. **ColorChecker 24 / 140 patch, IT8 / SG** (X-Rite ColorChecker Passport; Colour Science patches; real raw capture with a real camera to test full chain incl. DCP and WB).
5. **Real-world corpus**: ~40-100 raw images (public: raw.pixls.us samples (CC0), Adobe DNG test files, RawSamples) spanning skies, skin, foliage, night, backlit - render with each preset.
6. **Lens**: checkerboard & grid shots for 2-3 lens/camera combos with and without profile for LCP validation.
7. **Noise/grain**: flat gray ISO series and Poisson-noise synthetic; measure NPS before/after.

### 18.3 Sweep protocol
- One slider at a time: values {-100, -75, -50, -25, -10, 0, 10, 25, 50, 75, 100} (Exposure in EV: {-5..+5} step 0.5). All others default; Process Version 2012+ (current PV = 6). Script via XMP sidecars (`crs:Exposure2012`, etc.) -> batch export with Lightroom Classic (or ACR via Photoshop actions / `adobe-dng-converter`? DNG Converter doesn't render). Interaction sweeps: pairs for known couplings (Highlights x Whites, Shadows x Blacks, Contrast x Exposure, Vibrance x Saturation).
- Record per-slider *response surface*: for global controls, build 1-D transfer function (input patch value -> output) for each slider value; for local controls, response as functions of (pixel value, local mean, local contrast).
- Fit: parametric model per control; minimise `sum ΔE2000(Lab(out_LR), Lab(out_ours))` (CIEDE2000, via `colour-science`) + gradient/edge term (SSIM/LPIPS on edge patterns) using Nelder-Mead / L-BFGS-B / CMA-ES; global piecewise controls fit via splines; for local ones fit LLF `sigma`, `alpha`, pivot, pyramid levels weighting. Target: median ΔE00 < 1, p95 < 3 on corpus (ΔE00 < 1 is generally imperceptible).
- **Preset-level validation**: render real purchased presets (user-provided; not redistributed) on the corpus in LR and in ours; compute ΔE00 maps; use as final regression suite and to find which controls dominate error (e.g. via leave-one-control-out ablation: replace our control with LR's rendered *intermediate*? Not available; instead use the sweep predictions).

### 18.4 How much matching is feasible
Rough expectation (my estimate, not measured):
- **Near-exact (ΔE00 < 1)**: WB (given matrices), Exposure (mid-range), point curve, DCP profiles (HueSatMap/LookTable/ToneCurve) if the user's own DCPs are used, Vibrance/Saturation/HSL/Calibration after fitting, Vignette, gradient masks, color grading (after fitting masks).
- **Close (ΔE00 1-3, visual match in most images)**: Contrast, Whites/Blacks, Clarity, Dehaze, Sharpening, Grain (statistical), lens profile corrections (needs user's profile).
- **Approximate (visible differences in some images)**: Highlights/Shadows in high dynamic range scenes, Texture, NR, Upright auto, AI masks, Lens Blur, Denoise AI, demosaic micro-contrast.
- Cameras without DCP/ColorMatrix: unmatched colour. Camera matrices are the biggest invisible determinant: Adobe's "Adobe Standard" colour comes from per-camera profiles; use user-provided DCP/DNG-converted files or camera color matrices (Adobe DNG converter-produced DNGs embed ColorMatrix/ForwardMatrix, which is the legal path for supplying them).
- Presets that rely on specific Adobe profile (e.g. "Adobe Color" LookTable) must have the user supply the DCP or fallback to approximate; keep that explicit in the UI.

### 18.5 Legal notes on black-box comparison (not legal advice)
- Observing the *outputs* of Lightroom under controlled inputs and fitting independent models is generally lawful black-box/clean-room analysis; do **not** decompile, disassemble, or inspect Adobe binaries/lens profiles/internal tables, and do not copy Adobe DCP/LCP files into the distribution. Check the Lightroom EULA (https://www.adobe.com/legal/terms.html) clauses on reverse engineering/benchmarks and that purchases of presets allow use outside Adobe (preset licences vary - some forbid conversion; recommend reading the user's own presets locally, never redistributing converted presets).
- Maintain a documented clean-room process: two roles (measurer produces response data; implementer works from data + published papers/spec only); record sources; avoid contact with leaked/decompiled Adobe code.
- Using the Adobe DNG SDK is permitted under its own EULA (see link above), but it is not an open-source licence: keep it as a separate optional dependency or re-implement from the DNG spec. GPL darktable/RawTherapee code can be studied and reused only if our project is GPL-compatible (decide the licence early; darktable is GPL-3, RawTherapee is GPL-3). If the project is, e.g., MIT/Apache, port from the papers, not from these.
- Patents: check Adobe and third-party patents (local Laplacian, dark channel prior - Microsoft US 8,340,461 reportedly lapsed; bilateral grid; Adobe dehaze/learning patents) in the jurisdictions of distribution.
- XMP/preset formats: the XMP `crs:` namespace is documented by Adobe's XMP spec and ExifTool/Lightroom SDK docs; parsing is fine. Lightroom Classic SDK is permitted for plugins only.

---

## 19. Priority order (by fidelity per effort)
1. DNG/DCP pipeline exact (WB, matrices, HueSatMap/LookTable, tone curve, ProPhoto working space). 
2. Tone curve (spline) + Exposure/Contrast/Whites/Blacks global model.
3. HSL/Vibrance/Saturation/Color Grading/Calibration fits.
4. Highlights/Shadows (LLF-based) - biggest risk, invest in the sweep harness early.
5. Clarity/Texture/Dehaze.
6. Lens (LCP), Vignette, Grain, Sharpening, NR.
7. Masks (gradients first), then AI masks and Upright.

## 20. Open questions to verify
- Exact SDK licence text (patent grant terms) and whether `dng_render.cpp` constants (ACR3 default curve table) may be redistributed.
- Adobe's internal working space TRC for the curve/histogram (ProPhoto-primaries with sRGB TRC vs gamma 1.8).
- Whether PV2012 Whites/Blacks are purely global.
- Patent identification for Adobe PV2012/Clarity/Dehaze (Google Patents assignee search).
- Official Adobe pages blocked by 403 for WebFetch (helpx.adobe.com/camera-raw/using/*) - collect exact wording manually.
