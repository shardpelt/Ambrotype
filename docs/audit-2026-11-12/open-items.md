# Open items (read-only pass)

Repo: /home/user/ambrotype. Paths below are relative to it.
Read in full: docs/WHATS-NEXT.md, docs/NEXT.md, docs/GAPS.md, docs/SCOPE.md, docs/design/future-ideas.md, docs/lightroom-trial-plan.md, docs/cleanup.md. Other docs were not read, so items only in those are not listed.
"Recently done" / "Done" sections skipped. Items marked DISPUTED are open in one doc and done (or otherwise contradicted) in another; see Contradictions.
Line numbers are from the files as read. The GAPS.md rows are open gaps by definition, so every row that names a missing, partial, approximate or question item is listed.

## Engine/raw pipeline

- Camera support: per-camera fitted fallback profiles and a sample-submission pipeline not built | sources: docs/GAPS.md:76, docs/SCOPE.md:293, docs/SCOPE.md:394 | size: ? | blocker: -
- 4-colour CFA sensors rejected | sources: docs/GAPS.md:122 (crates/amb-engine/src/raw.rs:95-99, 118-122) | size: ? | blocker: -
- Cameras without a colour matrix in rawler are rejected | sources: docs/GAPS.md:123 (crates/amb-engine/src/raw.rs:69-85) | size: ? | blocker: -
- Demosaic: no choice (AMaZE, LMMSE); X-Trans 3-pass not ported; highlight rebuild only for one or two clipped channels; no capture sharpening or deconvolution | sources: docs/GAPS.md:124, docs/SCOPE.md:122, docs/SCOPE.md:311 | size: ? | blocker: -
- Non-raw inputs: HEIC, AVIF, PSD not opened; LUT-based ICC falls back to sRGB; a raw and its JPEG cannot show as two separate photos | sources: docs/GAPS.md:121 | size: ? | blocker: -
- LibRaw fallback in a sandboxed helper process not built (rawler only) | sources: docs/GAPS.md:139, docs/SCOPE.md:377 | size: ? | blocker: -
- GPU preview: CPU steps still inside it (camera profile tables, defringe, spots, masks, LUTs, blank-edge fill); no per-stage caching; region renders take globals from a 1600 px copy | sources: docs/GAPS.md:288, docs/GAPS.md:359, docs/design/gpu.md | size: ? | blocker: -
- GPU precision and wgpu limits unresolved: fp16 vs fp32 intermediates, storage-texture formats, buffer sizes, HDR swapchain | sources: docs/GAPS.md:292, docs/SCOPE.md:276 | size: ? | blocker: - (GPUs from Intel, AMD, NVIDIA, Apple needed to test)
- Lens warp costs ~1.5 s per 24 MP on CPU; GPU resampling not done; defringe is a separate CPU pass | sources: docs/GAPS.md:289 | size: ? | blocker: -
- Default sharpening + colour NR ~1.0 s per 24 MP; Luminance NR (off by default) adds ~1.4 s | sources: docs/GAPS.md:290 | size: ? | blocker: -
- SIMD and assembly performance ladder not started | sources: docs/GAPS.md:294, docs/SCOPE.md:350-361 | size: ? | blocker: -
- Focus stacking (after HDR and panorama, as a merge) | sources: docs/GAPS.md:55, docs/SCOPE.md:128 | size: L | blocker: -
- Lens-softness correction from measured lens data | sources: docs/GAPS.md:186, docs/SCOPE.md:126 | size: ? | blocker: -
- HDR merge: translation-only alignment, raws only, no Create stack or HDR panorama, own weights and deghost rule; DNG not yet opened in Lightroom | sources: docs/GAPS.md:113, docs/design/hdr-merge.md, docs/lightroom-trial-plan.md:153 | size: ? | blocker: Lightroom trial
- Panorama merge: no Boundary Warp, Fill Edges, HDR panorama, Create stack or preview; raws only; no parallax; DNG not yet opened in Lightroom | sources: docs/GAPS.md:114, docs/design/panorama.md | size: ? | blocker: Lightroom trial
- Sharpening: unsharp mask; Radius, Detail and Masking curves guessed | sources: docs/GAPS.md:180 | size: ? | blocker: Lightroom trial (fit, docs/GAPS.md:322)
- Noise reduction: no camera noise profile; constants guessed | sources: docs/GAPS.md:181 | size: ? | blocker: Lightroom trial (fit, docs/GAPS.md:322)
- Post-crop vignette (3 styles): model guessed; centred and scaled to the crop | sources: docs/GAPS.md:183 | size: ? | blocker: Lightroom trial (fit, docs/GAPS.md:322)
- Grain model guessed; whether Lightroom stores a GrainSeed unknown | sources: docs/GAPS.md:184 | size: ? | blocker: Lightroom trial (fit, docs/GAPS.md:322)
- Upright Guided (PerspectiveUpright 5) left off (its guides undocumented); Upright analysis tags (Upright*) not read | sources: docs/GAPS.md:193, docs/design/transform.md | size: ? | blocker: Lightroom trial (plan 4.9, docs/lightroom-trial-plan.md:152)
- Crop and straighten: angle sign, corner semantics and Constrain Crop interaction unverified | sources: docs/GAPS.md:193, docs/NEXT.md:105, docs/design/crop.md | size: ? | blocker: Lightroom trial
- Upright found again from the photo's lines, not read from UprightTransform_0..5 | sources: docs/GAPS.md:194, docs/research/05-processing-algorithms.md §15 | size: ? | blocker: Lightroom trial (docs/lightroom-trial-plan.md:152)
- Transform slider signs, strengths, Projection (18.3) and parameter-to-homography mapping: guesses | sources: docs/GAPS.md:195, docs/NEXT.md:106 | size: ? | blocker: Lightroom trial (docs/lightroom-trial-plan.md:152)
- Built-in lens profiles in maker notes not read (MFT, Fujifilm X, many compacts); Sony vignetting and CA tags not read; DNG opcodes partly | sources: docs/GAPS.md:196, docs/design/lens-corrections.md:83-86 | size: ? | blocker: -
- Fisheye profiles and projection changes unsupported | sources: docs/GAPS.md:197, docs/design/lens-corrections.md:18, 89 | size: ? | blocker: -
- Local Defringe: strength is a guess, to fit in the trial | sources: docs/GAPS.md:198, docs/design/masks.md | size: ? | blocker: Lightroom trial
- lensfun database not bundled; the editor downloads it (one click) | sources: docs/GAPS.md:199, docs/design/lens-corrections.md | size: ? | blocker: -
- LCP auto-match scans every LCP; slow on very large Adobe installs (no timing recorded) | sources: docs/GAPS.md:201 (crates/amb-lens/src/lcp.rs:599-640) | size: ? | blocker: -
- lensfun calibrations differ from Adobe's; exact match needs the user's own LCP | sources: docs/GAPS.md:202, docs/design/lens-corrections.md:44 | size: ? | blocker: -
- Lens profile matching checked on two real raws only | sources: docs/GAPS.md:203, docs/design/lens-corrections.md:69-72 | size: ? | blocker: -
- Blank-edge fill (blank_fill 1.0, "to verify") and profile auto-scale for barrel lenses unverified | sources: docs/GAPS.md:204 (crates/amb-engine/src/lens/mod.rs:43) | size: ? | blocker: Lightroom trial
- With Remove CA off, does an LCP's CA model still apply (profile_tca_always)? | sources: docs/GAPS.md:205 | size: ? | blocker: Lightroom trial
- Profile Amount semantics guessed (linear displacement for distortion, gain power for vignetting) | sources: docs/GAPS.md:206, docs/design/lens-corrections.md:78 | size: ? | blocker: Lightroom trial
- Manual Distortion strength, Lens Vignetting stops and Midpoint, Defringe hue mapping and edge width, auto-CA constants: need fitting | sources: docs/GAPS.md:207 | size: ? | blocker: Lightroom trial

- Chromatic aberration removal without a lens profile | sources: docs/GAPS.md:51, docs/WHATS-NEXT.md:147 | size: M | blocker: - | DISPUTED (Contradictions 7)
## Colour & film

