# Adobe Lightroom Feature Inventory (reference for a free OSS alternative)

## 0. Provenance and confidence

- Adobe's helpx.adobe.com returned HTTP 403 to every fetch from this environment (WebFetch and curl). Adobe help URLs below were surfaced by search results, whose snippets quote the pages, but the full pages were **not read**.
- Facts were taken from: search-result snippets of Adobe pages, secondary pages that were fetched (Wikipedia, Lightroom Killer Tips, Julieanne Kost, DPReview), and the author's prior knowledge.
- Tags: **[A]** = Adobe doc snippet seen; **[S]** = reputable secondary source fetched; **[K]** = background knowledge, not re-verified this session. Treat all numeric ranges tagged [K] as "verify against a real Lightroom install".
- Version note: Wikipedia fetch states that as of Aug 2026 Lightroom Classic is 15.5 and Lightroom (cloud/desktop) is 9.5 [S] https://en.wikipedia.org/wiki/Adobe_Lightroom. A search result mentions 15.6 (Sept 2026) adding cleaner reflection removal and Check All/None for landscape/people masks [A] https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/release-notes.html.

Key Adobe doc entry points (unread, 403): 
- https://helpx.adobe.com/lightroom-classic/user-guide.html
- https://helpx.adobe.com/lightroom-classic/help/image-tone-color.html
- https://helpx.adobe.com/lightroom-classic/help/masking.html
- https://helpx.adobe.com/lightroom-classic/help/color-mixer.html
- https://helpx.adobe.com/lightroom-classic/help/enhance-details.html
- https://helpx.adobe.com/lightroom-classic/help/remove-tool.html
- https://helpx.adobe.com/lightroom-classic/help/hdr-output.html
- https://helpx.adobe.com/lightroom-classic/help/supported-file-formats.html
- https://helpx.adobe.com/camera-raw/using/hdr-output.html

---

## 1. Product overview

### 1.1 The three surfaces
| Product | Storage model | Notes |
|---|---|---|
| **Lightroom Classic** | Local catalog (SQLite `.lrcat`) referencing files in place on disk; previews in `*Previews.lrdata`; optional sync of selected collections to cloud | Full feature set: virtual copies, Book, Slideshow, Print, Web modules, Lua plugin SDK, tethering. Written partly in C++, ~40% Lua [S] https://en.wikipedia.org/wiki/Adobe_Lightroom |
| **Lightroom (cloud, "CC", desktop/mobile/web)** | Originals + edits live in Adobe cloud (plan storage), local cache; edits are parametric | Introduced Oct 2017 [S wiki]. Simplified UI, albums, no plugin SDK, no Book/Print modules; gets AI features first. |
| **Adobe Camera Raw (ACR)** | Plug-in for Photoshop/Bridge; settings in XMP sidecar or embedded | Same raw engine and the same Develop parameter set as Lightroom (shared "Adobe Camera Raw" develop settings, crs: XMP namespace) [K]. |

Both use non-destructive editing: edits are stored as parametric instructions separate from the image [S wiki]. History began as PixelToy/Shadowland (2002); 1.0 shipped 19 Feb 2007 [S wiki].

### 1.2 Process versions (PV)
A Process Version fixes the rendering algorithms for the tone/detail sliders; older photos retain their PV until the user updates (UI shows a lightning-bolt badge "Process version" warning; Settings > Process: Version dropdown) [K].

| PV | Introduced | Key characteristics |
|---|---|---|
| **PV2003** | LR 1.0 | Original: Exposure, Recovery, Fill Light, Blacks, Brightness, Contrast; Tone Curve parametric; Clarity (original), Vibrance, Saturation; sharpening/NR originals. |
| **PV2010** | LR 3 | New sharpening and noise reduction (luminance/color NR with detail), post-crop vignetting and grain added. Same tone sliders as 2003. |
| **PV2012** | LR 4 / ACR 7 (2012) | Rewrote tone controls: Exposure, Contrast, **Highlights, Shadows, Whites, Blacks** replace Recovery/Fill/Brightness; "evolved" old adjustments [A snippet https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html]. New tone-mapping for high-contrast images; local adjustments gain same sliders. Blacks in 2003/2010 "specifies which image values map to black" [A]. Clarity/Texture/Dehaze all built on PV2012. |
| **PV Version 4 / 5 / 6** | LR CC era | Naming in dropdown is "Version 4/5/6" (Adobe Color profile era); minor processing changes for new profile pipeline, HDR/AI features; PV2012-based sliders unchanged [K — unverified details]. |
| Current default | | New imports default to latest (Version 6 as of recent builds) [K]. |

OSS implication: implement PV2012-style slider semantics only; ignore PV2003/2010.

