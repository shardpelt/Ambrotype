# 02 - Lightroom / Camera Raw preset and profile formats

Goal: an open-source photo editor that imports and faithfully applies commercially sold Lightroom presets.
Status: research notes. Confidence flags: [V] verified this session from a cited source or local code, [K] from prior knowledge / experience with real files (verify against a corpus before relying on it), [?] uncertain.

---------------------------------------------------------------------------
## 1. Formats

### 1.1 Legacy `.lrtemplate` (Lightroom Classic up to 7.2, still readable by LrC today and by Lightroom mobile import)

Plain text, a Lua table literal (not JSON). One file = one preset, assigned to a global `s`. [V: lightroomqueen / metafilter threads, many parser repos]
Structure: `id` (UUID, sometimes absent), `internalName` (display name), `title` (display name, may be a `LOC("$$$/...")` string for Adobe-supplied presets), `type = "Develop"`, `value = { settings = {...}, uuid = "..." }`, `version = 0`.
Values: numbers, quoted strings, booleans, nested tables. Tone curves are flat number arrays (`ToneCurvePV2012 = { 0, 0, 32, 22, 255, 255, }`), i.e. x,y pairs flattened. Keys have **no `crs:` prefix**; they are the same names as the XMP tags (`Exposure2012`, `Contrast2012`, `ToneCurvePV2012`...). Lua features seen: trailing commas, `--` comments, `[1]=` style keys (rare), `\"` escapes, `LOC()` calls.

```lua
s = {
	id = "3E6E5A06-A5C3-4F9E-8F5E-0A2B7C0F1D11",
	internalName = "Warm Matte",
	title = "Warm Matte",
	type = "Develop",
	value = {
		settings = {
			ProcessVersion = "11.0",
			Version = "13.0",
			WhiteBalance = "As Shot",
			Exposure2012 = 0.35,
			Contrast2012 = -12,
			Highlights2012 = -40,
			Blacks2012 = 18,
			Vibrance = 10,
			ToneCurveName2012 = "Custom",
			ToneCurvePV2012 = {
				0, 18,
				64, 70,
				192, 200,
				255, 245,
			},
			SplitToningShadowHue = 210,
			SplitToningShadowSaturation = 18,
			ConvertToGrayscale = false,
		},
		uuid = "3E6E5A06-A5C3-4F9E-8F5E-0A2B7C0F1D11",
	},
	version = 0,
}
```
(Synthetic example, structure per [V] search results; values illustrative.)

Parsing advice: write a small tokenizer/recursive-descent parser for the Lua table subset (do NOT embed a Lua VM for untrusted files); handle `LOC("...")`, comments, nested tables, both `key = v` and `["key"] = v`. Settings that existed only in `.lrtemplate` era but have no XMP-era twin: none significant - the XMP preset is a straight re-serialisation. In .lrtemplate, settings absent from `settings` mean "do not touch" (same as XMP presets).
Local corrections inside `.lrtemplate` appear as nested tables (`GradientBasedCorrections = { { CorrectionAmount = 1, LocalExposure = 0.5, ... } }`); AI/mask-group corrections only exist in the XMP era.

### 1.2 Modern `.xmp` develop presets (Lightroom Classic >= 7.3, ACR >= 10.3, Lightroom CC/mobile "user presets")

An XMP packet (`x:xmpmeta` / `rdf:RDF` / one `rdf:Description`) with namespace `crs` = `http://ns.adobe.com/camera-raw-settings/1.0/`. Most scalars are XML **attributes** on rdf:Description; arrays (`rdf:Seq`), language alternatives (`rdf:Alt`) and structs (nested `rdf:Description` or `rdf:parseType="Resource"`) are child **elements**. Both attribute and element forms are legal RDF and must both be parsed (darktable parses `//crs:*` and `//@crs:*` for this reason, lightroom.c ~L1300). Old Lr up to 6 wrote attributes only.