- Two camera calibration matrices mixed by temperature; a DNG's ForwardMatrix and BaselineExposure not read | sources: docs/GAPS.md:125, docs/SCOPE.md:286, docs/research/05-processing-algorithms.md §1 | size: ? | blocker: -
- No Adobe-like base look: placeholder ACES-fit curve ("NOT Adobe's"); no fitted base look of our own | sources: docs/GAPS.md:126, docs/ARCHITECTURE.md:87, docs/SCOPE.md:25-29, docs/NEXT.md:100, docs/NEXT.md:108 | size: ? | blocker: Lightroom trial data (docs/NEXT.md:108)
- Adobe Raw .xmp profiles (Adobe Color, the default) not read | sources: docs/GAPS.md:128, docs/SCOPE.md:26, 251, docs/NEXT.md:100 | size: ? | blocker: -
- Creative profiles: Look, RGBTable, LookTable not rendered; Table_<md5> needs the base-85 alphabet and the crs: property names | sources: docs/GAPS.md:231, docs/research/02-preset-and-profile-formats.md:241, docs/NEXT.md:19, docs/NEXT.md:108 | size: ? | blocker: two facts from the user or community (docs/NEXT.md:19)
- Fitted per-camera replacements for Adobe Color, Standard, Neutral, Landscape, Portrait, Vivid, Camera Matching profiles (planned from Lightroom renders, week 3) | sources: docs/GAPS.md:127, docs/GAPS.md:320, docs/SCOPE.md:28, docs/SCOPE.md:390, docs/lightroom-trial-plan.md:54, 143 | size: ? | blocker: Lightroom trial
- Adaptive profiles and adaptive presets (image-adaptive AI) not reproducible; fallback is a base profile plus a warning | sources: docs/GAPS.md:129, docs/SCOPE.md:240 | size: ? | blocker: -
- Camera Raw Calibration panel (Shadow Tint, primaries Hue and Saturation): own unfitted model; the working space is unknown | sources: docs/GAPS.md:130, docs/design/calibration.md, docs/lightroom-trial-plan.md:144 | size: ? | blocker: Lightroom trial
- B&W mixer: per-band strength is a guess; no Auto mix (AutoGrayscaleMix); no B&W camera profiles | sources: docs/GAPS.md:131 | size: ? | blocker: Lightroom trial
- Point Color (PointColors) missing; encoding from memory; waits for a Lightroom sample | sources: docs/GAPS.md:132, docs/NEXT.md:99, docs/NEXT.md:109, docs/research/02-preset-and-profile-formats.md:159, docs/lightroom-trial-plan.md:145 | size: ? | blocker: Lightroom sample of PointColors (docs/NEXT.md:99)
- White balance IncrementalTemperature and IncrementalTint scale: own, unfitted | sources: docs/GAPS.md:133, docs/research/02-preset-and-profile-formats.md:119 | size: ? | blocker: -
- Temp/Tint: low-Kelvin range (below 2000 K) and readouts unmeasured | sources: docs/GAPS.md:134, docs/GAPS.md:324, docs/research/05-processing-algorithms.md §2 | size: ? | blocker: Lightroom trial (docs/GAPS.md:324)
- Custom white balance without Temperature is not rendered; Tint alone is refused | sources: docs/GAPS.md:135 | size: ? | blocker: -
- Tone curves act on sRGB-encoded values; Adobe's curve domain unknown. Whether Whites and Blacks are global also unknown | sources: docs/GAPS.md:136, docs/GAPS.md:149, docs/GAPS.md:325, docs/research/05-processing-algorithms.md §8, §20 | size: ? | blocker: Lightroom measurement (docs/GAPS.md:325)
- Exact tone-curve interpolation and Medium/Strong Contrast point sets unknown; legacy ToneCurve, ToneCurveName, ToneCurveName2012 tags planned, not rendered | sources: docs/GAPS.md:137, docs/GAPS.md:247, docs/research/02-preset-and-profile-formats.md:138 | size: ? | blocker: Lightroom measurement
- Process-version numbers after 11.0 (PV5/PV6, 13.x/15.x) from memory; ProcessVersion-to-preset mapping unstated | sources: docs/GAPS.md:138, docs/research/02-preset-and-profile-formats.md:102, 259, 329 | size: ? | blocker: Lightroom trial (docs/GAPS.md:316)
- Highlights and Shadows: strength, curve shape and radius are guesses | sources: docs/GAPS.md:147, docs/SCOPE.md:391 | size: ? | blocker: Lightroom trial (fit)
- Contrast, Whites, Blacks, parametric-curve strengths and shapes are guesses | sources: docs/GAPS.md:148 | size: ? | blocker: Lightroom trial (fit)
- Exposure: Adobe's soft highlight roll-off for positive Exposure only inferred; ours is a plain gain | sources: docs/GAPS.md:150, docs/research/05-processing-algorithms.md §3 | size: ? | blocker: Lightroom trial
- Parametric curve is a smooth-bump model; Adobe's formula undocumented | sources: docs/GAPS.md:151 | size: ? | blocker: Lightroom trial
- Vibrance, Saturation, 8-band HSL and Color Grading in OkLab/OkLCh; HSL band centres and weights guessed from HSV hues | sources: docs/GAPS.md:152, docs/research/05-processing-algorithms.md §9 | size: ? | blocker: Lightroom trial
- Colour Grading region masks, Blending and Balance shapes guessed; legacy Split Toning mapped onto the wheels | sources: docs/GAPS.md:153, docs/research/05-processing-algorithms.md §10 | size: ? | blocker: Lightroom trial
- Texture and Clarity: guided-filter model; Adobe's likely Local-Laplacian; rated hard | sources: docs/GAPS.md:154, docs/research/05-processing-algorithms.md §5 | size: ? | blocker: Lightroom trial
- Dehaze: dark-channel model; Adobe's contrast, saturation and tone shaping inferred; rated hard | sources: docs/GAPS.md:155, docs/research/05-processing-algorithms.md §6 | size: ? | blocker: Lightroom trial
- Native scene-referred rendering mode (filmic, sigmoid, AgX) not built | sources: docs/GAPS.md:156, docs/SCOPE.md:121, docs/SCOPE.md:311 | size: ? | blocker: -
- Tone equaliser not built | sources: docs/GAPS.md:156, docs/SCOPE.md:120, docs/SCOPE.md:324 | size: ? | blocker: -
- Auto (tone and WB): calibrated against Lightroom's Auto not done; no Auto for Vibrance or Saturation; WB moves only when near-grey covers 15 % of the photo | sources: docs/GAPS.md:273, docs/GAPS.md:91, docs/design/future-ideas.md:7 | size: ? | blocker: Lightroom trial (what Auto writes, docs/GAPS.md:317)
- Auto: Shift-click per slider (auto for that slider only) | sources: docs/design/future-ideas.md:7, 12 | size: ? | blocker: -
- Presets: Preset Amount is a provisional linear lerp that extrapolates above 100 %; Adobe's maths unknown; non-numeric settings taken whole when Amount > 0 | sources: docs/GAPS.md:228 (crates/amb-presets/src/amount.rs:3-8), docs/SCOPE.md:246, docs/lightroom-trial-plan.md:142 | size: ? | blocker: Lightroom trial (plan 4.3)
- Two Amount sliders (preset Amount since ACR 14.4; profile Amount for some profiles); neither scaling documented | sources: docs/GAPS.md:230, docs/SCOPE.md:246 | size: ? | blocker: Lightroom trial
- Range mismatches with the old XMP spec table (Brightness -150..150 vs 0..150; Sharpness 0..150 vs 0..100); kept wider | sources: docs/GAPS.md:229 (crates/amb-presets/data/tags.json) | size: ? | blocker: Lightroom trial (getRange, docs/lightroom-trial-plan.md:140)
- Tag registry ranges, defaults and steps mostly unverified; several (Point Color, LensBlur, HDR, mask enums) from memory | sources: docs/GAPS.md:236, docs/research/02-preset-and-profile-formats.md:329 | size: ? | blocker: Lightroom trial (docs/lightroom-trial-plan.md:140)
- No crs: tag has status "exact"; all 157 rendered Camera Raw tags are "approx" | sources: docs/GAPS.md:237 | size: ? | blocker: Lightroom trial
- Tags planned or unsupported: planned 10 (profile 5 of 6, tone curve 3, Point Color 1, EnableRedEye); unsupported 18 (enhance 8, hdr 9, lens blur 1) | sources: docs/GAPS.md:247-248 | size: ? | blocker: -
- PV2003 and PV2010 edits: 7 old Basic sliders mapped to 2012 ones with our own scales; old tone curve and defaults not modelled | sources: docs/GAPS.md:232 (crates/amb-engine/src/pipeline/settings.rs) | size: ? | blocker: -
- Enable* panel switches: which settings each covers is our reading, unchecked against Lightroom | sources: docs/GAPS.md:233, docs/research/02-preset-and-profile-formats.md:219 | size: ? | blocker: -
- .cube LUTs: assume sRGB in and out; store the file path as is; run on CPU inside the GPU preview | sources: docs/GAPS.md:234, docs/design/luts.md | size: ? | blocker: - (DISPUTED: see Contradictions)
- HDR editing tags (HDREditMode, HDRMaxValue, SDR*) unsupported; HDR-edited photos render as SDR | sources: docs/GAPS.md:235, docs/SCOPE.md:330 | size: ? | blocker: -
- Every model constant is a first guess; no Calibration section fitted (8 sections) | sources: docs/GAPS.md:300 (crates/amb-engine/src/calibration.rs:1-23) | size: ? | blocker: Lightroom trial (fit)
- Fit the Film recipe looks to the owner's Fujifilm shots; film recipe looks are own first guesses | sources: docs/GAPS.md:334, docs/NEXT.md:109, docs/design/film-look-measurement.md | size: ? | blocker: Fujifilm in-camera re-renders with each film simulation (docs/NEXT.md:18)
- Film negatives: Auto levels assume near-neutral extremes; fail on sunset rolls; cast removal tried on a synthetic sunset and two frames only | sources: docs/GAPS.md:213, docs/design/film-negatives.md:34 | size: ? | blocker: -
- Auto off without stored levels: per-layer share measured on the photo, so colour still depends on the scene (slight green-cyan tint on some daylight frames) | sources: docs/GAPS.md:214, docs/design/film-negatives.md | size: ? | blocker: - (fix: Sync negative roll or per-film shares)
- Bare light vs film base: own rule, tried only on synthetic scans; expired-film crossover tried on eight BlueNeg frames | sources: docs/GAPS.md:215 (crates/amb-engine/src/negative/analysis.rs, cast.rs) | size: ? | blocker: -
- Film stocks are own generic looks (13) with no measured data; measured stock profiles and sharing them not built | sources: docs/GAPS.md:219, docs/design/film-negatives.md:47-53, 73, docs/SCOPE.md:143 | size: ? | blocker: measured film data (docs/NEXT.md:18)
- Halation, bloom and grain are the same for every photo; not tied to a film stock (colour and tone only) | sources: docs/GAPS.md:218, docs/WHATS-NEXT.md:16, docs/design/film-negatives.md:72 | size: ? | blocker: measured stock data (docs/GAPS.md:219)
- Film negative model v1 constants are ours; only a synthetic scanned negative is a test target | sources: docs/GAPS.md:220, docs/design/film-negatives.md:26-45 | size: ? | blocker: real scans
- Film lab 1, metering region: draw a region by hand (the levels are measured inside the crop now) | sources: docs/NEXT.md:137 | size: ? | blocker: -
- Film lab 9, a paper curve in density: toe, straight line, shoulder, ISO-R grades, split grade, preflash, B&W papers | sources: docs/WHATS-NEXT.md:15, docs/NEXT.md:148, docs/GAPS.md:43 | size: L | blocker: -
- Film lab 5, dust and scratches: guided rectangle and a heal brush that searches its stroke | sources: docs/WHATS-NEXT.md:17, docs/NEXT.md:143, docs/NEXT.md:144, docs/GAPS.md:43, docs/GAPS.md:216 | size: L (whole item) | blocker: -
- Dust: automatic finding partial (smooth ground only; 73 % of finds real, 37 % of expert repairs); light defects not found | sources: docs/GAPS.md:216, docs/design/dust.md | size: ? | blocker: -
- Dust: a scanner's infrared channel (4th channel or _ir file) not read; parked | sources: docs/GAPS.md:217, docs/NEXT.md:144, docs/design/dust.md | size: ? | blocker: a scan with an infrared channel (owner's scanner, docs/GAPS.md:217)
- Film lab 7, darkroom aids: density and zone under the cursor, zone overlay, placing a spot in a zone, step wedge, negative-range and scan-clip readouts, test strip, ring-around, peek at the negative; Pick Better | sources: docs/WHATS-NEXT.md:18, docs/NEXT.md:146, docs/GAPS.md:43 | size: M | blocker: -
- Film lab 8, scans in: flat-field from a bare-light shot, channel-bleed matrix per film type or from a colour chart, half-frame split, strip split into frames, stitching a large negative | sources: docs/NEXT.md:147 | size: M each | blocker: -
- Film lab 11, B&W processes and finishing: lith, cyanotype, Sabattier, chemical toners, edge burn, filed carrier, chroma denoise that keeps edges, skin chroma ceiling | sources: docs/NEXT.md:154 | size: S-M each (some as effect plugins) | blocker: -
- Fade Restoration for faded prints and slides, in the Film tab | sources: docs/GAPS.md:44, docs/WHATS-NEXT.md:22 | size: M | blocker: -
- Soft proofing: exists for export spaces; not printer and paper ICC profiles; no proof copies or paper-white simulation; display taken as sRGB | sources: docs/GAPS.md:115, docs/SCOPE.md:320 | size: ? | blocker: printer and paper profiles
- Monitor colour: not on Wayland without a chosen file, not LUT-based monitor profiles, not thumbnails; macOS and Windows not checked by hand | sources: docs/NEXT.md:43, docs/GAPS.md:89, docs/GAPS.md:333 | size: ? | blocker: a Mac and a Windows machine
- Wayland colour management needs a profile file chosen in Settings (colord not read) | sources: docs/GAPS.md:333, docs/research/08-cross-platform-and-mobile.md:27 | size: ? | blocker: -
- Film metadata per roll: stock, camera, development; search terms film:... iso:>=400; hot folder | sources: docs/NEXT.md:155 | size: M | blocker: -