---

## 2. Library / catalog

### 2.1 Catalog
- **Catalog file**: SQLite database `Name.lrcat` (plus `.lrcat-journal`/-wal, `Previews.lrdata`, `Smart Previews.lrdata`, backups zip). Stores file paths (not images), metadata, develop settings, collections, keywords, history, virtual copies, face data [K].
- One catalog open at a time; folder panel mirrors real disk folders; photos referenced "in place".
- Catalog backup prompts on exit; optimization and integrity test available [K].

### 2.2 Import
- Sources: card, folder, camera. Methods: **Copy as DNG**, **Copy**, **Move**, **Add** (in place) [K].
- Options: file handling (build previews: Minimal/Embedded & Sidecar/Standard/1:1; build Smart Previews; don't import suspected duplicates), apply develop preset, metadata preset (copyright etc.), keywords, destination folder organization (by date format, into one folder, etc.), rename template (tokens: date, sequence, custom text, original filename), backup copy to second location.
- Auto Import from a watched folder; tethered capture (section 6).
- Duplicate detection: "Don't import suspected duplicates" by capture time + filename; newer 15.4 adds "auto-detect Duplicates" [A snippet https://community.adobe.com/announcements-673/lightroom-classic-v15-4-is-live-cull-people-shots-faster-auto-detect-duplicates-sync-keywords-everywhere-1627070].
- DNG conversion: lossless, optional lossy, embedded fast-load data, optionally embed original raw, compatibility level (Camera Raw version), linear (demosaiced) DNG option.

### 2.3 Organisation
- **Folders**: mirror filesystem; can move/rename in-app (moves on disk).
- **Collections**: static virtual groups; **Collection Sets**; **Quick Collection** (target collection `B`); **Smart Collections**: saved rule sets (any/all/none) on any metadata field - rating, flag, label, keyword, camera, lens, ISO, date, file type, develop state, location, text search, "has smart preview", etc. [K]
- **Keywords**: hierarchical keyword list, synonyms, "include on export", "export containing keywords", parent keywords; keyword sets; keyword tagging via painter tool; auto keyword suggestions. 15.4 adds keyword sync across ecosystem [A snippet above].
- **Metadata**: EXIF (read-only camera data), IPTC Core/Extension (title, caption, creator, copyright, location fields, etc.), custom metadata panels, metadata presets; write to XMP embedded for DNG/JPEG/TIFF/PSD, sidecar `.xmp` for proprietary raw.
- **Rating / flag / colour label**: 0-5 stars (keys 0-5), flags Pick/Reject/Unflagged (P, X, U), colour labels red/yellow/green/blue/purple (keys 6-9), label sets customisable [K].
- **Filtering**: Library Filter Bar - Text (search any field, contains/starts with…), Attribute (flag, rating >=/=/<=, label, kind (Master/Virtual/Video), Metadata (up to many columns of camera/lens/date/keyword/location)). Sorting by capture time, edit time, filename, rating, etc. Views: Grid (loupe, compare `C`, survey `N`).
- **Stacking**: group photos; collapse to top photo; auto-stack by capture time; stacks follow in folders/collections [K].
- **Virtual copies**: additional develop-settings set for same file; no extra file; separate metadata copy-name; badge on thumbnail [K].
- **Faces/People**: face detection runs in background on whole catalog, People view with named people stacks, confirm/reject suggestions, Faces panel for culling by person introduced in 15.4 [A snippet community 15.4 link]. Face data stored in catalog and written to XMP keywords optionally [K].
- **Map module**: shows geotagged photos on a Google Maps/ Mapbox-like map (Lightroom Classic uses its own map provider), GPS tracklog (GPX) auto-tagging, saved locations with radius (private), reverse geocoding into IPTC location fields [K].
- **History & Snapshots**: Develop history is a per-photo ordered list of steps (persisted in the catalog until cleared); Snapshots are named saved states; both stored in catalog. Copy/paste settings (select which), Sync settings, Match Total Exposures, Auto Sync [K].
- **Previews**: Minimal, Embedded & Sidecar, Standard (screen-size), 1:1 (full-res, in cache), **Smart Previews** (lossy DNG, ~2560 px long edge, ~1/20 original size, allow editing while originals offline and enable mobile sync) [K]. Cache is `Previews.lrdata` (SQLite+JPEG pyramid `.lrprev`) [K].
- **Sync**: Classic syncs selected collections to Lightroom cloud as smart previews; edits round-trip (both directions) with limits (Lightroom cloud missing some Classic features) [K].
- **XMP sidecars**: edit settings written under `crs:` namespace (e.g. `crs:Exposure2012`, `crs:Contrast2012`, `crs:Highlights2012`, `crs:Clarity2012`, `crs:Texture`, `crs:Dehaze`, etc.). Auto-write setting is optional; "Read metadata from file" to refresh; conflict flags when external edits differ [K]. This is the interoperability target for an OSS clone.
- **Other**: Painter tool, Quick Develop panel (library presets + incremental tone buttons), Compare/Survey, Library "Sync Metadata", Publish Services, Auto-stack panorama/HDR output.

---

## 3. Develop module

### 3.1 Layout and global tools
- Panels: Navigator, Presets, Snapshots, History, Collections | Histogram, Tool strip (Crop, Remove [spot/heal], Red Eye, Masking, ...), **Basic, Tone Curve, Color Mixer (HSL / Color / B&W), Color Grading, Detail, Lens Corrections, Transform, Effects, Calibration**; Profile browser is at the top of Basic [K].
- Before/After: side-by-side (`Y`), left/right split, top/bottom, `\` toggles Before. Copy Before settings to After etc.
- Reference View (`Shift+R`): fixed reference image beside the active photo for matching.
- Histogram: RGB histogram with R/G/B/lum overlays; clipping indicators (`J`): shadow clipping blue, highlight clipping red; click triangles toggle; dragging in histogram adjusts Exposure/Highlights/Whites/Blacks/Shadows by region [K].
- Soft Proofing (`S`): choose destination ICC profile, simulate paper & black ink, intent (perceptual/relative), out-of-gamut warnings (source/dest), proof copies [K].
- Auto: Basic "Auto" button (Adobe Sensei tone), Auto White Balance (`As Shot`, `Auto`, `Daylight`, ...). Masks, Lens Corrections, Upright also have Auto. 
- **Adaptive presets**: presets that recompute masks for each image (Subject, Sky, Background, People parts...). Masks inside presets are re-computed per image [S] https://jkost.com/blog/2024/09/making-selective-edits-using-the-masking-tools-in-lightroom-classic.html

### 3.2 Profile browser
- Profiles define the base "look" (camera tone curve + colour transforms: DCP-like with 3D LUT and tone curve), distinct from presets which set slider values.
- Groups: **Adobe Raw** (Adobe Color, Adobe Monochrome, Adobe Landscape, Adobe Neutral, Adobe Portrait, Adobe Vivid), **Camera Matching** (camera-specific e.g. Camera Standard/Vivid/Portrait/Landscape/Neutral), **Legacy** (Adobe Standard), **Creative** (Artistic, B&W, Modern, Vintage…) and user-imported `.dcp`/`.xmp`/LUT profiles [K].
- **Amount slider** (0-200 for creative profiles; default 100) scales profile effect. Adobe Color/Adobe Raw/Camera matching profiles have no amount (or a fixed). Favorites star, Grid/List/Large/Small view [K].
- Profile information stored in `crs:CameraProfile` + `crs:LookTable` for LUT-based looks [K].

### 3.3 White Balance
- WB selector: As Shot, Auto, Daylight, Cloudy, Shade, Tungsten, Fluorescent, Flash, Custom. Eyedropper picks neutral point (`W`), with magnified loupe and optional "Auto dismiss".
- **Temp**: for raws, Kelvin approx. 2000-50000 K (slider shows Kelvin; range displayed 2000-50000 for raw); for JPEG/TIFF a relative -100 to +100 scale. **Tint** -150 to +150 (green to magenta) [K].
- Mechanism: raw WB multipliers on the camera-RGB (before demosaic/matrix), derived by solving between two calibration illuminants of the camera profile (DNG ColorMatrix1/2 interpolation) [K].

### 3.4 Basic tone (PV2012)
| Slider | Range / default | Behaviour |
|---|---|---|
| Exposure | -5.00 to +5.00 EV, 0 | Global linear-light gain applied before the tone curve; ±1.00 ≈ one stop; also has highlight roll-off protection [K] |
| Contrast | -100 to +100, 0 | S-curve pivoting on mid-grey; affects midtones most |
| Highlights | -100 to +100, 0 | Adjusts bright areas; left darkens & recovers detail, right brightens while minimizing clipping [A snippet image-tone-color] |
| Shadows | -100 to +100, 0 | Left darkens shadows minimizing clipping; right brightens and recovers [A] |
| Whites | -100 to +100, 0 | Sets white clipping point; left reduces clipping, right increases clipping [A] |
| Blacks | -100 to +100, 0 | Sets black clipping point; left darkens, right lifts |
| Texture | -100 to +100, 0 | Enhances/smooths medium-scale detail (mid-frequency) without affecting fine noise or tone; added LR Classic 9.3 / 2019 [K] |
| Clarity | -100 to +100, 0 | Local contrast on midtones (large-radius unsharp-mask-like on luminance); positive sharpens, negative softens/glows [K] |
| Dehaze | -100 to +100, 0 | Removes/adds haze (atmospheric scattering model: estimates transmission via dark channel prior) [K] |
| Vibrance | -100 to +100, 0 | Selectively saturates low-saturation colours, protects skin-tone hues and already saturated colours |
| Saturation | -100 to +100, 0 | Uniform saturation of all colours |
- Alt/Option-drag on Whites/Blacks shows clipping preview (Threshold view). Shift-double-click on Whites/Blacks sets automatically to just-clip [K].
- Tone slider interactions: local, tonal-range-aware (PV2012 uses edge-aware tone-mapping, with halo-reduced local contrast—not a simple curve) [K]. Exact algorithm is proprietary; OSS clone must approximate (e.g., Laplacian-pyramid/guided-filter tone mapper).

### 3.5 Tone Curve
- **Parametric curve**: Region sliders Highlights, Lights, Darks, Shadows (-100..+100) with three **split points** (Shadow/Dark split, Dark/Light, Light/Highlight - default 25/50/75 %) draggable below the curve [K].
- **Point curve**: Linear/Medium Contrast/Strong Contrast (PV2003 style) presets, custom points; **Targeted adjustment tool** (drag on image); Edit Point Curve per channel: RGB (luminance-ish), Red, Green, Blue; point values input/output 0-255 [K].
- In PV2012 the curve is applied after the Basic tone controls; RGB curve operates in the working space, and R/G/B channel curves enable colour grading/cross-processing [K].

### 3.6 Color Mixer (HSL) / B&W
- **Color Mixer** with modes HSL (Hue, Saturation, Luminance) and Color (individual colour tab with the three sliders for each band) [A] https://helpx.adobe.com/lightroom-classic/help/color-mixer.html.
- Eight fixed bands: Red, Orange, Yellow, Green, Aqua, Blue, Purple, Magenta; each Hue/Sat/Lum -100..+100, 0. Bands fixed in hue (ranges overlapping with smooth falloff). Targeted Adjustment Tool drags on image [K].
- **B&W** mode: "Black & White Mix" converts using 8 colour weights (-100..+100), Auto button. Applies Adobe Monochrome style profile or conversion; Treatment dropdown [K].

### 3.7 Point Color
- Eyedropper samples a colour on the image producing a swatch (the "pin"); the panel shows a colour field (hue horizontal, sat vertical) with original/adjusted colour and a luminance bar [S] https://lightroomkillertips.com/get-to-know-point-color-in-lightroom-classic/.
- Controls: **Hue Shift, Saturation Shift, Luminance Shift** (-100..+100 for each [K]); **Range** slider widening/narrowing the set of similar colours selected; expanded **Hue Range, Saturation Range, Luminance Range** sliders for fine control; "no direct one-to-one relationship between Range and the three range sliders" [A community snippet]. **Visualize Range** greys out unaffected colours [S].
- Unlike HSL: user-defined target and falloff; works globally and also inside the Masking panel (local) [S]. Max swatches: up to 8 per image [K — not verified; article did not state].

### 3.8 Color Grading
- Replaces Split Toning (Classic 10, Oct 2020). Wheels: **Shadows, Midtones, Highlights**, **Global** (tints whole image regardless of tone) with Hue (0-360°), Saturation (0-100) and per-wheel **Luminance** (-100..+100) [A snippet phlearn/search].
- **Blending** (0-100, default 50): overlap/softness between shadow/mid/highlight regions. **Balance** (-100..+100, default 0): shifts the pivot toward shadows or highlights [A snippet].
- 3-way view and individual wheel view; wheels are applied in the working colour space. Split Toning legacy settings are mapped into Color Grading when converting old photos [K].

### 3.9 Detail
- **Sharpening**: Amount 0-150 (default 40 for raw in PV2012? Defaults: 40 on Adobe presets; a secondary source says default applied to raw is 25 [S snippet, older]); Radius 0.5-3.0 px (default 1.0); Detail 0-100 (default 25 [K]); Masking 0-100 (default 0; Alt-drag shows mask - white = sharpened). Detail low = halo-reducing deconvolution-like, high = unsharp-mask-like [K, S snippet "Detail slider gives you control over different-sized details"].
- **Noise Reduction (manual)**: Luminance 0-100 (default 0), Detail 0-100 (default 50), Contrast 0-100 (default 0); Color 0-100 (default 25), Detail 0-100 (default 50), Smoothness 0-100 (default 50) [S snippet confirms Color default 25] [K for the rest].
- **Denoise (AI)** — in Enhance dialog: strength slider 0-100 (default 50), preview with 100% zoom; output new DNG or (newer) stored as enhance data in the catalog/XMP of original raw [S, A] https://helpx.adobe.com/lightroom-classic/help/enhance-details.html ; Denoise became available in Classic 12.3, ACR 15.3, LR 6.3 [S] https://www.dpreview.com/articles/9805565494/adobe-denoise-brings-photoshop-lightroom-nr-into-the-modern-era/. Speed: heavy on CPU/GPU (a 10-image batch took over 2 hours on a laptop with weak GPU) [S dpreview]. Cannot combine Denoise with Raw Details or Super Resolution [S, A].
- **Raw Details**: improves demosaic detail/colour/edge rendering/fewer artifacts; supports Bayer and Fuji X-Trans raws [A snippet]. 
- **Super Resolution**: ML upscaling producing 2x linear (4x pixel count); includes Raw Details; also accepts JPEG/TIFF [A snippet].
- All Enhance operations write a derived DNG ("-Enhanced-NR", "-Enhanced") or apply as per-raw enhance data. Requires GPU acceleration for decent speed [K].

### 3.10 Lens Corrections
- **Profile** tab: Enable Profile Corrections, Make/Model/Profile (Adobe lens profile database `.lcp`: distortion, vignette, TCA model with focal-length/focus/aperture interpolation), Amount: Distortion 0-200 (default 100), Vignetting 0-200 (default 100). Auto-matching via EXIF [K].
- **Manual** tab: Distortion -100..+100; Defringe Amount 0-20 and Purple/Green Hue ranges (two sliders each: Purple Hue 0-100 pair min/max, Green Hue likewise); Lens Vignetting Amount -100..+100 and Midpoint 0-100 (default 50); Remove Chromatic Aberration checkbox (handles lateral CA automatically); Defringe: Off/Highlight Edges/All Edges (legacy) [K].
- Also "Remove Chromatic Aberration" in Profile tab [K].

### 3.11 Transform / Geometry
- **Upright**: Off, Auto (balanced), Level, Vertical, Full, **Guided** (draw 2-4 lines); constraint "Constrain Crop"; Upright analysis requires lens profile for best results. Show Grid with Offset, Show Guides [K].
- **Manual** sliders: Vertical -100..+100, Horizontal -100..+100, Rotate -10..+10°, Aspect -100..+100, Scale 50-150 %, X Offset / Y Offset -100..+100; Lock/Constrain crop [K].
- **Crop & Straighten** (`R`): aspect presets (Original, 1:1, 4:5, 5:7, 2:3, 16:9, Custom), Angle slider -45..+45°, Straighten tool (draw horizon), overlays (Thirds, Grid, Diagonal, Triangle, Golden Ratio, Golden Spiral, Aspect Ratios), Lock aspect. 15.5 adds viewing cropped-out area via photo opacity [A search snippet].

### 3.12 Effects
- **Post-Crop Vignetting**: Style Highlight Priority / Color Priority / Paint Overlay; Amount -100..+100, Midpoint 0-100 (default 50), Roundness -100..+100, Feather 0-100 (default 50), Highlights 0-100 (not available in Paint Overlay) [K].
- **Grain**: Amount 0-100 (default 0), Size 0-100 (default 25), Roughness 0-100 (default 50) [K].
- (Dehaze lives under Basic in recent versions; it used to be in Effects.)

### 3.13 Calibration
- Shadows Tint -100..+100 (green/magenta in shadows), Red/Green/Blue Primary Hue and Saturation -100..+100. Changes primaries of the camera matrix — tool for custom camera calibration [K].

### 3.14 Retouching
- **Remove tool** (`Q`): Heal, Clone, **Content-Aware Remove** (when "Use Generative AI" is unchecked, samples pixels from elsewhere in the image), **Generative Remove** (Adobe Firefly; processed in cloud; three variants to choose from; removes shadow/object) [A] https://helpx.adobe.com/lightroom-classic/help/remove-tool.html [S] https://petapixel.com/2024/05/21/huge-adobe-lightroom-update-adds-ai-generative-remove-and-lens-blurring/. Brush/Object-aware selection; also "Distraction Removal" with Dust and, in recent versions, Reflection removal (15.6) [A search snippet]. Sliders: Size, Feather, Opacity, Visualize Spots, Heal vs Clone, Sync spot positions across images [K].
- **Red Eye**: Pupil size, Darken (in LR) and Pet Eye option [K].

### 3.15 Masking
Local adjustments in LR 10 (Oct 2021) introduced the Masking panel, which replaced Graduated Filter/Radial Filter/Adjustment Brush [K].
- **Mask components** [S jkost, https://jkost.com/blog/2024/09/making-selective-edits-using-the-masking-tools-in-lightroom-classic.html]:
  - AI: **Subject, Sky, Background, Objects (Brush or Rectangle select, auto-refined), People** (select individual person; parts: face skin, body skin, eyes (eyebrows, sclera, iris & pupil), lips, teeth, hair, facial hair, clothes) [A search snippets], **Landscape** (water, vegetation, mountains, buildings, architecture, natural ground, artificial ground, sky, etc. — "Check All/None" in 15.6) [A].
  - Geometric: **Linear Gradient** (M), **Radial Gradient** (Shift+M) with feather, invert; Shift-drag constraints.
  - **Brush** (K): Size, Feather, Flow, Density, Auto Mask (edge-aware); eraser toggle via Alt. 15.5 adds Feather and Edge controls when masking [A].
  - **Range masks**: Color Range (click to sample, shift-click up to 5 samples, Refine slider), Luminance Range (range rectangle, smoothness), **Depth Range** (needs depth map; originally HEIC portrait depth data only; newer versions can compute depth from photo via AI as in Lens Blur [K]) [S jkost].
- **Mask ops**: Add, Subtract, Intersect; Invert (`'`); per-mask Amount slider scales all adjustments [S]; copy/paste/duplicate masks, mask overlay (`O`) colours; mask presets saved with adjustments.
- **Adjustment sliders per mask**: Exposure, Contrast, Highlights, Shadows, Whites, Blacks, Texture, Clarity, Dehaze, Hue, Saturation, Temp, Tint, Sharpness (Amount), Noise, Moire, Defringe, Color (tint swatch), Point Color, Curve, Color Grading (wheels), Lens Blur amount, Mask-specific profile Look [K].

### 3.16 Lens Blur
- AI-estimated depth map; synthetic bokeh with Amount slider, Bokeh type (Circle, Bubble, 5-blade, Ring, Cat Eye, etc.), Focal range (set sharp depth with eyedropper), Visualise Depth, Optical vignette, Highlights boost, Focal range feather. Added May 2024 [S] https://www.dpreview.com/news/6028427022/adobe-updates-lightroom-with-generative-remove-and-lens-blur-tools; [K for slider names].

### 3.17 Merge
- **Photo Merge > HDR**: Auto Align, Auto Tone, Deghost (None/Low/Medium/High), Create Stack. Output is a 16-bit linear-ish floating-point DNG (for raws) [K].
- **Panorama**: Spherical, Cylindrical, Perspective, Auto Crop, **Boundary Warp** slider, Fill Edges (generative/content-aware); output DNG.
- **HDR Panorama**: combined.
- Output DNGs maintain raw editing latitude.

### 3.18 HDR editing & output
- **HDR mode** in Basic (button "HDR"), for supported HDR photos (merged DNGs, mobile HDR, HEIF); HDR histogram, "Highlight Priority/Headroom" settings; Editing in HDR display uses extended luminance up to ~SDR white × 4 (per display capabilities) [A snippet https://helpx.adobe.com/lightroom-classic/help/hdr-output.html and Greg Benz https://gregbenzphotography.com/hdr-photos/lightroom-adds-hdr-edit-and-export/].
- Export: **HDR Output** checkbox with AVIF, JPEG XL, JPEG (Ultra HDR gain-map), TIFF, PSD, PNG supported; AVIF/JPEG XL recommended for web; TIFF/PSD for further compositing [A].
- Colour space for HDR export: Rec.2020 / P3 with PQ or HLG [K].
- **Render to DNG** (15.5): save photo as rendered DNG with edits baked in [A snippet].

---

## 4. Presets and profiles (user perspective)

- **Develop presets**: collections of slider values (`.xmp`; Lightroom Classic native `.lrtemplate` legacy). Groups: Color, Creative, B&W, Curve, Grain, Optics, Sharpening, Vignetting; **Adaptive** (Subject, Sky, Background, Portrait...); presets can include masks (re-computed per image) [S jkost].
- Preset creation dialog: select which settings groups to include (Treatment & Profile, WB, Basic Tone, Tone Curve, HSL/Color, Color Grading, Detail, Lens Corrections, Transform, Effects, Calibration, Crop, Spot Removal, Masking, Process Version). Uncheck "Exposure/WB" for look-only presets.
- **Amount slider** on presets (0-200): scales applied values relative to default; Amount for presets and for profiles (see 3.2) [K].
- Import/export: right-click > Import/Export presets; `.xmp` / `.lrtemplate` / `.zip` bundles; preset folders in `~/Library/Application Support/Adobe/CameraRaw/Settings`. Presets shared with ACR (`.xmp` format with `crs:` keys) [K].
- Apply on import, Quick Develop panel preset popup, Auto Sync.
- **Metadata presets**: IPTC/Copyright templates applied on import/export. **Export presets**: saved Export dialog settings, plus plugin-presets. **Keyword sets**, **Filename templates**, **Print templates**, etc.

---

## 5. Export and output

### 5.1 Export dialog (File > Export, `Ctrl/Cmd+Shift+E`) [K]
- **Export To**: hard drive, email, CD/DVD. Folder: choose, same-as-original, subfolder; "Add to this catalog", "Add to stack". Existing files: ask/overwrite/skip/unique name.
- **File Naming**: templates (custom text, sequence, original name, date).
- **Video** export (H.264, DPX).
- **File Settings**: Format JPEG (quality 0-100, limit size to X KB), PSD (8/16-bit), TIFF (None/ZIP/LZW compression, 8/16-bit), DNG (compatibility, JPEG preview size, fast-load data, lossy compression, embed original), **Original**, **PNG**, and (recent versions) **AVIF**, **JPEG XL**, with HDR output option [A snippets supported-file-formats + hdr-output]. Colour Space: sRGB, AdobeRGB (1998), ProPhoto RGB, Display P3, Rec.2020 (HDR), PQ/HLG.
- **Image Sizing**: Resize to fit: Width & Height / Dimensions / Long edge / Short edge / Megapixels / Percentage; Don't Enlarge; Resolution (ppi); **Super Resolution** option inside export resize [K for last].
- **Output Sharpening**: Sharpen For Screen / Matte Paper / Glossy Paper, Amount Low/Standard/High [K].
- **Metadata**: All / Copyright only / Copyright & Contact / All except Camera & Camera Raw info / All except Location; Remove Person Info; Remove Location Info; Write Keywords as Lightroom Hierarchy; Write keyword hierarchy [K].
- **Watermarking**: simple text (font, style, alignment, shadow, size) or graphic (PNG/JPEG), position/anchor/inset/opacity; Watermark Editor with saved watermarks [K].
- **Post-Processing**: Show in Finder/Explorer, Open in Photoshop/other app, custom action.
- Presets: User presets + Adobe defaults (Burn Full-sized JPEGs, Export to DNG, For Email); plugin presets.
- **Publish Services**: Hard Drive, Adobe Stock, Behance (older), Facebook/Flickr via third-party plugins; incrementally sync collections; republish modified photos [K].

### 5.2 Other modules
- **Book**: auto-layout via templates, Blurb printing, export PDF/JPEG, page layouts and cell guides.
- **Slideshow**: layouts, backdrop, overlays, music, playback & export PDF/JPEG/video.
- **Print**: layout styles (Single Image/Contact Sheet, Picture Package, Custom Package), page setup, print job to printer or JPEG file, colour management (Managed by Lightroom with ICC profile / Printer), print sharpening, Print Adjustment slider; draft mode.
- **Web**: HTML & Flash galleries (HTML galleries deprecated; removed in Classic 13 [K unverified]).
- **Map**: see 2.3.
- **Edit In**: Photoshop, other external editors (TIFF/PSD/JPEG with options), Merge to Panorama/HDR in Photoshop, Open as Smart Object, Edit in Photoshop Camera Raw etc. Result returned as a stacked image beside original [K].
- **Plugins**: Lua-based **Lightroom SDK** (`Info.lua`, `LrView`, `LrTasks`, `LrExportServiceProvider`, `LrMetadataProvider`, filters, Library/Develop-menu items). Used for exporters, publishing, metadata, geotagging, and tools like Jeffrey's; Classic only [S wiki mentions Lua ~40%].

---

## 6. Tethering, sync, mobile

- **Tethered capture** (Classic): File > Tethered Capture; supported Canon/Nikon (some Sony) cameras via vendor SDKs; session name, segment, shot name, destination, develop preset on import, metadata preset, live Tether bar (shutter button, ISO etc.), Auto Import (watched folder) as fallback [K].
- **Sync**: Classic "Sync with Lightroom" for selected collections (smart previews uploaded; edits and metadata sync both ways; originals optional via Lightroom ecosystem). Cloud Lightroom: full original upload, albums, shared albums/links (web galleries), version history of edits [K]; 15.4 keyword sync across the ecosystem [A snippet].
- **Mobile (iOS/Android)**: capture (Pro mode with DNG & HDR), full editing parity for most panels incl. masking/AI, preset sync, Discover/community, "Lightroom for mobile" HDR capture produces DNG with HDR profile [K].
- **Web**: Lightroom web client for browse/edit/share.

---

## 7. MVP / v1 / later / out-of-scope matrix (recommendation for free OSS clone)

| Feature | Tier | Rationale |
|---|---|---|
| Raw decode (libraw/rawler), colour matrix/DCP-lite profile, WB | MVP | Core pipeline. |
| Exposure, Contrast, Highlights, Shadows, Whites, Blacks (PV2012-like) | MVP | Defining UX; needs a custom tone-mapper approximation. |
| Temp/Tint, Auto WB, eyedropper | MVP | Basic. |
| Vibrance/Saturation | MVP | Cheap. |
| Tone curve (point curve + RGB channels) | MVP | Easy, high value. |
| Parametric curve with split points | v1 | Simple on top of curve. |
| HSL (8 bands) | MVP | Expected. |
| Crop/rotate/aspect | MVP | Mandatory. |
| Histogram + clipping overlay | MVP | Cheap. |
| Sharpening, Luminance/Colour NR (classic) | MVP | Needed for raw. |
| Non-destructive history/XMP sidecar (`crs:` compatible) | MVP | Interop + persistence. |
| Library grid, rating/flag/colour label, keywords, basic filter | MVP | Culling workflow. |
| Import copy/add with rename templates | MVP | Entry path. |
| Export JPEG/PNG/TIFF, sRGB/P3/AdobeRGB, resize, output sharpen | MVP | Minimum deliverable. |
| Texture, Clarity, Dehaze | v1 | Need careful algorithms. |
| Color Grading (3-way + global, Blending, Balance) | v1 | Straightforward; high demand. |
| Linear/radial gradient and brush masks, mask add/subtract/intersect | v1 | Local edits essential. |
| Luminance/Colour range masks | v1 | Cheap to implement. |
| Profile browser with LUT profiles + Amount | v1 | Needed for looks; import .cube/.dcp. |
| Presets (apply/save/import XMP), Amount | v1 | Community ecosystem. |
| Lens corrections (profile via lensfun, CA, defringe, vignetting) | v1 | lensfun provides data. |
| Transform / Upright (Auto/Vertical/Level/Full) | v1 | OpenCV-style line detection; Guided later. |
| Snapshots, virtual copies, stacks, smart collections | v1 | Catalog features. |
| Effects: vignette, grain | v1 | Cheap. |
| Spot heal/clone | v1 | Needed; content-aware via inpainting later. |
| Point Color | later | Complex UI/range math. |
| Calibration panel | later | Niche. |
| Soft proofing | later | Needs ICC pipeline. |
| Smart previews, 1:1 previews cache | v1 | Performance. |
| Merge HDR / Panorama / HDR pano | later | Use Hugin/enfuse integrations. |
| AI masks (Subject, Sky, Background, People parts, Landscape, Objects) | later | Use open models (SAM 2, MobileSAM, SegFormer); heavy. |
| AI Denoise, Super Resolution, Raw Details | later | Open models (e.g., NAFNet/Real-ESRGAN); slow. |
| Lens Blur / depth masks | later | Monocular depth models (Depth Anything). |
| Generative Remove | out-of-scope | Cloud generative model; replace with local LaMa inpainting later. |
| Face recognition/People | later | Local model (insightface); privacy-friendly. |
| Map module/GPS tagging (GPX) | later | OpenStreetMap tiles. |
| Smart collections rules UI | v1 | Reasonably cheap. |
| Tethering | later | libgphoto2. |
| Print module | later | Use CUPS + ICC. |
| Book / Slideshow / Web modules | out-of-scope | Low demand, high effort. |
| Publish services (cloud) | later | Plugin API for exporters. |
| Plugin SDK (Lua parity) | out-of-scope | Provide own plugin API (e.g., WASM/Python). |
| HDR editing/AVIF, JXL output | later | Export AVIF/JXL SDR in v1, HDR later. |
| Cloud sync/mobile apps | out-of-scope | Syncthing/WebDAV optional. |
| Watermarking | v1 | Easy. |
| Lightroom `.lrcat` import | later | SQLite; reverse-engineer schema (read-only). |
| XMP develop-setting read for `crs:` | v1 | Migration path; document differences. |
| Reference view, before/after | v1 | UI polish. |
| Adaptive presets | later | Dependent on AI masks. |

---

## 8. Open items / not verified
- Exact full-page content from helpx.adobe.com (403): all slider ranges tagged [K] need verification, especially Sharpening Amount default, Point Color max swatches, Lens Blur slider list, Process Version "4/5/6" semantics, Web module status, Depth Range availability beyond HEIC portrait, Enhance default strength, HDR export colour space options.
- Exact formulas for PV2012 tone algorithms, Clarity, Texture, Dehaze, and Vibrance are proprietary; use community reverse-engineering (e.g., darktable, RawTherapee, ART) as approximations.