Real minimal example [V, https://github.com/clebert/lightroom-preset/blob/main/example.xmp]:
```xml
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:PresetType="Normal"
    crs:UUID="6720197431C54003A9972ADBBA80670E"
    crs:Version="13.3" crs:ProcessVersion="11.0"
    crs:ToneCurveName2012="Medium Contrast"
    crs:ParametricShadowSplit="25" crs:ParametricMidtoneSplit="50" crs:ParametricHighlightSplit="75"
    crs:WhiteBalance="As Shot" crs:Sharpness="40" crs:Texture="30" crs:Clarity2012="20"
    crs:ColorNoiseReduction="25" crs:LensProfileEnable="1">
   <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Example</rdf:li></rdf:Alt></crs:Name>
   <crs:ToneCurvePV2012><rdf:Seq>
     <rdf:li>0, 0</rdf:li><rdf:li>32, 22</rdf:li><rdf:li>128, 128</rdf:li><rdf:li>255, 255</rdf:li>
   </rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
```
Typical commercial preset adds (from memory of real files [K]): `crs:SupportsAmount="True" crs:SupportsColor="True" crs:SupportsMonochrome="True" crs:SupportsHighDynamicRange="True" crs:SupportsNormalDynamicRange="True" crs:SupportsSceneReferred="True" crs:SupportsOutputReferred="True" crs:CameraModelRestriction="" crs:Copyright="" crs:ContactInfo="" crs:HasSettings="True"`, a `crs:Group` lang-alt, optionally `crs:Cluster`, and for preset-embedded profiles a `crs:Look` struct / `crs:RGBTable`. The `<?xpacket ...?>` wrapper may or may not be present; tolerate leading BOM and `xpacket` PIs.

### 1.3 Presets vs. sidecar XMP

| | Develop preset .xmp | Sidecar .xmp (per photo) |
|---|---|---|
| Identity | `crs:PresetType`, `crs:UUID`, `crs:Name`, `crs:Group`, `crs:Support*` | `crs:RawFileName`, `crs:HasSettings`, `xmp:*`, `dc:*`, `exif:*`, `tiff:*`, `xmpMM:*`, `lr:hierarchicalSubject`, rating/label |
| Content | **Sparse**: only the settings the author wanted applied (a "partial" preset). Absent = leave untouched. | **Complete** snapshot incl. crop (`CropTop/...`, `HasCrop`), WB as shot, `crs:History` (xmpMM-style history), `Orientation`, spot-heal `RetouchAreas`, etc. |
| Crop/WB/exposure | Usually excluded (Lr UI lets the author choose groups) | Always |
| Parser | Same crs: namespace; same tag semantics | Same |
Implication: one parser for crs: tags, two "intents" (apply-as-delta with mask of present keys vs. replace-all). A preset is not "default + overrides"; it is "set only these keys". Note Lightroom's own **Reset** step before applying differs depending on the feature (applying a preset does not reset others).

### 1.4 Preset header fields

| Tag | Meaning |
|---|---|
| `crs:PresetType` | `Normal` (develop preset), `Look` (Creative profile/"Look"), `Camera` (not shown to users as presets; profile-like), `Defaults`? (`crd:` namespace holds Camera Raw Defaults - see exiftool forum "XMP-crd"), `Monochrome`? [?]. Real files seen: "Normal", "Look". |
| `crs:UUID` | 32-hex uppercase GUID without dashes; identity for dedupe and for reference from Look. (lrtemplate uses dashed 36-char.) |
| `crs:Name` | `rdf:Alt` lang-alt; `x-default` always, plus `xml:lang="en-US"` etc. Pick UI language, fall back to x-default. (`crs:LookName` flat string in Look struct.) |
| `crs:Group` | lang-alt, folder name shown in panel. `crs:Cluster` = grouping for profile browser. |
| `crs:SupportsAmount` | True => preset can be scaled by an Amount slider 0..200 (Lr: 0-200 for profiles/presets since 2020; <=100 attenuation, >100 extrapolation). Also `crs:SupportsAmount2` seen on Look/profile structs [K]. |
| `crs:SupportsColor`, `crs:SupportsMonochrome` | Whether preset is offered for colour / B&W photos. (If photo is B&W and `SupportsMonochrome=False`, Lr greys it out.) |
| `crs:SupportsHighDynamicRange`, `crs:SupportsNormalDynamicRange` | HDR-edit-mode applicability (photo in HDR mode vs SDR). `crs:HDREditMode` per photo: 0/1. |
| `crs:SupportsSceneReferred` / `SupportsOutputReferred` | Whether profile applies to scene-referred (raw) vs. output-referred (JPEG/TIFF) sources. [K] |
| `crs:CameraModelRestriction` | `rdf:Seq`/string of camera model names the preset may be applied to (used by camera-matching presets). Empty = all. |
| `crs:Version` | Camera Raw version string that wrote it ("13.3"), also appears as `crs:CompatibleVersion` (hex, e.g. "15400960") in newer files. Informational; **do not gate on it** other than for unknown-tag warnings. |
| `crs:ProcessVersion` | Develop pipeline generation: "5.7"(PV2003), "6.7"(PV2010), "11.0"(PV2012), ... newer "15.4"? for recent ACR [?] - see section 4. Presets pin it; Lr converts the photo to that PV when the preset has it (with a warning dialog). |
| `crs:HasSettings` | True if file carries adjustments. |
| `crs:Copyright`, `crs:ContactInfo` | author-supplied strings. |

Preset **Amount** behaviour [K, Adobe help "Apply presets"; exact maths not published]: Lightroom (since 2019/2020) scales each *numeric* setting linearly between the photo's pre-existing value and the preset value: `v = v0 + amount*(vpreset - v0)`, amount in 0..1 (and to 2.0 on newer builds, extrapolating). Non-numeric settings (booleans, enum strings, profile names) switch on when amount>0 (or at 100%?). Tone curve points are scaled toward identity (linear). Profile Amount (Look) scales the table/profile effect (interpolating the HueSatMap/LookTable/RGBTable toward identity; `crs:LookAmount`). Masks scale their local sliders. Practical approach: implement as lerp on numeric params relative to defaults; curves lerp control point y toward y=x; LUTs lerp toward identity. Verify empirically against Lr renders at 50%.

---------------------------------------------------------------------------
## 2. crs: tags (as used by presets; PV2012+ unless noted)

Sources: Adobe XMP namespace page https://developer.adobe.com/xmp/docs/xmp-namespaces/crs/ (documents only the OLD PV2003-era tags: Exposure, Brightness, Contrast, Shadows, ToneCurve, Crop*, Vignette*...), exiftool https://exiftool.org/TagNames/XMP_with_ids.html (most complete public list), exiv2 https://exiv2.org/tags-xmp-crs.html, go-xmp crs package, and darktable's lightroom.c. Ranges/defaults below are mostly [K] from UI behaviour; "unbounded" means UI limits as shown. Defaults are the "no adjustment" value, except WB.

### 2.1 Basic / tone (Basic panel)
| Tag | Type | Range | Default | Meaning / UI |
|---|---|---|---|---|
| `WhiteBalance` | string | As Shot, Auto, Daylight, Cloudy, Shade, Tungsten, Fluorescent, Flash, Custom | As Shot | WB dropdown. "As Shot" = use camera-recorded; presets with "As Shot" must NOT override. |
| `Temperature` | int | raw: 2000-50000 K; non-raw (JPEG/TIFF) uses a relative scale ~ -100..100 | as shot | Temp slider. Only meaningful with WhiteBalance=Custom. |
| `Tint` | int | -150..150 | as shot | Tint slider. |
| `IncrementalTemperature`, `IncrementalTint` | int | ~ -100..100 | 0 | Relative WB shifts (used by presets/auto-sync "relative" WB, camera-neutral presets). Apply as delta to the photo's WB, converted from slider units to mired-like shift [?]. |
| `Exposure2012` | real | -5.0..+5.0 | 0 | EV; Exposure slider. (PV2003/2010: `Exposure`.) |
| `Contrast2012` | int | -100..100 | 0 | Contrast |
| `Highlights2012` | int | -100..100 | 0 | Highlights |
| `Shadows2012` | int | -100..100 | 0 | Shadows |
| `Whites2012` | int | -100..100 | 0 | Whites |
| `Blacks2012` | int | -100..100 | 0 | Blacks |
| `Texture` | int | -100..100 | 0 | Texture (ACR 10.3+). |
| `Clarity2012` | int | -100..100 | 0 | Clarity (PV2012). Old PV: `Clarity`. |
| `Dehaze` | int | -100..100 | 0 | Dehaze. |
| `Vibrance` | int | -100..100 | 0 | Vibrance. |
| `Saturation` | int | -100..100 | 0 | Saturation. |
| `AutoExposure`/`AutoContrast`/`AutoBrightness`/`AutoShadows`, `AutoTone` | bool | | False | "Auto" button state (AutoTone=True in a preset means compute auto at apply time; image-dependent!). |
| `Exposure`, `Contrast`, `Brightness`, `Shadows`, `ShadowTint` | legacy | Brightness 0..150, Contrast -50..100, Shadows 0..100 | | PV2003/2010 Basic panel; `ShadowTint` -100..100 (Camera Calibration shadows tint) |
| `Defringe`-family see 2.8 | | | | |

### 2.2 Tone curve
| Tag | Type | Notes |
|---|---|---|
| `ToneCurvePV2012` | rdf:Seq of "x, y" strings (also Red/Green/Blue: `ToneCurvePV2012Red`, `...Green`, `...Blue`) | 0..255 both axes, point list ascending x, includes endpoints. Interpolation: spline (Lr uses a monotone-ish cubic through points; exact formula not public [?] - Hermite/natural cubic spline over points gives close results). Defaults `0,0 / 255,255`. In `.lrtemplate`: flat `{x,y,x,y,...}`. Curve operates on gamma-ish/sRGB-encoded display-referred values after the tone ops [K]. |
| `ToneCurveName2012` | string | "Linear", "Medium Contrast", "Strong Contrast", "Custom". **Name may be set with `ToneCurvePV2012` omitted** (name implies built-in points; Medium Contrast = approx 0,0 / 32,22 / 64,56 / 128,128 / 192,196 / 255,255; Strong Contrast = 0,0 / 32,16 / 64,50 / 128,128 / 192,202 / 255,255 [K - verify]). Importers should honour the name if no points. |
| `ParametricShadows`, `ParametricDarks`, `ParametricLights`, `ParametricHighlights` | int -100..100 | Parametric curve region sliders. |
| `ParametricShadowSplit`, `ParametricMidtoneSplit`, `ParametricHighlightSplit` | int 0..100 | Region boundaries; defaults 25/50/75. |
| `ToneCurve`, `ToneCurveRed/Green/Blue`, `ToneCurveName` | legacy PV2003/2010 | Same seq format, 0..255. |

### 2.3 Colour (HSL / Color Mixer, Color Grading, Calibration)
| Tag | Type | Range | Meaning |
|---|---|---|---|
| `HueAdjustment{Red,Orange,Yellow,Green,Aqua,Blue,Purple,Magenta}` | int | -100..100 | HSL hue shift per band. |
| `SaturationAdjustment*` | int | -100..100 | HSL saturation. |
| `LuminanceAdjustment*` | int | -100..100 | HSL luminance. |
| `ColorGradeShadowHue/Sat/Lum`, `ColorGradeMidtoneHue/Sat/Lum`, `ColorGradeHighlightHue/Sat/Lum` (also written as `ColorGradeShadowLum`, `ColorGradeHighlightLum`, `ColorGradeMidtoneLum`) | int | Hue 0..359, Sat 0..100, Lum -100..100 | Color Grading (ACR 13 / Lr 10.4+) 3-way wheels. |
| `ColorGradeGlobalHue/Sat/Lum` | int | same | Global wheel. |
| `ColorGradeBlending` | int | 0..100, default 50 | Blending. |
| `ColorGradeBalance` -> actually `SplitToningBalance` (shared) | int | -100..100 | Balance (shared with legacy). |
| `SplitToningShadowHue/ShadowSaturation/HighlightHue/HighlightSaturation/Balance` | int | Hue 0..359, Sat 0..100, Bal -100..100 | Legacy Split Toning; still written for Color Grading shadows/highlights wheels (mirrored). When ColorGrade* present, prefer them. |
| `ConvertToGrayscale` | bool | | B&W conversion on (HSL panel becomes B&W Mix). |
| `GrayMixer{Red,Orange,Yellow,Green,Aqua,Blue,Purple,Magenta}` | int | -100..100 | B&W mixer channel weights. |
| `RedHue,RedSaturation,GreenHue,GreenSaturation,BlueHue,BlueSaturation` | int | -100..100 | Camera Calibration primaries. |
| `ShadowTint` | int | -100..100 | Calibration shadows tint. |
| `PointColors` | rdf:Seq of strings | | Point Color (Color Mixer "point sampler", 2024). Each li: 9 comma-separated floats: sample hue, sat, lum (source), then range/shift params (hue shift, sat shift, lum shift, ranges) [?]. Proprietary algorithm. |
| `ColorGradeGlobal*` etc. | see above | | |
| `LookTable`-like: see 3.3 | | | |

### 2.4 Detail (sharpening, NR)
| Tag | Range | Default | Notes |
|---|---|---|---|
| `Sharpness` | 0..150 | 25 (raw default) | Amount |
| `SharpenRadius` | 0.5..3.0 | 1.0 | |
| `SharpenDetail` | 0..100 | 25 | |
| `SharpenEdgeMasking` | 0..100 | 0 | |
| `LuminanceSmoothing` | 0..100 | 0 | Luminance NR |
| `LuminanceNoiseReductionDetail` | 0..100 | 50 | |
| `LuminanceNoiseReductionContrast` | 0..100 | 0 | |
| `ColorNoiseReduction` | 0..100 | 25 | |
| `ColorNoiseReductionDetail` | 0..100 | 50 | |
| `ColorNoiseReductionSmoothness` | 0..100 | 50 | |
| `NoiseReduction`-AI ("Denoise" `crs:EnhanceDetails`?) | | | AI Denoise / Enhance are NOT preset-able via simple tags (they write to DNG/need generation) [?]. |

### 2.5 Optics / geometry
| Tag | Notes |
|---|---|
| `LensProfileEnable` (0/1), `LensProfileSetup` ("Auto"/"Default"/"Custom"), `LensProfileName`, `LensProfileFilename`, `LensProfileDigest`, `LensProfileIsEmbedded`, `LensProfileDistortionScale`, `LensProfileChromaticAberrationScale`, `LensProfileVignettingScale` (0..200) | Lens-profile corrections. Requires Adobe lens profile database (proprietary, `.lcp` files; format is XML = public via DNG-SDK-adjacent LCP spec; Lensfun is the open alternative). |
| `LensManualDistortionAmount` (-100..100) | Manual distortion. |
| `AutoLateralCA` (0/1) | Remove chromatic aberration. `ChromaticAberrationR/B` legacy. |
| `DefringePurpleAmount`, `DefringePurpleHueLo`, `DefringePurpleHueHi`, `DefringeGreenAmount`, `DefringeGreenHueLo/Hi` | Remove fringe. Hue ranges 0..100 internal units (defaults purple 30..70, green 40..60). |
| `PerspectiveUpright` | int enum 0 Off, 1 Auto, 2 Level, 3 Vertical, 4 Full, 5 Guided (6 = "Auto/Legacy"? [?]) |
| `PerspectiveVertical`, `PerspectiveHorizontal` (-100..100), `PerspectiveRotate` (-10..10), `PerspectiveScale` (50..150, default 100), `PerspectiveAspect` (-100..100), `PerspectiveX`, `PerspectiveY` (-100..100 offsets) | Transform panel. Upright state also carries `UprightVersion`, `UprightCenterMode`, `UprightFocalMode`, `UprightFocalLength35mm`, `UprightPreview`, `UprightTransform_0..5`, `UprightFourSegments_*` — image-specific analysis, should be ignored in presets except manual sliders. |
| `PostCropVignetteAmount` (-100..100), `PostCropVignetteMidpoint` (0..100, 50), `PostCropVignetteFeather` (0..100, 50), `PostCropVignetteRoundness` (-100..100, 0), `PostCropVignetteStyle` (1 Highlight Priority, 2 Color Priority, 3 Paint Overlay), `PostCropVignetteHighlightContrast` (0..100) | Effects > Vignetting (after crop). |
| `VignetteAmount`, `VignetteMidpoint` | Lens-vignetting legacy (pre-crop). |
| `GrainAmount` (0..100), `GrainSize` (0..100, 25), `GrainFrequency` (0..100, 50), `GrainSeed` | Grain. |
| `CropTop/Left/Bottom/Right/Angle`, `HasCrop`, `CropConstrainToWarp`, `CropWidth/Height/Units` | Crop; **sidecar only**, not normally in presets. Presets that include `CropAngle` etc. should be ignored/optionally applied. |
| `Orientation`/`Rotation` | sidecar. |

### 2.6 Profile / look
| Tag | Notes |
|---|---|
| `CameraProfile` | string name of camera profile ("Adobe Standard", "Camera Standard", "Camera Vivid"...) - the "Profile" dropdown (ACR 'Camera Calibration' profile for legacy). `CameraProfileDigest` = MD5 of the profile for matching. |
| `Look` | struct (rdf:Description inside element `crs:Look`): `Name`, `Amount` (0..2? 1.0 = 100%), `UUID`, `SupportsAmount`, `SupportsMonochrome`, `Cluster`, `Group` (lang-alt), `Copyright`, `Parameters` (struct containing a self-contained crs settings subset: ProcessVersion, CameraProfile, ToneCurvePV2012, ColorGrade*, `RGBTable`+`Table_<md5>` ...). The Look is Adobe's "Profile" (Creative profile). A preset typically includes the whole `crs:Look` so it is self-contained. Exiftool names: `Look`, `LookName`, `LookAmount`, `LookCluster`, `LookCopyright`, `LookGroup`, `LookParameters`. |
| `RGBTable`, `RGBTableAmount`, `RGBTableDigest`? | in `Parameters`: the 3D LUT identity and strength (`Table_<digest>` carries the data). See 3.3. |
| `HDREditMode` | int 0/1; photo in HDR editing mode. |
| `HDRMaxValue` | real (HDR headroom, e.g. 1.0 to 8) [?]. |
| `ProcessVersion`, `Version`, `CompatibleVersion` | see 1.4 |
| `OverrideLookVignette` | bool, ties Look vignette. [?] |
| `ToneMapStrength`, `Dehaze`... | `ToneMapStrength` HDR/tone map control for Look [?]. |

### 2.7 Local corrections (Masking)

Layers of the data model:
1. Legacy (PV2003-2010 + PV2012 pre-Oct 2021): `crs:GradientBasedCorrections` (`rdf:Seq` of correction structs; `What="Correction"`, `ZeroX/ZeroY/FullX/FullY` linear gradient endpoints, `CorrectionMasks` seq), `crs:CircularGradientBasedCorrections` (`Top/Left/Bottom/Right/Angle/Midpoint/Roundness/Feather/Flipped`), `crs:PaintBasedCorrections` (`CorrectionMasks` entries with `What="Mask/Paint"`, `Radius`, `Flow`, `Center`, `Dabs` as `rdf:Seq` of strings like "d 0.5 0.4 0.03 1" / "m x y" / "l x y"). Slider tags are flattened in the correction struct: `LocalExposure2012`, `LocalContrast2012`, etc.
2. Current (Lr Classic 11, ACR 14, Oct 2021+): `crs:MaskGroupBasedCorrections` -> seq of correction structs, each:
   - `What="Correction"`, `CorrectionAmount`(0..1), `CorrectionActive`, `CorrectionName`, `CorrectionSyncID`, `LocalExposure2012`,... (all Local* sliders), `LocalCorrectionRange`?, `LocalPointColors`, `LocalCurveRefineSaturation`, `CorrectionMasks` (rdf:Seq of mask components), optionally `CorrectionRangeMask` struct at correction level.
   - Mask component (rdf:Description in Seq): `What` = `Mask/CircGradient` (radial), `Mask/Gradient` (linear), `Mask/Paint` (brush), `Mask/Image` (AI / semantic), `Mask/RangeMask` (luminance/color/depth range - the `CorrectionRangeMask` sub-struct with `Version`, `Type` (1 color, 2 luminance, 3 depth), `ColorAmount`, `LumRange` "min feather max feather" 4 floats, `LumMapSamples`, `DepthRange`, `Invert`, `SampleType`, `PointModels` for colour samples), plus `MaskActive`, `MaskName`, `MaskBlendMode` (0 add, 1 subtract, 2 intersect?, 3 ... [?]), `MaskInverted`, `MaskValue` (feather/opacity 0..1), `MaskSyncID`, geometry (`Top/Left/Bottom/Right/Angle/Midpoint/Roundness/Feather/Flipped` for radial; `ZeroX/ZeroY/FullX/FullY` for linear), `Dabs` for brush, and for AI: `MaskSubType` (1 = Select Subject, 0 = ? object/other, 2 = Select Sky, 3 = Background, 4 = ... Objects/people-part types [?]), `MaskSubCategoryID` (people part ids e.g. face skin, eyes, hair...[?]), `ReferencePoint`, and `MaskVersion`. [V partially: https://github.com/LrGenius/LrGeniusAI/pull/390 - fields What=Mask/Image, MaskSubType=1, ReferencePoint, Mask/RangeMask with CorrectionRangeMask Version=3 Type=2 LumRange="0 0.2 0.8 1"; MaskInverted; MaskBlendMode; MaskSyncID; CorrectionSyncID]
   - **AI masks are stored as semantic recipe, not pixels**: type id + reference point, and the actual bitmap is regenerated by Adobe's ML models on each image when a preset is applied (Adobe docs/community: "masks that include AI masks will be recomputed"; also https://community.adobe.com/t5/photoshop-ecosystem-discussions/ai-camera-raw-masks-not-re-computed-when-used-in-an-action/td-p/14176133). Photo-embedded sidecar may additionally store `crs:MaskGroupBasedCorrections` with a `MaskDigest`/`...Data` (a cached mask in a base64 `crs:CorrectionMaskData`? [?]). Presets do not carry bitmaps, so we need our own subject/sky segmenter (open models: U2Net/SAM/ DeepLab / sky-seg) to approximate.
3. Local slider tags (all in correction struct; defaults 0 unless noted; ranges -1..1 for 2012 float-based unless noted):
   `LocalExposure2012` (-4..4 EV), `LocalContrast2012`, `LocalHighlights2012`, `LocalShadows2012`, `LocalWhites2012`, `LocalBlacks2012` (-1..1 representing -100..100), `LocalTexture`, `LocalClarity2012`, `LocalDehaze`, `LocalHue`, `LocalSaturation`, `LocalLuminance`? (`LocalLuminanceNoise`), `LocalSharpness`, `LocalMoire`, `LocalDefringe`, `LocalTemperature`, `LocalTint`, `LocalToningHue`, `LocalToningSaturation`, `LocalGrain`, `LocalCurveRefineSaturation`, `LocalPointColors`, `LocalCurve` / `LocalCurveRed/Green/Blue` (tone-curve per mask), `LocalBrightness`/`LocalExposure`/`LocalContrast`/`LocalClarity`/`LocalShadows` (legacy), `LocalWhat`? `LocalAmount`. Note: unit scaling: local 2012 values are typically stored as fractions (e.g. `LocalExposure2012="0.350000"`, `LocalHighlights2012="-0.4"`), where the UI shows x100 (except exposure, EV).
4. `crs:RetouchAreas` (rdf:Seq): spot heal/clone: `SpotType` (clone/heal), `SourceState`, `Method`, `SourceX`, `OffsetY`, `Opacity`, `Feather`, `Seed`, `Radius`... image-specific: ignore in presets (and Adobe does not allow healing in presets), but sidecars have them. darktable maps to `spots`.
5. Misc non-slider local: `crs:ImageBasedCorrections`? (older).

### 2.8 Other tags likely seen in preset corpora
`crs:AutoLateralCA`, `crs:Defringe*`, `crs:EnableCalibration`, `crs:EnableColorAdjustments`, `crs:EnableCircularGradientBasedCorrections`, `crs:EnableGradientBasedCorrections`, `crs:EnableDetail`, `crs:EnableGrayscaleMix`, `crs:EnableLensCorrections`, `crs:EnableEffects`, `crs:EnableSplitToning`, `crs:EnableToneCurve`, `crs:EnableTransform`, `crs:EnableRedEye`, `crs:EnableRetouch`, `crs:EnablePaintBasedCorrections`, `crs:EnableMaskGroupBasedCorrections`, `crs:EnableProfileCorrections` (Enable* = panel toggle booleans: honour them), `crs:UprightVersion`, `crs:UprightCenterMode`, `crs:HDRMaxValue`, `crs:ToneMapStrength`, `crs:ColorGrade*`, `crs:AutoWhiteVersion`, `crs:LensBlur*` (Lens Blur ACR 14.x: `LensBlurActive`, `LensBlurAmount`, `LensBlurFocalRange`, `LensBlurCatEye`, `LensBlurHighlightsBoost`, `LensBlurBokehShape`...) [?] - requires depth map, not preset-able in image-independent way.
(Names in 2.8 are from memory [K/?]; validate against a real corpus: tool = histogram of all `crs:` attribute and element names over >=200 free presets.)

### 2.9 Value conventions to implement
- Booleans: `"True"`/`"False"` strings (XMP), `0/1` for some (LensProfileEnable, AutoLateralCA, ConvertToGrayscale appears `True/False`). Accept both, case-insensitively.
- Reals may have explicit sign `+0.50` (Lr writes `+` for positive numbers in attributes: `crs:Exposure2012="+0.35"`). Parse `+`.
- Locale: always `.` decimal.
- Seq of ints for curves vs. strings "x, y".
- Namespaces: older files also use `crs` prefix bound to the same URI; always compare namespace URI, not prefix. Other namespaces in sidecars: `crd` (Camera Raw Defaults), `lr` (hierarchicalSubject), `xmpMM` (history), `photoshop`, `aux`.

---------------------------------------------------------------------------
## 3. Camera profiles

### 3.1 DCP (DNG Camera Profile)
Binary container in TIFF structure with a header magic `IIRC` (little endian; "MMCR" big endian) instead of `II*\0` (DNG spec 1.6: DNG Camera Profile format, chapter "Camera Profiles" / tags in "Mapping Camera Color Space to CIE XYZ Space"). Same tag definitions as in DNG IFD0: spec https://helpx.adobe.com/camera-raw/digital-negative.html (PDF "DNG Specification 1.7.x"). Tags (DNG spec section refs by name; numbers from spec) [K]:
- `ProfileCalibrationSignature` (50932), `ProfileName` (50936), `ProfileEmbedPolicy` (50941: 0 allow copying, 1 embed if used, 2 embed never, 3 no restrictions), `ProfileCopyright` (50942), `ProfileHueSatMapDims` (50937: Hue, Sat, Val divisions), `ProfileHueSatMapData1/2` (50938/50939), `ProfileHueSatMapEncoding` (51107: 0 linear, 1 sRGB), `ProfileLookTableDims` (50981), `ProfileLookTableData` (50982), `ProfileLookTableEncoding` (51108), `ProfileToneCurve` (50940: float pairs, in/out, applied after HSM and LookTable; DNG-level default curve when absent), `DefaultBlackRender` (51110: 0 auto, 1 none), `BaselineExposureOffset` (51109), `ColorMatrix1/2` (50721/50722) XYZ->camera, `CalibrationIlluminant1/2` (50778/50779: LightSource enum: 17 = Standard A, 21 = D65, 20=D55, 22=D75, 23=D50...), `ForwardMatrix1/2` (50964/50965: camera white-balanced -> XYZ D50), `ReductionMatrix1/2` (50725/50726), `AsShotProfileName` (50934), `UniqueCameraModel`, `PreviewColorSpace`.
- Processing (DNG spec "Mapping Camera Color Space to CIE XYZ Space"): given camera neutral (as-shot WB), estimate WB temperature -> interpolate between illuminant 1 & 2 by **inverse correlated colour temperature (mired)** weights; interpolate ColorMatrix (and ForwardMatrix) accordingly (iterating, since the neutral depends on matrix); HueSatMap interpolated the same way; apply HSM in ProPhoto-linear HSV (encoding 0 linear), then exposure, then LookTable, then ProfileToneCurve. Reference implementation: Adobe DNG SDK `dng_color_spec.cpp`, `dng_camera_profile.cpp`, `dng_hue_sat_map.cpp`, `dng_render.cpp`; open implementations: RawTherapee `rtengine/dcp.cc` (reads DCP, applies HSM/LookTable/ToneCurve/ForwardMatrix, interpolation), darktable `colorin` (DCP not applied except matrices; uses own ICC/matrix), dcamprof (Anders Torger, `dcamprof` tool, can make/convert DCP <-> JSON), LibRaw for matrices.
- DCP files are **tiny and shareable** (ProfileEmbedPolicy governs); copyright of Adobe's shipped profiles though: see 3.4.

### 3.2 Adobe XMP-based profiles (`.xmp` in `CameraProfiles/` or `Settings/` folders; "Enhanced" profiles, Look profiles)
Since LrC 7.3/ACR 10.3 (2018), Adobe "Creative Profiles"/"Looks" and **v2 profiles** are XMP files: a `crs:Look` or the root description with `crs:ProcessVersion`, `crs:RGBTable`/`crs:Table_<32-hex>`. Structure [K + V from Adobe forum https://community.adobe.com/feature-requests-564/p-document-the-format-of-rgb-luts-in-enhanced-profiles-666490]:
- A 3D RGB LUT (and/or HSV tables) is stored as a text element/attribute `crs:Table_<MD5 fingerprint>` whose content is **zlib-compressed binary encoded with an Ascii85 variant** (a minor variant "suitable for XML" - avoids `<`,`&` etc.). `crs:RGBTable="<fingerprint>"` links to the table. Similar tables: `crs:Table_<fp>` for `ProfileHueSatMapData`, `ProfileLookTableData` equivalents (`crs:HueSatMap`?, `crs:LookTable`).
- Adobe says: "no official documentation exists"; the data structures are in the **DNG SDK**: `dng_rgb_table` (class holding dims, divisions, samples, `Encoding`/gamut/gamma, "Amount") and `dng_big_table` (base class providing `Fingerprint` MD5, `Encode/ASCII85-ish + zlib` and `Decode`). Community partial RE work exists on GitHub (decompress works; value transform and fingerprint unclear per the thread). The DNG SDK's `dng_rgb_table` supports: RGBTable divisions (typically 32x32x32 or 17^3? [?]), "RGBTableDims", min/max range, gamut = ProPhoto primaries with linear (or sRGB gamma "Encoding") input; plus **Amount** (`RGBTableAmount`/`LookAmount`) and an optional `DNG_SDK`-"gamut extension" flag. Interpolation: tetrahedral or trilinear [?; SDK code is the authority - read `dng_render.cpp`/`dng_rgb_table.cpp`].
- Decoding plan (to do, using SDK source as the spec - license permits reading and using, see section 6): (1) base-N variant: Ascii85 alphabet shifted/modified to avoid XML special chars; (2) first 4 bytes = uncompressed length (LE) then zlib stream (Adobe `dng_big_table::EncodeBase85` pattern: `compress2` with length prefix) [K/?]; (3) parse header (version, dims, divisions as uint32 / float table of uint16 or float RGB triples). Validate by decoding Adobe's shipped Creative profiles (e.g. "Adobe Landscape" look) and checking the identity subtleties.
- Other tools: XMP-to-CUBE converters (https://github.com/chrisperkles/LightroomPresetsToLUTS - actually applies the sliders through its own Python emulation, does not decode Table_), colorsuite.app (JPG->XMP/LUT) and "lightroom-preset-decoder" (https://github.com/ly1lb/lightroom-preset-decoder - web tool extracting settings from metadata; no table decoding). I found **no public, complete decoder of Table_<md5>** beyond the partial work cited in the Adobe thread -> **uncertainty**.

### 3.3 Commercial presets and profiles
Many paid packs now ship `.xmp` **profiles** (colour grading "LUT profiles") created with Adobe's Profile Creator/DNG Profile Editor or by third-party tools exporting `.cube -> .xmp` (e.g. via Lightroom Profile Creator plugin): they carry an RGBTable in the XMP. Presets + profiles typically installed together; the preset references profile via `crs:Look` or `crs:CameraProfile` + digest. A preset without the embedded table, referencing profile by name, only works if that profile is installed.
Another common alt: presets include `crs:ToneCurvePV2012`, HSL, ColorGrade, curves per channel to fake film LUT looks.

### 3.4 Which profiles are copyrighted / what can ship
- Adobe Standard, Adobe Color, Adobe Monochrome, Adobe Landscape/Neutral/Portrait/Vivid, Camera Matching profiles, Adobe-supplied Look profiles, and the Adobe lens-profile database (.lcp): **copyrighted by Adobe**; redistribution only per their EULA / `ProfileEmbedPolicy` (Adobe Standard DCPs shipped inside Lightroom/DNG Converter - the DNG Converter EULA allows using them only with Adobe software; some community discussions assume DCP files "allow copying" (embed policy 0) are redistributable but the *copyright field* stands) [?]. **Do not bundle** them.
- Camera-maker profiles (Nikon/Canon "Camera Standard/Vivid") in presets: they refer by name only (`CameraProfile="Camera Vivid"`) - these are Adobe-built emulations (copyrighted) not the camera makers' picture styles.
- Can ship: our own profiles; DCPs generated by us (dcamprof from public target measurements; RawTherapee's `rtdata/dcpprofiles` - check licenses before importing), matrices derived from **public domain**/own measurements, from libraw/darktable `adobe_coeff` tables (GPL/LGPL - match our license), `darktable` basecurve/film emulation. An embedded-in-preset profile (RGBTable in a purchased preset) is the preset author's work and we may *apply it to the user's own image on import* (interoperability) but should not re-share it standalone.
- How to approximate **"Adobe Color"** (the default profile for raw since 2018) / **"Adobe Standard"**: Both = per-camera DCP: camera matrices + ForwardMatrix + HueSatMap + LookTable + a default tone curve (Adobe Color adds a stronger contrast curve and a LookTable giving a "punchier/more saturated" look than Adobe Standard; Standard is a flatter "neutral"). Without Adobe's DCP: (a) use the camera's matrices (libraw/DNG `ColorMatrix` embedded in the DNG, or our own DB) plus an **approximated default tone curve** (the Adobe "default render" curve: slight S-curve with toe/shoulder; DNG SDK's `dng_tone_curve`/`dng_render` default: the DNG SDK renders with `dng_function_GammaEncode_sRGB`+`dng_tone_curve_acr3_default` (ACR3 default curve - source available in `dng_render.cpp` [K]) ); (b) if the user owns Lightroom/DNG Converter, **detect and read their installed DCPs at runtime** (paths: Windows `C:\ProgramData\Adobe\CameraRaw\CameraProfiles\Adobe Standard\*.dcp`; macOS `/Library/Application Support/Adobe/CameraRaw/CameraProfiles/`) as a user-initiated feature - no redistribution, legally cleaner; (c) calibrate our own default "Adobe Color-like" profile by optimization against reference renders of ColorChecker shots.
- Reference-hint: darktable maps LR imports to "cmatrix" (colorin) as "closer to what Lightroom does" (lightroom.c ~L1313).

---------------------------------------------------------------------------
## 4. Process version dependence and the fidelity problem

- `crs:ProcessVersion` selects **entirely different algorithms**: 2003 (5.7; Exposure/Recovery/FillLight/Brightness/Contrast), 2010 (6.7; adds PV2010 Recovery/FillLight/Clarity/Vibrance/curve), **2012 (11.0; Exposure2012/Highlights2012/Shadows2012/Whites2012/Blacks2012, Contrast2012, Clarity2012; 2012-named tags)**, and newer (13.x/15.x: "Version 5 (2023)" HDR/ACR 15+?) [?]. Tag *names* tell which: `...2012` present => PV2012. Real-world corpora: >95% of current commercial presets are PV2012 (`ProcessVersion="11.0"`), many old ones PV2010 (`6.7`).
- Core problem: values are parameters of closed algorithms: Highlights2012/Shadows2012/Whites/Blacks are local-contrast-aware tone mapping (based on a guided/ bilateral base-detail decomposition with an "auto-adaptive" stage), Clarity is a large-radius local contrast, Texture medium frequency, Dehaze is a dark-channel-prior-like model (Adobe patent family), Vibrance is a saturation-protected function, HSL operates in a proprietary space, Color Grading uses its own blend, Point Color unknown. No spec, no source. Exact pixel match is **impossible** without reverse engineering and even then it's moving (algorithm tuned across versions; Adobe also changes defaults like AI denoise).
- What *is* known: ACR image pipeline is documented partially in DNG SDK (how DNG reference renders - baseline exposure, tone curve ACR3, camera-to-ProPhoto, DCP application, opcodes in `dng_opcodes.cpp` (WarpRectilinear, FixVignetteRadial, GainMap), `dng_render.cpp`, `dng_color_space`, `dng_ifd`, `dng_negative`); plus Adobe papers/patents (Dehaze/Clarity/"highlight recovery" - e.g. US patents by Adobe on "local Laplacian filters" (Paris et al.), "dehazing" (He et al.; Adobe-affiliated authors), and Sylvain Paris' publications). The DNG SDK does **not** contain Camera Raw's tone/HSL/clarity ops, noise reduction, sharpening or masking.
- Strategies for good-enough fidelity:
  1. Map each tag to analogous open algorithms in our pipeline: exposure (linear gain in ProPhoto/scene linear), contrast (S-curve around mid-grey with pivot ~ 0.18->?), highlights/shadows (tone-equaliser/local-Laplacian), whites/blacks (white/black point + toe/shoulder), clarity/texture (unsharp on large/medium radius on luma), dehaze (dark-channel based), vibrance/saturation (HSL/Lch with skin protection), HSL (8-band hue-dependent weighting in HSV/Lch with Adobe-like band centers: R 0, O 30, Y 60, G 120, A 180, B 240, P 270, M 300 deg approx - needs calibration), tone curve (exact: point curves are the only almost-exactly reproducible bit), parametric curve (implementable: region sliders with split points - Adobe uses spline blend [?]), split toning/colour grading (blend maths unknown but straightforward), grain (own), vignette (post-crop vignette formulas partially reproducible by matching style 1/2/3), sharpen (USM with masking), NR (own).
  2. **Empirical calibration**: build per-slider response curves by rendering test charts (ColorChecker, ramps, natural images) through Lightroom (the developer's licensed copy; export JPEG/TIFF with only one slider changed in steps of 25), then fit parameters (nonlinear least-squares / Nelder-Mead / small NN) of our own algorithm so that our output matches in CIEDE2000 / luma-ramp RMSE. Darktable's lightroom.c is exactly this on a tiny scale: piecewise-linear lookup tables (`lr2dt_*`) for blacks, clarity, grain, vignette (see 5.1). Provide tables as data files (JSON) so they're refit without code changes.
  3. Auto-fit presets: the fallback "bake" path - treat preset as black box: render a 3D LUT (and tone curve) **per preset** from reference renders (user-supplied), i.e. preset -> .cube. Only valid for non-spatial colour/tone ops (not clarity, grain, vignette, etc.).
  4. Use DNG SDK pieces where licence-compatible for the **base render** (matrices, DCP interpolation, default curve, opcodes): saves months of ambiguity (see section 6 for licence).
  5. Report fidelity: tag import report listing exact / approximated / ignored for each tag (UI shows "approximate").
- Dependence on **input**: Lightroom works on raw demosaiced camera data in ProPhoto-linear w/ baseline exposure & DCP; preset applied to JPEG behaves differently (different WB scale, no profile). Preserve a per-photo "scene-referred vs output-referred" flag.
- Relative vs absolute: WB, Auto-tone, Upright, AI masks, Auto-Settings depend on image content.

---------------------------------------------------------------------------
## 5. Existing importers (surveyed locally and online)

### 5.1 darktable (local: scratchpad/darktable/src/develop/lightroom.c, 1631 lines; lightroom.h)
Entry: `dt_lightroom_import(imgid, dev, iauto)` (L1125) reads `<name>.xmp`, accepts only `x:xmpmeta` root (L1167), registers namespaces crs, dc, tiff, xmp, exif, lr, rdf (L1260-1295) and runs XPath `//crs:*` (elements) and `//@crs:*` (attributes) (L1300). `.lrtemplate` and develop-preset `.xmp` are **not** a separate path: it imports *sidecar* XMPs only (also used by `darktable-cli`/"import lr edits" when importing images with a Lr sidecar).
Mapped (from `xmlStrcmp(name, "...")` list): CropTop/Right/Left/Bottom/Angle/HasCrop/ImageWidth/ImageLength/Orientation -> `clipping`+`flip`; **Exposure2012**, **Blacks2012** -> `exposure` (blacks via `lr2dt_blacks` table {-100->+0.020, 0->0, 100->-0.010}); PostCropVignette{Amount,Midpoint,Style,Feather,Roundness} -> `vignette` (tables lr2dt_vignette_gain/midpoint); Grain{Amount,Frequency} -> `grain` (tables); Parametric* + ToneCurveName2012 + ToneCurvePV2012 -> `tonecurve` (spline from the Seq; named curves handled); HueAdjustment*/SaturationAdjustment*/LuminanceAdjustment* (8 colours) -> `colorzones` (value/200+0.5); SplitToning{Shadow,Highlight}{Hue,Saturation}+Balance -> `splittoning`; **Clarity2012** -> `bilat` (local contrast; table +-100 -> +-0.65); RetouchInfo -> `spots`; metadata: Rating, Label (colour), subject/hierarchicalSubject (tags), title/description/creator/rights, GPS*. Also sets colorin to `cmatrix` for raws.
**Not mapped**: WhiteBalance/Temperature/Tint, Contrast2012, Highlights2012, Shadows2012, Whites2012, Texture, Dehaze, Vibrance, Saturation, ColorGrade*, Sharpness*, NR, lens profile, Upright/perspective, masks (GradientBased/Circular/Paint/MaskGroup), Look/profile/LUTs, curves per channel (Red/Green/Blue), GrayMixer/ConvertToGrayscale, calibration primaries, point colours. No amount scaling, no preset/lrtemplate handling. Hardcoded legacy-version module params (`dt_iop_*_params_v1_t`).
Takeaway: darktable's approach = direct 1:1 or LUT-based mapping of a small subset to native modules, no fidelity claims. Table style `lr2dt_t{lr,dt}` + piecewise-linear interpolation is reusable as our "calibration table" pattern.
Other darktable pieces: `src/imageio/imageio_dng.h` (DNG writing), Lua scripts community: "lr2dt"? Not found.[?]

### 5.2 RawTherapee (local clone scratchpad/RawTherapee)
**No Lightroom/XMP-crs importer in the repository** (grep for `crs:`/`lrtemplate`/`Lightroom` finds only `rtgui/windows/iccprofilecreator.cc` text mention and libraw changelog). RawTherapee does implement **DCP reading/applying** (rtengine/dcp.cc: ForwardMatrix, HueSatMap with interpolation by illuminant, LookTable, ToneCurve, "baseline exposure", `DCPProfile::apply`) and reads LCP lens profiles (rtengine/lcp.cc) and has "Film Simulation" (HaldCLUT) tool. Users convert presets by hand/with Lightroom-to-PP3 forum scripts (not maintained) [?]. Takeaway: RT's DCP code is the best open-source implementation to study/license-compat (GPLv3).

### 5.3 Others
- LrGeniusAI XMP preset writer https://github.com/LrGenius/LrGeniusAI/pull/390 - writes `crs:MaskGroupBasedCorrections` (reference for mask schema).
- XMP-to-CUBE converters: https://github.com/chrisperkles/LightroomPresetsToLUTS (Python/numpy/OpenCV, emulates Exposure/Contrast/Highlights/Shadows etc. into a .cube; blend with reference LUT) - same black-box-bake concept.
- colorsuite.app, https://github.com/ly1lb/lightroom-preset-decoder: extract settings from an image to preset; not rendering.
- Adobe's own: Lightroom -> Photoshop ACR shares the engine (presets cross-compatible). Adobe Lightroom mobile imports both .xmp and .lrtemplate and DNG-embedded presets (DNG with embedded `crs:` settings = "preset DNG" trick used by Lr mobile pre-xmp: a tiny DNG whose XMP carries the preset).
- Capture One: imports Lightroom *.lrtemplate/xmp?* No - Capture One supports its own styles (.costyle); third-party converters sell Lr->C1 style conversion; Capture One 23+ can read **LUT profiles (.cube)**. [?]
- ON1 Photo RAW: imports Lightroom presets (.lrtemplate/.xmp) and maps to ON1 filters ("Import Lightroom Presets" in ON1 Presets panel; approximate, mapping Basic, Tone Curve, HSL, Split Toning...). [K, ON1 docs; not fetched]
- Luminar Neo / Skylum: imports Lightroom *presets* as "Lightroom presets (.xmp/.lrtemplate)" via Presets > Add Presets from Lightroom [K]; approximate.
- Pixelmator Pro/Affinity Photo: no Lr preset import (Affinity imports LUT/Lightroom presets? Affinity Photo 2 supports .lut; Pixelmator Pro has "Import Presets" for its own) [?].
- DxO PhotoLab: no Lr preset import (has own presets, imports "Lightroom Settings" sidecar? DxO can read Lr XMP develop settings partially: "DxO PhotoLab: Import of Lightroom edits"? [?]).
- Darkroom (iOS), Polarr: import `.lrtemplate/.xmp` (Polarr "Lightroom preset import" in web) [?].
- ImageMagick: no, but Hald-CLUT/`-clut`. 
- `dcamprof`, `lcms2` for LUT/ICC route.
Overall approaches: (1) per-tag mapping into native modules (darktable/ON1/Luminar), (2) bake to LUT (converters), (3) host real ACR (Adobe only).

---------------------------------------------------------------------------
## 6. Legal (not legal advice; get counsel before release)

- **File-format interoperability**: Facts and functional file formats/parameter names are not copyrightable subject matter in many jurisdictions (US: Baker v. Selden, Oracle v. Google fair-use ruling on API; Lotus v. Borland - menu structure) though not uniformly. Reading `.xmp`/`.lrtemplate` produced by the user's own purchases for interoperability is generally defensible. EU Directive 2009/24/EC Art. 6 (decompilation for interoperability, strictly limited; observing/testing under Art. 5(3) and no protection for ideas/interfaces under Art. 1(2) - "ideas and principles underlying any element including interfaces"); SAP v WPL (CJEU C-406/10) says functionality and file formats are not protected by copyright. Black-box observation of Lightroom's rendered output to calibrate (our own implementation) is clean-room style; **do not decompile or disassemble Camera Raw / Lightroom binaries**; Adobe EULA forbids reverse engineering (contractual; EU Art. 6 override only for interoperability with independently-created programs).
- **XMP**: Adobe XMP specification is open (ISO 16684-1) and **Adobe XMP SDK is BSD-3-Clause** (also vendored in the DNG SDK per hfiguiere repo notes). We can use libexempi/exiv2/xmp-toolkit or any RDF/XML parser. Camera Raw `crs` namespace itself is only partially documented by Adobe (developer.adobe.com), the rest is defacto.
- **DNG**: Spec is public with Adobe's **patent licence grant** for Compliant Implementations reading/writing DNG (https://scancode-licensedb.aboutcode.org/adobe-dng-spec-patent.html). **DNG SDK licence** (https://www.adobe.com/support/downloads/dng/dng_sdk_eula_win.html): "non-exclusive, worldwide, royalty free license to use, reproduce, prepare derivative works from, publicly display, publicly perform, distribute and sublicense the Software for any purpose" with retention of notices, plus the patent grant for Essential Claims [V via search summary]; it is custom, **not OSI-approved** ("Please don't consider this code to be under an OSI license" - hfiguiere/dng_sdk note), which complicates inclusion in GPL/OSI projects - check GPL compatibility (permissive, so likely OK but flagged by FSF-style review); safer to *re-implement from the spec/SDK algorithms* or consume as a separate optional library. SDK includes Boost (BSL), libjpeg-derived code, XMP SDK (BSD). Latest 1.7.x (per search summary Sept 2026, unverified).
- **Trademark**: "Adobe", "Lightroom", "Camera Raw", "Photoshop" are Adobe trademarks. Use nominatively and factually only ("imports Adobe Lightroom Classic `.xmp` presets") without implying endorsement; never name the product/ modules with them; do not use Adobe logos/icons. Say "compatible with" sparingly.
- **Preset licences**: commercial presets are copyrighted works (the *selection of values* is thin, but sellers license: single-user, no redistribution, no conversion/resale). Users importing their purchased presets for personal use into another editor is generally within their licence unless explicitly forbidden (some EULAs forbid conversion). Never bundle commercial presets in repo/tests/CI; for tests use free presets with permissive licences (CC0 / MIT), synthetic ones we author, or Adobe's own sample presets only if licence allows (it does not for redistribution).
- Embedded Adobe lens profiles/camera profiles: do not redistribute (see 3.4); Lensfun is CC-BY-SA for DB.
- Patents: Adobe holds patents on Dehaze, local-contrast etc.; implementing *analogous* algorithms (standard dark-channel dehaze, local laplacian) carries unknown risk; many are expired/narrow; get counsel.

---------------------------------------------------------------------------
## 7. Recommendations for our importer

Architecture
1. **Parsers**: (a) XMP (XML/RDF: use a real XML parser with namespace-URI awareness; handle attribute + element + `rdf:parseType="Resource"` + `rdf:Seq/Bag/Alt` forms; tolerant of xpacket wrappers; embedded in DNG/JPEG by reading the XMP packet), (b) `.lrtemplate` (Lua-table subset parser), (c) optional: DCP (TIFF-IIRC) reader, XMP-profile reader with `Table_` decoder, `.cube`/`.3dl` LUT reader for non-Adobe profiles.
2. **Canonical model**: `Preset { header{uuid,name(lang map),group,type,supports{amount,color,mono,hdr,ndr,scene,output},cameraRestriction,version,processVersion}, settings: Map<TagName,Value> (sparse, preserve order & raw text), look: Option<Look{name,amount,uuid,params,rgbTable}>, masks: Vec<Correction{...}> , unknown: Vec<(name, raw xml)> }`. Mirror `crs:` names 1:1 (so tags are greppable and round-trippable) and keep **sparseness** (present vs. default is information). Typed value variants: Int, Real, Bool, String, Seq<String>, Struct, LangAlt.
3. **Versioning**: record `ProcessVersion`; dispatch to a per-PV mapping table (`pv2012`, `pv2010`, `pv2003`) that converts legacy tags (Exposure/Brightness/Contrast/Shadows/FillLight/Recovery/Clarity) to the nearest 2012 model with documented approximations; carry `Version`/`CompatibleVersion` only for diagnostics. New tags: unknown never errors.
4. **Unknown tags**: keep, report, never fail. Classification per tag in a registry (JSON): `status = exact|approx|ignored|image-specific|unsupported`, `group` (for UI toggles), `default`, `range`, `scale` (e.g., local values x100). Show an "import report" panel (and log). `Enable*` flags gate their groups. For attributes whose names match `Local*` etc. in unknown struct, preserve.
5. **Apply-as-delta semantics**: applying a preset sets only present keys; Amount slider = lerp from current to preset (numeric), curves toward identity, LUT toward identity; boolean/enum toggles at amount>0. Honour `SupportsMonochrome`/`CameraModelRestriction` as warnings (not hard fails).
6. **Profile handling**: `CameraProfile` name -> our profile registry: "Adobe Standard/Color/..." -> our built-in neutral+curve approximation (and user-installed Adobe DCP detection); Look with RGBTable -> decode and apply as 3D LUT in ProPhoto-linear before tone ops (same position as DCP LookTable); otherwise named creative looks -> approximated by nearest curve+HSL or ignored with warning.
7. **Masks**: support linear/radial/brush/luminance range natively; AI mask (subject/sky/background/people) -> our own segmenter with graceful degrade (apply to whole image? No: skip + warn, or apply with feathered gradient for sky heuristic). Don't apply RetouchAreas from presets.
8. **Pipeline fidelity**: implement each slider as our own module with a calibration table (JSON) mapping Lr units -> module params; build reference-render tooling (scripted Lightroom/ACR exports of single-slider sweeps on ColorChecker + ramp + ~20 photos) and fit; track error metrics (CIEDE2000 mean/95th, luma RMSE) in CI against stored reference renders (store reference renders ONLY from our own/CC0 photos).
9. **Unit tests/corpus**: (a) synthetic presets, one tag each (generated); (b) free presets: from Adobe's? (no), community free packs with permissive/CC0 terms (check each), preset sellers' free samples (license often non-redistributable -> keep in a private, git-ignored fixtures dir and run locally), GitHub repos (e.g. clebert/lightroom-preset example.xmp), RawPedia, "free lightroom presets" CC0 on Unsplash/Pixabay-like sites; (c) fuzz for parsers (quoted names, BOM, CRLF, huge Seq); (d) tag-census script over any corpus -> prioritise by frequency (expect: Exposure2012, Contrast2012, Highlights/Shadows/Whites/Blacks, Vibrance/Saturation, HSL x24, ToneCurvePV2012(+RGB), ColorGrade*/SplitToning*, Texture/Clarity/Dehaze, Sharpness/NR, Grain, PostCropVignette, WB, CameraProfile/Look, LensProfileEnable, masks).
10. **Priorities**: P0 tone/exposure/curves/HSL/colour grading/vignette/grain/sharpen/Vibrance/Saturation/WB; P1 Texture/Clarity/Dehaze, camera profile+LUT decode, masks (gradients, brush, range); P2 AI masks, Point Color, lens profile, upright/transform, B&W mixer details, Lens Blur.

---------------------------------------------------------------------------
## Open questions / uncertainties
- Exact Table_<md5> binary layout and fingerprint algorithm (only SDK `dng_rgb_table`/`dng_big_table` source authoritative; not read this session).
- Exact preset Amount maths per tag type; behaviour above 100%.
- Value ranges/defaults for several tags (Point Color encoding, LensBlur, HDR tags, MaskSubType enum, MaskBlendMode enum, process-version numbers after 11.0) are from memory and flagged [?].
- Exact tone-curve interpolation and Adobe's Medium/Strong Contrast point sets.
- Legal points need counsel review, esp. DNG SDK licence vs GPL, and patents.
- Claims about ON1/Luminar/Capture One/Polarr/DxO import behaviour not verified here.
- Adobe's own page for the `crs` namespace documents only legacy tags (https://developer.adobe.com/xmp/docs/xmp-namespaces/crs/); exiftool/exiv2 lists are the best public names list; helpx supported-metadata page returned 403.

## Sources
- https://developer.adobe.com/xmp/docs/xmp-namespaces/crs/
- https://exiftool.org/TagNames/XMP_with_ids.html ; https://exiv2.org/tags-xmp-crs.html ; https://pkg.go.dev/github.com/unidoc/go-xmp/models/crs
- https://github.com/clebert/lightroom-preset/blob/main/example.xmp
- https://community.adobe.com/feature-requests-564/p-document-the-format-of-rgb-luts-in-enhanced-profiles-666490
- https://github.com/LrGenius/LrGeniusAI/pull/390
- https://community.adobe.com/t5/photoshop-ecosystem-discussions/ai-camera-raw-masks-not-re-computed-when-used-in-an-action/td-p/14176133
- https://www.adobe.com/support/downloads/dng/dng_sdk_eula_win.html ; https://www.adobe.com/support/downloads/dng/dng_sdk.html ; https://helpx.adobe.com/camera-raw/digital-negative.html ; https://scancode-licensedb.aboutcode.org/adobe-dng-spec-patent.html ; https://github.com/hfiguiere/dng_sdk
- https://github.com/chrisperkles/LightroomPresetsToLUTS ; https://github.com/ly1lb/lightroom-preset-decoder ; https://colorsuite.app/
- https://ask.metafilter.com/285181/Does-this-Adobe-Lightroom-preset-format-conform-to-any-standard ; https://digital-photography-school.com/lightroom-preset-compatibility-xmp-or-lrtemplate/
- Local: scratchpad/darktable/src/develop/lightroom.c (+lightroom.h); scratchpad/RawTherapee (no importer; rtengine/dcp.cc for DCP).