## Library & organising

- Speed number for big libraries (100k photos: open, filter, search) not published | sources: docs/GAPS.md:30 | size: S | blocker: -
- Background indexing of whole drives while the editor runs (today only at start and when a drive returns); preview cache for big libraries | sources: docs/NEXT.md:119, docs/GAPS.md:106 | size: ? | blocker: -
- Catalogue: smart collections still check every file; All photos trusts the last read; second-resolution file times (FAT/exFAT) can hide quick same-size changes | sources: docs/GAPS.md:106, docs/NEXT.md:98 | size: ? | blocker: -
- Full-text search (FTS5) and queries straight from the catalogue; import by move; keyword hierarchy and synonyms; preview pyramid | sources: docs/GAPS.md:107, docs/SCOPE.md:318 | size: ? | blocker: -
- .lrcat migration: tested only on synthetic catalogues; smart collections named, not converted; publish services, edit history, snapshots, face regions, location not moved; keyword hierarchies flat; opened in place it is read-only (changes reach Lightroom only via sidecars); virtual copies not shown; moved photos not remapped; unsaved Lightroom edits not seen | sources: docs/GAPS.md:108, docs/SCOPE.md:278, docs/SCOPE.md:319, docs/research/06-tech-stack-options.md:425 | size: ? | blocker: real Lightroom Classic catalogues of several versions
- Data safety: rebuildable catalogue, versioned recipes, automatic catalogue backups listed as not implemented | sources: docs/GAPS.md:109, docs/SCOPE.md:62-66 | size: ? | blocker: - | DISPUTED (see Contradictions 17)
- Safety: checksums kept in the catalogue; backup check (every photo also on the backup drive); space per folder, year and camera | sources: docs/NEXT.md:125 | size: ? | blocker: -
- Several computers: library on a NAS or synced folder, each machine with its own catalogue cache | sources: docs/NEXT.md:126 | size: ? | blocker: -
- Videos: list and play clips next to photos, with ratings and keywords | sources: docs/NEXT.md:128 | size: ? | blocker: -
- Tethered shooting (libgphoto2) | sources: docs/GAPS.md:112, docs/SCOPE.md:125, docs/SCOPE.md:330 | size: ? | blocker: -
- Resumable import; mapping of Lightroom smart collections | sources: docs/SCOPE.md:323 | size: ? | blocker: -
- File care partial: trash tested on Linux only (Mac and Windows untried); keyword list flat; only the last move can be undone, and only while the app runs; photos can't be dragged onto a folder | sources: docs/GAPS.md:103, docs/NEXT.md:124 | size: ? | blocker: -
- Duplicates and bursts: fingerprint is a difference hash of the camera preview (misses heavily cropped or edited copies; a photo without a preview needs a render first); sharpness is whole-picture | sources: docs/GAPS.md:104 (crates/amb-library/src/duplicates.rs) | size: ? | blocker: -
- Duplicates dialog: delete the extra copies (to trash) from the dialog | sources: docs/NEXT.md:121 | size: ? | blocker: -
- Culling help: exposure score missing; sharpness and closed-eyes scoring listed as still to do | sources: docs/NEXT.md:122, docs/GAPS.md:104 | size: ? | blocker: - | DISPUTED (Contradictions 10)
- Best in each burst: eye judgement is a heuristic (sunglasses, squint, profile can fool it; faces under 16 px skipped) | sources: docs/GAPS.md:170 | size: ? | blocker: -
- Timeline and Map: pins can't be dragged (place again instead); map images need the network and OpenStreetMap servers | sources: docs/GAPS.md:105 | size: ? | blocker: -
- Place search by name | sources: docs/NEXT.md:120, docs/GAPS.md:105 | size: ? | blocker: - | DISPUTED (Contradictions 4)
- Reverse geocoding: city and country names from placed photos (metadata, search, captions) | sources: docs/GAPS.md:37, docs/NEXT.md:120 | size: S | blocker: -
- Pick flags kept in amblib:Flag and stacks in amblib:Stack (Lightroom does not read them); collections are lists by path; keyword hierarchies kept but not edited | sources: docs/GAPS.md:86, docs/GAPS.md:103 | size: ? | blocker: -
- No undo for rating, label and keyword changes | sources: docs/GAPS.md:86 | size: ? | blocker: -
- Loupe and compare do not zoom | sources: docs/GAPS.md:86 | size: ? | blocker: - | DISPUTED (Contradictions 5)
- Rotate and flip: a virtual copy can't turn on its own; the turn is not a History step | sources: docs/GAPS.md:192 | size: ? | blocker: -
- Film metadata per roll (stock, camera, development), search terms film:... iso:>=400, hot folder | sources: docs/NEXT.md:155 | size: M | blocker: -
- Face recognition / people by name: not built, on purpose for now | sources: docs/GAPS.md:111, docs/GAPS.md:112 | size: ? | blocker: -

## Develop UI

- Speed Edit: hold a key and drag anywhere on the photo to change the last setting used | sources: docs/GAPS.md:27, docs/WHATS-NEXT.md:22 | size: S | blocker: -
- Peek on hover: area round the pointer magnified after a moment still (the loupe, I, is a mode now) | sources: docs/GAPS.md:36, docs/WHATS-NEXT.md:47 | size: S | blocker: -
- Second window for the photo (viewer on a second screen) | sources: docs/GAPS.md:48, docs/WHATS-NEXT.md:22 | size: M | blocker: -
- MIDI controllers for sliders | sources: docs/GAPS.md:49, docs/WHATS-NEXT.md:22 | size: M | blocker: -
- Explain an edit: Auto, Match look and the Assistant say why each change helps, on the photo | sources: docs/GAPS.md:45 | size: M | blocker: -
- AI coach: explains why a preset looks the way it does, or suggests the next step (bring-your-own-AI) | sources: docs/design/future-ideas.md:48, docs/GAPS.md:93 | size: ? | blocker: -
- Basic's own sliders past their ranges in Trippy (would be our own extra settings) | sources: docs/GAPS.md:35, docs/WHATS-NEXT.md:35 | size: S-M | blocker: -
- Surprise me: locks (keep white balance, exposure), scope to one panel, "mutate" around the current look, film stocks, entering a seed, Save as preset (status not stated) | sources: docs/GAPS.md:53, docs/GAPS.md:92, docs/design/future-ideas.md:26, 31-34 | size: S-M | blocker: -
- Teach by showing: before/after strip per slider on this photo (hover) | sources: docs/GAPS.md:52, docs/design/future-ideas.md:46 | size: M | blocker: -
- Teach by showing: tips while waiting; tips where text normally is (search box, empty menu); "one answer leads to more" | sources: docs/WHATS-NEXT.md:30-32 | size: ? | blocker: - | DISPUTED (Contradictions 8)
- Lightroom switchers' lesson mapping Lightroom panels and shortcuts to ours, and how to load their presets and sidecars | sources: docs/GAPS.md:52, docs/design/future-ideas.md:47 | size: M | blocker: - | DISPUTED (Contradictions 9)
- "?" help: more photography terms, terms in Learn's lessons, translations | sources: docs/NEXT.md:41 | size: ? | blocker: -
- Preset UX: layered presets, applying a preset through a mask, opt-in adaptation assist | sources: docs/GAPS.md:95, docs/SCOPE.md:252, docs/SCOPE.md:255 | size: ? | blocker: -
- Saved presets: no rename or delete in the app (use the presets folder) | sources: docs/GAPS.md:334 | size: ? | blocker: -
- Layouts: two side columns, not free splits; each setting can be shown once | sources: docs/GAPS.md:83 | size: ? | blocker: -
- Crop tool: no JS tests; keeps the preset name in the aspect list after an X swap; does not write CropWidth, CropHeight, CropUnits | sources: docs/GAPS.md:83 (crates/amb-gui/ui/tools/croptool.js, crop.js) | size: ? | blocker: -
- Keyboard shortcut sets can't be shared (Settings keys can be changed) | sources: docs/GAPS.md:83 | size: ? | blocker: -
- Menu bar: untried on a Mac; keys without Cmd/Ctrl/Alt show in titles; no check marks on on/off items; hidden on Windows and Linux unless switched on (choice kept per computer in webview storage) | sources: docs/GAPS.md:84, docs/NEXT.md:63 | size: ? | blocker: a Mac
- Native GPU viewport not built: previews reach the webview as JPEG; wry build_as_child is X11-only on Linux; Wayland needs the gtk::Fixed route; stability on all three OSes unproven | sources: docs/GAPS.md:82, docs/GAPS.md:97, docs/SCOPE.md:275, docs/SCOPE.md:393, docs/research/08-cross-platform-and-mobile.md:35 | size: ? | blocker: -
- Preview is rendered at screen size from a downscaled raw, so pixel-radius effects differ from the full-size export | sources: docs/GAPS.md:87 (crates/amb-gui/src/session/mod.rs, render.rs) | size: ? | blocker: -
- Preview speed measured only in a debug build under Xvfb (0.6-1.0 s for 860 px of a 24 MP raw); no draft refinement beyond a 0.6x drag size | sources: docs/GAPS.md:88 | size: ? | blocker: release-build timing on the owner's Mac (docs/NEXT.md:112)

- Blank-canvas layout editor: start a layout from nothing | sources: docs/GAPS.md:34, docs/NEXT.md:29 | size: S | blocker: - | DISPUTED (Contradictions 2)
## Masks & AI

- Masks: fall-off curves, Feather mapping, brush dab model and each local slider's strength are guesses | sources: docs/GAPS.md:162, docs/design/masks.md | size: ? | blocker: Lightroom trial (plan 4.7, docs/lightroom-trial-plan.md:146-150)
- Mask conventions unverified: radial Angle sign; how Lightroom stores erasing inside a brush; luminance range brightness scale; order of lens corrections vs brushes; brushes in the .acr file (MaskBrushTable) not read | sources: docs/GAPS.md:163, docs/design/masks.md | size: ? | blocker: Lightroom trial (docs/lightroom-trial-plan.md:146-150)
- Colour range: Lightroom's AreaModels samples in an unknown format (its colour ranges select nothing here); our amb:ColorSamples not read by Lightroom | sources: docs/GAPS.md:164, docs/design/masks.md | size: ? | blocker: -
- Not rendered inside masks: Moire, LocalHue, LocalGrain, LocalPointColors; depth-range masks | sources: docs/GAPS.md:165, docs/GAPS.md:173 (crates/amb-presets/src/masks.rs UNRENDERED_FIELDS) | size: ? | blocker: -
- Mask editing: polygon lasso and auto-mask (edge-aware brush) not built; old per-tool tags render only | sources: docs/GAPS.md:166, docs/SCOPE.md:321, docs/design/masks.md | size: ? | blocker: -
- AI masks: Select Sky misses sunset and stormy skies; Select Subject picks the most eye-catching thing; Select People is one mask; objects and landscape parts select nothing | sources: docs/GAPS.md:167, docs/NEXT.md:95 | size: ? | blocker: -
- Face-part masks (eyes, lips, skin) | sources: docs/GAPS.md:50, docs/WHATS-NEXT.md:27 | size: M | blocker: -
- Generative fill and expand, relight, face reshaping | sources: docs/GAPS.md:56 | size: L | blocker: - | DISPUTED (Contradictions 20)
- Blemish removal with the heal brush: improved 2026-10-22 (membrane match, whole-pixel copy, shape-matched donors); untried on real portraits; a soft edge still mixes some blemish texture | sources: docs/GAPS.md:169, docs/NEXT.md:96, docs/design/spot-removal.md | size: ? | blocker: -
- AI Remove fills are not Lightroom's: Big-LaMa (~10 s a spot on CPU) or MI-GAN (faster, a little rougher); large removals differ | sources: docs/GAPS.md:170, docs/design/spot-removal.md | size: ? | blocker: -
- Remove object by click needs SAM 2.1 tiny (130 MB, ~9 s the first time); masks asked in words use CLIP (~0.6 s a look), can miss very small things and give a part of an object | sources: docs/GAPS.md:170 | size: ? | blocker: -
- Select object: brush mask in Lightroom's format, not Lightroom's Object mask; its shadow is found by brightness (may take a dark background); donors outside the crop are clamped | sources: docs/GAPS.md:170 | size: ? | blocker: -
- Red eye and pet eye not built (RedEyeInfo format unknown) | sources: docs/GAPS.md:171, docs/NEXT.md:96, docs/lightroom-trial-plan.md:151 | size: ? | blocker: Lightroom sample with red eye (plan 4.8)
- Moving brushed spots by dragging in the editor not built (they can be resized and moved with set_spot) | sources: docs/GAPS.md:171 | size: ? | blocker: -
- AI on the GPU: speed on a real GPU unmeasured; f32 only (no fp16); a layer larger than the GPU's buffer binding runs on CPU; 8-bit search models stay on CPU; GPU AI memory not shared with the preview budget | sources: docs/GAPS.md:172 (crates/amb-ai/src/net/, crates/amb-gpu/src/nn.rs) | size: ? | blocker: owner's Mac timing (docs/WHATS-NEXT.md:10)
- AI kernels: each thread reads its own rows and columns, no shared workgroup tiles (those crashed the Windows-on-ARM CI's software GPU); shared tiles untried | sources: docs/GAPS.md:293 (crates/amb-gpu/src/shaders/nn.wgsl, crates/amb-gpu/src/nn.rs) | size: ? | blocker: -
- Enhance: Raw Details and Super Resolution not built (AI Denoise built as our own, RawNIND) | sources: docs/GAPS.md:185, docs/SCOPE.md:329 | size: ? | blocker: -
- Phase 4 AI: Lens Blur (Depth Anything V2-S), Super Resolution, people parts, depth-range masks | sources: docs/GAPS.md:173, docs/SCOPE.md:329 | size: ? | blocker: -
- Permissive-only AI model manifest with a CI gate not built; non-commercial models (RMBG, Depth Anything Base+) must not be bundled | sources: docs/GAPS.md:174, docs/SCOPE.md:152, docs/SCOPE.md:211 | size: ? | blocker: -
- Smart search: indexes photos at first search (about 1 s a photo); searches only the photos the library has open; user's own search models can't be added | sources: docs/GAPS.md:111 (crates/amb-ai/src/search.rs, crates/amb-library/src/search.rs) | size: ? | blocker: -
- Face grouping (optional, local, off by default) | sources: docs/NEXT.md:123 | size: ? | blocker: - | DISPUTED (Contradictions 11)
- Vision-model refinement loop, and matching with masks (sky, subject) | sources: docs/GAPS.md:280, docs/design/ai-control.md:45, 66-67, docs/SCOPE.md:324 | size: ? | blocker: -

## Export

- AVIF: 8-bit, no ICC embedded (sRGB, Display P3, Rec. 2020 named by CICP); Adobe RGB and ProPhoto refused | sources: docs/GAPS.md:258, docs/ARCHITECTURE.md:58 | size: ? | blocker: -
- WebP: lossless only (lossy needs libwebp) | sources: docs/GAPS.md:259, docs/ARCHITECTURE.md:57 | size: ? | blocker: -
- JPEG XL: lossless only; sRGB only, no ICC profile | sources: docs/GAPS.md:260 | size: ? | blocker: -
- No HDR AVIF, JXL or gain-map export | sources: docs/GAPS.md:261, docs/SCOPE.md:330 | size: ? | blocker: -
- No DNG export of the developed image | sources: docs/GAPS.md:264, docs/ARCHITECTURE.md:73 | size: ? | blocker: -
- Integer outputs hard-clip to the gamut; no gamut mapping | sources: docs/GAPS.md:266 | size: ? | blocker: -
- Output sharpening (screen, matte, glossy) is Model v1 with guessed radius and strength | sources: docs/GAPS.md:267 (crates/amb-export/src/geometry.rs:176-185) | size: ? | blocker: -
- Watermarks: one bundled font; cannot be shared as presets through the hub | sources: docs/GAPS.md:265 | size: ? | blocker: -
- Exported OpenEXR files carry no metadata | sources: docs/GAPS.md:86 | size: ? | blocker: -
- Collage maker: no edge-dragging of grid cells; no own undo; no snapping when arranging freely; no per-cell edits (each photo's own edit); batch shapes come from the camera preview (crop ignored); border captions one line; editor Frame preview drawn by browser (1-2 px off) | sources: docs/GAPS.md:86, docs/design/collage-and-borders.md | size: ? | blocker: -
- Collage for every photo shown: one layout over all of them, as pages | sources: docs/GAPS.md:31, docs/WHATS-NEXT.md:25 | size: S | blocker: -
- Contact sheet as a straight proof (film strips) | sources: docs/NEXT.md:150 | size: M | blocker: - | DISPUTED (Contradictions 3)
- Print layouts and paper sizes; contact sheets as PDF; printing from the app (none yet) | sources: docs/NEXT.md:151, docs/GAPS.md:46, docs/GAPS.md:112 | size: M | blocker: -
- Printer and paper soft proof, applied once to the composed page | sources: docs/NEXT.md:152, docs/GAPS.md:115 | size: M | blocker: printer and paper ICC profiles
- Flat 16-bit master | sources: docs/NEXT.md:153 | size: S | blocker: -
- Static web gallery export and slideshow export (video or web page; music) | sources: docs/NEXT.md:127 | size: ? | blocker: - | DISPUTED (Contradictions 19)
- GPU exports: speed on a real GPU unmeasured; background export queue (post-export hooks) not built; after export only "show in folder" or open in the Edit-in app | sources: docs/GAPS.md:96, docs/SCOPE.md:308 | size: ? | blocker: -
- Edit in another app: no "edit the original" choice for JPEGs; the TIFF is not stacked with its photo | sources: docs/GAPS.md:96 | size: ? | blocker: -
- Convert raws: camera JPEG used only at 2000 px or more on the long side (older Sony bodies matched instead); Auto and Like solve on a 256 px copy; GPU speed unmeasured; Canon and Fujifilm untried | sources: docs/GAPS.md:110 (crates/amb-ops/src/convert.rs) | size: ? | blocker: - | DISPUTED (Contradictions 6)

- Shareable before/after page (one HTML file with a slider) | sources: docs/GAPS.md:29, docs/NEXT.md:29 | size: S | blocker: - | DISPUTED (Contradictions 1)
## Plugins & hub

- Plugins: free-form plugin panels and dialogs (effects get controls from their manifest only); a JIT for pixel effects | sources: docs/NEXT.md:84, docs/GAPS.md:334 | size: ? | blocker: -
- Data plugins (presets, LUTs, profiles) not built; pipeline effects are built | sources: docs/SCOPE.md:379 | size: ? | blocker: -
- CI does not rebuild hub plugins from source (they are shared as pull requests to plugins/) | sources: docs/GAPS.md:334 | size: ? | blocker: -
- Recipes are per plugin only; not shared through the hub | sources: docs/GAPS.md:334 | size: ? | blocker: -
- Windows x64: a plugin's call into the app crashes the wasmi interpreter; plugins there may not import amb.log (not reported upstream yet) | sources: docs/GAPS.md:334 | size: ? | blocker: -
- Custom interfaces: no way for one to add its own Rust commands; tried on Linux only | sources: docs/GAPS.md:85 (crates/amb-gui/src/interface.rs, docs/ai/custom-interface.md) | size: ? | blocker: -
- Hub: no ratings or screenshots; very large layouts must be added by hand | sources: docs/GAPS.md:86 | size: ? | blocker: -
- Hub built-ins are a copy in the app (tools/sync-hub.py): a change to a built-in needs a sync and a new release | sources: docs/GAPS.md:86 | size: ? | blocker: -
- Shareable Trippy looks: a Trippy edit saved as a preset with its effects, shared to the hub | sources: docs/GAPS.md:32, docs/WHATS-NEXT.md:26 | size: S | blocker: -

## CLI/MCP/agents

- Protocol has no list_presets command; preview, stats, match and export are MCP tools and CLI commands, not protocol commands | sources: docs/GAPS.md:277, docs/design/ai-control.md:16-17 | size: ? | blocker: -
- --json missing on several CLI commands (render, lens, sweep, calibration) | sources: docs/GAPS.md:278, docs/design/ai-control.md:57-59 | size: ? | blocker: -
- MCP: no content-aware masks for the AI yet; editor follows sidecar changes from MCP only while a photo is open | sources: docs/GAPS.md:275, docs/ARCHITECTURE.md:88 | size: ? | blocker: -
- Agents drive the whole UI (menus, tools, brush strokes), with undo | sources: docs/GAPS.md:47 | size: M | blocker: -
- Assistant panel: cannot place masks, crop or spots; does not stream answers; keys need a system keychain | sources: docs/GAPS.md:279, docs/ai/assistant.md | size: ? | blocker: -
- Assistant safety not built: a pending diff to Accept before applying; cost limits in money | sources: docs/GAPS.md:281, docs/design/ai-control.md:49-54 | size: ? | blocker: -
- Learn-my-style from edit history; batch "apply my look to this shoot" | sources: docs/GAPS.md:282, docs/design/ai-control.md:68 | size: ? | blocker: -
- AI variant of Auto: "Auto in the style of ..." via the amb match solver | sources: docs/design/future-ideas.md:22 | size: ? | blocker: -
- amb match v1 limits: no tone curve, texture or clarity; no worded explanation; no Amount for the result; no batch apply with per-photo adaptation | sources: docs/GAPS.md:276, docs/design/ai-control.md:30-47, 63 | size: ? | blocker: -
- Token budget: core card about 360 tokens (plan was 300); budget checked with a 4-characters-a-token estimate; provider token-count check in CI not done | sources: docs/design/future-ideas.md:101, 108, docs/GAPS.md:274 | size: ? | blocker: -
- Per-group reference files on disk not made (amb ref GROUP prints them) | sources: docs/GAPS.md:274, docs/design/future-ideas.md:102 | size: ? | blocker: -
- Feedback button: sending straight from the app (relay that files the issue) not built; `amb feedback` CLI command planned | sources: docs/GAPS.md:94, docs/design/future-ideas.md:55 | size: M | blocker: -

## Docs

- Decisions record (docs/decisions/ ADRs) missing; decided items unrecorded (AI runtime tract plus own GPU kernels; SAM 2.1 tiny not via ort; Tauri 2; .lrcat reading); fp16 vs fp32 and the GPU viewport still open | sources: docs/GAPS.md:73, docs/SCOPE.md:281 | size: ? | blocker: -
- Governance not written: contributor AI-use policy, no-rug-pull charter, ADR on data-safety invariants, usability-test plan (CONTRIBUTING.md with DCO exists) | sources: docs/GAPS.md:74, docs/SCOPE.md:280, docs/SCOPE.md:400 | size: ? | blocker: -
- README screenshots again: library (grid and Insights) and a Trippy edit | sources: docs/WHATS-NEXT.md:24, docs/GAPS.md:33 | size: S | blocker: owner's sample photos (docs/WHATS-NEXT.md:7)
- Agent measurement plan: fresh agents on fixed tasks with and without recipes and maps; only rounds A1 and B1 listed as done | sources: docs/NEXT.md:164 (docs/design/agent-navigation.md) | size: ? | blocker: -
- Map for agents: "largely done"; the rest is not named in the doc | sources: docs/NEXT.md:165 | size: ? | blocker: -
- Trademark notices ("Lightroom is a trademark of Adobe Inc.") kept wherever Lightroom or Fujifilm is named | sources: docs/GAPS.md:63 | size: ? | blocker: -
- Patent risk "low, not zero" for local-Laplacian, dark-channel and Adobe dehaze/local-contrast methods; Microsoft dark-channel patent only "reportedly lapsed" | sources: docs/GAPS.md:66, docs/research/05-processing-algorithms.md §6, §18.5, docs/research/02-preset-and-profile-formats.md:308 | size: ? | blocker: counsel review (docs/GAPS.md:65)
- Docs: the copy check (keep notes on Adobe sources in our own words) to re-run before each release | sources: docs/GAPS.md:358 | size: ? | blocker: -

## Infra/CI/release

- v0.1.0: tag v0.1.0, check the draft release, publish | sources: docs/NEXT.md:68, docs/WHATS-NEXT.md:8 | size: ? | blocker: owner's go after the Mac and Windows tries
- README "no installers yet" becomes a link to the release, after publishing | sources: docs/NEXT.md:67 | size: ? | blocker: publish first
- Packaging and signing left: run the Release workflow on GitHub; Windows signing (SignPath); Apple signing secrets; Flatpak; MSIX; universal DMG; auto-update | sources: docs/NEXT.md:101, docs/NEXT.md:113, docs/GAPS.md:331, docs/SCOPE.md:304 | size: ? | blocker: owner's SignPath and Apple signing secrets | DISPUTED (Contradictions 13)
- Platforms: Windows, macOS, Linux (Wayland and X11), Windows ARM64 and Apple Silicon must all be first-class; editor tried mostly on Linux; UI tests on Linux only | sources: docs/GAPS.md:332, docs/ARCHITECTURE.md:13, docs/SCOPE.md:50 | size: ? | blocker: Mac and Windows machines
- GUI tests: more scripts left (masks, crop, library); macOS and Windows webviews not tested | sources: docs/NEXT.md:42, docs/GAPS.md:90 | size: ? | blocker: macOS and Windows machines | DISPUTED (Contradictions 12)
- Benchmark CI and performance budgets (slider latency, import, export, mask count, catalogue size) not written; CI runs fmt, clippy and tests only | sources: docs/GAPS.md:291, docs/SCOPE.md:59-61 | size: ? | blocker: -
- Fidelity metrics and CI not built (median and p95 dE00 per control, whole-preset scores, dashboard) | sources: docs/GAPS.md:305, docs/SCOPE.md:262, docs/SCOPE.md:291 | size: ? | blocker: Lightroom renders (fitting)
- Preset fidelity suite and parser fuzz targets not built; coverage-guided fuzzing (cargo-fuzz) not done; ICC and raw decoding not fuzzed | sources: docs/GAPS.md:75, docs/GAPS.md:306, docs/SCOPE.md:349, docs/SCOPE.md:378 | size: ? | blocker: -
- Hygiene: rust-toolchain.toml pin not done (needs the release workflow's cross targets) | sources: docs/GAPS.md:75, docs/SCOPE.md:380 | size: ? | blocker: -
- Usage insights (opt-in, local: panels used, not-rendered settings, timings, crashes) and "share usage report" not built | sources: docs/GAPS.md:54, docs/GAPS.md:94, docs/design/future-ideas.md:71-78 | size: M | blocker: -
- Feedback loop: a feedback label set and a monthly top-requests triage that updates GAPS.md | sources: docs/design/future-ideas.md:79 | size: ? | blocker: -
- Web build or self-hosted browser library (reach without installing) | sources: docs/GAPS.md:57 | size: L | blocker: -
- Mobile: out of scope; an Android app may follow (iOS App Store effectively impossible under GPL) | sources: docs/GAPS.md:335, docs/SCOPE.md:331 | size: ? | blocker: -

## Code cleanup

- Files past the size budget pinned at their current size (OVER_BUDGET in crates/amb-cli/tests/code_size.rs); split each when it needs to grow | sources: docs/GAPS.md:343, docs/cleanup.md:542 | size: ? | blocker: -
- UI scripts share one global scope; load order in index.html matters; a script may use only names loaded before it | sources: docs/GAPS.md:344 | size: ? | blocker: -
- Colour NR keeps an unused color_low_res_radius field, only so old calibration files still load | sources: docs/GAPS.md:182 (crates/amb-engine/src/detail/mod.rs) | size: ? | blocker: -
- ProfileHint.name (LensProfileName) is read from the edit but unused | sources: docs/GAPS.md:200 (crates/amb-lens/src/lib.rs:56-59, 76-83) | size: ? | blocker: -
- Cleanup pass remainder: one clear home per concern, consistent names, dead code out (the code map is largely done) | sources: docs/NEXT.md:166 | size: ? | blocker: -

## Needs the user

Blocked on the owner's files, the Lightroom trial, decisions, or hardware. Items marked DISPUTED are also listed above.

- Run the Lightroom Classic measurement month: subscription not yet run; the kit must be ready first | sources: docs/GAPS.md:301, docs/NEXT.md:105, docs/lightroom-trial-plan.md:5 | size: ? | blocker: Lightroom Classic subscription (owner)
- Build the kit before subscribing (make-kit.sh, ~260 MB of CC0 raws); day-1 setup; run 0-check by hand and with the plug-in | sources: docs/lightroom-trial-plan.md:27-41, 95-117 | size: ? | blocker: owner's Lightroom install and trial
- Run the plug-in in Lightroom (only tested against a stand-in) | sources: docs/GAPS.md:304, docs/lightroom-trial-plan.md:172 | size: ? | blocker: Lightroom install
- Shoot own photos: portrait, grey card or wall, flash portrait with red eyes, night street, backlit scene, tilted building, brick wall with each lens, brackets, a raw with a negative sidecar | sources: docs/lightroom-trial-plan.md:67-81, docs/GAPS.md:302 | size: ? | blocker: owner's camera and shots
- Trial batches 1a, 1b, 2, 3, 4 (about a day of exports) | sources: docs/lightroom-trial-plan.md:119-134 | size: ? | blocker: Lightroom subscription
- Interaction sidecars (Highlights x Whites, Shadows x Blacks, Contrast x Exposure, Vibrance x Saturation, Texture x Clarity) not in the sweep plan | sources: docs/GAPS.md:303, docs/lightroom-trial-plan.md:14, 49, 134 | size: ? | blocker: - (to add before the month)
- Slider sweeps for all global controls (74 sweeps, about 680 exports per input image) | sources: docs/GAPS.md:318, docs/lightroom-trial-plan.md:37-47 | size: ? | blocker: Lightroom trial
- Interaction pairs on synthetic and real raws | sources: docs/GAPS.md:319, docs/lightroom-trial-plan.md:48-49 | size: ? | blocker: Lightroom trial
- Read every slider's range, default, step; double-click reset (by hand) | sources: docs/GAPS.md:315, docs/GAPS.md:236, docs/lightroom-trial-plan.md:140 | size: ? | blocker: Lightroom trial
- Exact Lightroom Classic and Camera Raw versions; current process version | sources: docs/GAPS.md:316, docs/lightroom-trial-plan.md:97 | size: ? | blocker: Lightroom trial
- What Auto and Lightroom's defaults write to XMP | sources: docs/GAPS.md:317, docs/lightroom-trial-plan.md:141 | size: ? | blocker: Lightroom trial
- Does Lightroom keep our amb: tags when it saves a sidecar? | sources: docs/GAPS.md:221, docs/lightroom-trial-plan.md:154 | size: ? | blocker: Lightroom trial (plan 4.11)
- Apply every preset you own to the ten raws and export: end-to-end preset test; assemble the private preset corpus | sources: docs/GAPS.md:238, docs/lightroom-trial-plan.md:155, docs/SCOPE.md:279, docs/NEXT.md:16 | size: ? | blocker: owner's preset collections (not committed)
- Trial finish: re-run failures, check counts against manifests, copy lr-out to two drives, cancel subscription | sources: docs/lightroom-trial-plan.md:157-162 | size: ? | blocker: Lightroom trial
- Fit the model constants to the trial renders (only fitted numbers go into the repo) | sources: docs/lightroom-trial-plan.md:164, docs/GAPS.md:300 | size: ? | blocker: trial output (lr-out)
- Decide: cloud Lightroom API (lr.adobe.io) or computer use for part 4 | sources: docs/lightroom-trial-plan.md:176-184 | size: ? | blocker: decision; Adobe developer key and approval if cloud API
- Sample raw: replace the stand-in with the owner's photo (DNG preferred, under 30 MB, CC0 or CC BY; spec lists the sliders it must show) | sources: docs/NEXT.md:35, docs/NEXT.md:47-53, docs/NEXT.md:70, docs/WHATS-NEXT.md:11 | size: ? | blocker: owner's photo (after v0.1.0)
- Sample photos from a Fujifilm (raws or JPEGs) for README screenshots and the sample photo | sources: docs/WHATS-NEXT.md:7 | size: ? | blocker: waiting on the owner
- Try the editor on a Mac: menu bar, library, custom interface, Connect an AI app, export | sources: docs/NEXT.md:63, docs/WHATS-NEXT.md:8, docs/NEXT.md:15 | size: ? | blocker: owner's Mac
- Try the editor once on Windows (same list; terminal opens with cmd) | sources: docs/NEXT.md:64, docs/WHATS-NEXT.md:8, docs/NEXT.md:15 | size: ? | blocker: owner's Windows machine
- Say go for v0.1.0 after the Mac and Windows tries | sources: docs/WHATS-NEXT.md:8 | size: ? | blocker: waiting on the owner
- Speed on the owner's Mac: export a 24 MP raw with and without AMB_GPU=off; Select object first click and AI Remove with and without AMB_AI_GPU=off; release preview and 1:1 times (AMB_TIMING=1) | sources: docs/WHATS-NEXT.md:10, docs/NEXT.md:101, docs/NEXT.md:112 | size: ? | blocker: owner's Mac
- Auto Density and Auto Grade on a real roll: try both switches; if right, they become Auto's default | sources: docs/WHATS-NEXT.md:9, docs/NEXT.md:141 | size: M | blocker: waiting on the owner (a roll of negatives)
- Run `amb preset census` on your preset folders and share the output (names and counts only) | sources: docs/NEXT.md:16 | size: ? | blocker: owner's preset folders
- Film simulation references: raws plus in-camera re-renders with each film simulation | sources: docs/NEXT.md:18, docs/design/film-look-measurement.md, docs/GAPS.md:334 | size: ? | blocker: owner's Fujifilm camera
- Facts for Look and RGBTable: the 85-character base alphabet used by the DNG SDK for Table_<md5>, and the crs: property names holding table fingerprints | sources: docs/NEXT.md:19, docs/GAPS.md:231 | size: ? | blocker: facts from the user or community (facts only, no table data)
- Real ColorChecker capture for colour fitting (the synthetic chart is approximate) | sources: docs/GAPS.md:307 | size: ? | blocker: a real chart capture
- Counsel review: purchased-preset seller licences (some forbid conversion) before 1.0; one-hour IP-lawyer review before 1.0 or donations at scale | sources: docs/GAPS.md:64, docs/GAPS.md:65, docs/SCOPE.md:217, docs/legal.md:3 | size: ? | blocker: counsel
- DNG SDK licence vs GPL not reviewed by counsel; whether the SDK's ACR3 default curve table may be redistributed | sources: docs/GAPS.md:67, docs/research/02-preset-and-profile-formats.md, docs/research/05-processing-algorithms.md §20 | size: ? | blocker: counsel
- Trademark searches (TMview, WIPO, USPTO/EUIPO) for Ambrotype; fallback name ready | sources: docs/SCOPE.md:220, docs/SCOPE.md:224-225 | size: ? | blocker: owner or counsel
- Funding model: donations plus a fiscal host (Open Collective); NLnet round closed 2026-06-01; Sovereign Tech Fund excludes end-user apps | sources: docs/GAPS.md:68, docs/SCOPE.md:381 | size: ? | blocker: owner decision
- Fidelity target that defines "compatible" (proposal: median dE00 < 2, p95 < 5 over the top-20 tags) | sources: docs/GAPS.md:69, docs/SCOPE.md:294, docs/SCOPE.md:428 | size: ? | blocker: owner decision
- Who holds a licensed Lightroom Classic install for the calibration sweeps; where reference renders are stored | sources: docs/GAPS.md:70, docs/SCOPE.md:429 | size: ? | blocker: owner decision
- Contact RapidRAW's maintainer about a shared Adobe-compatible engine crate | sources: docs/GAPS.md:71, docs/SCOPE.md:430 | size: ? | blocker: owner decision
- UI shell: own shell vs RapidRAW-fork UI (Phase 2 gate, settled by a viewport spike and the maintainer conversation) | sources: docs/GAPS.md:72, docs/SCOPE.md:297, docs/SCOPE.md:427 | size: ? | blocker: owner decision | DISPUTED (Contradictions 14)
- Usability test with Lightroom switchers (Phase 2 exit; test plan not written) | sources: docs/SCOPE.md:313, docs/GAPS.md:74 | size: ? | blocker: testers and owner
- Own the amb: namespace domain (uses a domain not owned yet; must stay stable once sidecars exist) | sources: docs/GAPS.md:222, docs/design/film-negatives.md:5 | size: ? | blocker: owner
- Relicensing: decide before accepting outside code or porting GPL code | sources: docs/SCOPE.md:195, docs/SCOPE.md:426 | size: ? | blocker: owner decision | DISPUTED (Contradictions 15)

## Contradictions

1. Shareable before/after page: docs/GAPS.md:29 open (quick-wins row 3) vs docs/NEXT.md:29 done ("File → Share Before and After…, `amb share`").
2. Blank-canvas layout editor: docs/GAPS.md:34 open vs docs/NEXT.md:29 done ("Appearance → Start from scratch…").
3. Contact sheet film strips with edge print: docs/GAPS.md:46 open vs docs/NEXT.md:29 done (holes, frame numbers, edge print). Also docs/NEXT.md:150 lists "a contact sheet as film strips, with a straight proof" as open.
4. Place search by name: docs/WHATS-NEXT.md:44 done (Pick… by address or name) vs docs/NEXT.md:120 and docs/GAPS.md:105 open ("no search for a place by name"); docs/GAPS.md:37 says "the place search exists now".
5. Zoom in library: docs/WHATS-NEXT.md:47 done ("the same in the library's viewer") vs docs/GAPS.md:86 ("loupe and compare don't zoom").
6. Canon and Fujifilm Convert raws: docs/WHATS-NEXT.md:96 done 2026-11-05 vs docs/GAPS.md:110 ("Canon and Fujifilm untried").
7. CA removal without a lens profile: docs/GAPS.md:51 open idea (row 20) vs docs/WHATS-NEXT.md:147 (Remove Chromatic Aberration "already measured CA without a lens profile", done 2026-11-02).
8. Teach by showing: docs/WHATS-NEXT.md:30-32 list tips while waiting, tips in boxes, and "one answer leads to more" as ideas, but docs/WHATS-NEXT.md:29 says done 2026-11-05 (tips while waiting, search box, preset filter; each How to lists related features), docs/WHATS-NEXT.md:98 and docs/NEXT.md:90 say the same.
9. Lightroom switchers' lesson: docs/GAPS.md:52 "still to do" vs docs/design/future-ideas.md:39 (lesson "Coming from Lightroom" built).
10. Culling help: docs/NEXT.md:122 lists sharpness, closed eyes and exposure as open vs docs/WHATS-NEXT.md:86 (Best in each burst ranks by focus and open eyes, done 2026-11-05) and docs/GAPS.md:104 (eyes and sharpness built; no exposure score).
11. Smart search and face grouping: docs/NEXT.md:29 says smart search done; docs/NEXT.md:123 (parent item "Search by what's in the photo", with optional face grouping) has no done mark. Weak: the open part is face grouping.
12. UI tests, library: docs/NEXT.md:42 says more scripts left including library vs docs/GAPS.md:90 (browse.js covers the library; organise.js covers file care).
13. Installers on GitHub: docs/NEXT.md:61 ("green on every platform since 2bf5a58") and docs/GAPS.md:331 ("ran green on GitHub (2026-10-22, by hand)") vs docs/NEXT.md:101 ("to run on GitHub, then signing") and docs/NEXT.md:113 ("Left: run it on GitHub").
14. UI shell: docs/GAPS.md:72 open vs docs/GAPS.md:73 (the UI is Tauri 2, decided) vs docs/SCOPE.md:424 ("Since decided: 2 (our own engine with a Tauri editor)").
15. Licence: docs/SCOPE.md:424 says item 1 decided (GPL-3.0-or-later, 2026-10-08) vs docs/SCOPE.md:426 and docs/SCOPE.md:195 ("Decide before accepting outside contributions or porting GPL code").
16. .cube LUTs: docs/GAPS.md:28 and docs/WHATS-NEXT.md:22 list "read and apply .cube LUTs as looks" as an open idea vs docs/GAPS.md:234 (".cube LUTs work (amb:LUT, Basic → Profile → LUT)").
17. Data safety: docs/GAPS.md:109 says XMP sidecars as source of truth and a rebuildable catalogue are not implemented vs docs/NEXT.md:117 and docs/GAPS.md:106 (sidecars are the truth; the catalogue is a cache).
18. Look matching: docs/design/future-ideas.md:83 says "Look matching is still to come" vs docs/NEXT.md:110 and docs/GAPS.md:276 (amb match built 2026-10-12).
19. Web gallery: docs/NEXT.md:127 plans a static web gallery export vs docs/SCOPE.md:76 (web gallery modules a non-goal for v1).
20. Generative relight: docs/GAPS.md:56 open idea vs docs/SCOPE.md:130 ("Sky replacement and generative relight: Not planned").
21. Preset Amount: docs/GAPS.md:228 says non-numeric settings (including curves) are taken whole when Amount > 0, and notes that SCOPE says curves and LUTs blend toward identity; docs/SCOPE.md:246 says they blend toward identity.

## Code markers

Grep for TODO|FIXME|XXX|HACK|todo!(|unimplemented!( in crates, plugins, tools (target and *.wasm skipped): one hit, a false positive.

- /home/user/ambrotype/crates/amb-library/src/import.rs:40: `/// Look in subfolders of the source too (a card's `DCIM/100XXXXX`).` (false positive: "XXXXX" in a folder example, not a marker)
- plugins/: no hits
- tools/: no hits
