# Plan for the one-month Lightroom subscription

A month of Lightroom Classic is our only chance to gather **ground truth**: reference renders and exact UI facts. The renders stay usable forever after the subscription ends, so the goal is to batch-capture everything the fidelity harness will ever need. Don't explore by hand.

**Rule 1: don't start the month until the "sweep kit" below is ready.** Each day of the subscription should be spent mostly waiting on exports, not building tooling.

## Before subscribing (we build this first)

| Item | What it is | Status |
|---|---|---|
| Synthetic test DNGs | Linear DNGs: a grey ramp (14 stops), edge/frequency patterns, a hue × saturation grid and an approximate ColorChecker. The camera space is defined as linear ProPhoto, and a linear tone curve is embedded | **Built**: `amb testimage`, `amb sweep generate --synthetic ramp,hue,edges,colorchecker`. They decode correctly with LibRaw. Still to do: check them with Adobe's `dng_validate` and do a 7-day-trial import test |
| CC0 real raws | About 40–100 raws from raw.pixls.us and similar: skin, sky, foliage, night, backlit, several camera brands | To collect |
| Sweep sidecars | One `.xmp` sidecar per test file and slider value. Lightroom reads them on import, but only for proprietary raws (CR2/NEF/ARW...). For DNGs the settings are embedded in the file instead | **Built**: `amb sweep generate --out <dir> <raws>`, with the plan shown by `amb sweep plan` |
| HDR merge check | One set of real brackets (3 raws, ±2 EV, one handheld): merge them with `amb merge hdr`, then in Lightroom import our DNG and also merge the same brackets with Photo Merge > HDR (Auto Tone off). Compare both at default settings and with the same sliders ([design/hdr-merge.md](design/hdr-merge.md)) | To shoot |
| Interaction sidecars | Known coupled pairs: Highlights × Whites, Shadows × Blacks, Contrast × Exposure, Vibrance × Saturation, Texture × Clarity | To build (add to the sweep plan) |
| Preset corpus | Your purchased presets plus free ones, kept in a private folder and never committed | You |
| Run sheet | A checklist and folder layout so every export is labelled with LR version, process version and settings | To write |

## Storage setup
- Keep Lightroom's **catalog and Camera Raw cache on the fastest internal drive**.
- With two external drives, use one for inputs (the sweep kit) and one for exports, so reads and writes don't contend. Cheap DRAM-less SSDs slow down on long sustained writes; that's fine for exports, just slower. Use USB 3 ports.
- Format the drives exFAT if they'll move between Windows, macOS and Linux; otherwise the system's own format (APFS on a Mac, NTFS on Windows).
- Budget: about 30–60 GB of renders with the storage guidance below. Keep a copy of the final archive on two drives.
- **On a Mac:** Lightroom Classic runs natively on Apple Silicon. Keep the Mac plugged in and awake during long exports (`caffeinate -dis`). See [desktop-setup.md](desktop-setup.md#macos-on-apple-silicon-m1-and-later-with-an-external-ssd).

## Plan type
- Pick the **monthly, no-commitment** plan. Adobe's "annual, billed monthly" plan charges a fee if you cancel early. Check the current terms when you subscribe.
- Adobe's 7-day free trial can be used first for a quick check that the kit imports correctly.

## During the month

**Week 1: setup and UI facts**
1. Record the exact Lightroom Classic and Camera Raw versions and the current process version.
2. Set neutral preferences: no auto-tone, the default profile noted, no import presets. Export settings: 16-bit TIFF, ProPhoto RGB, no sharpening, no resizing, metadata included.
3. Capture **every slider's range, default and step size**, plus double-click reset behaviour. Use screenshots, recorded in a table. Adobe's docs leave most of these out.
4. Save Lightroom's own default and Auto settings as XMP for a few test images. That shows what "Auto" writes.

**Weeks 1–2: slider sweeps (the core harness data)**

The kit is generated with `amb sweep generate --out <kit-dir> [--groups tone,presence,...] <raws>` (see `amb sweep plan`). The full plan is 74 sweeps and about 680 exports per input image.

**Storage budget.** A full-resolution 16-bit TIFF of a 24 MP image is about 150 MB, so the full plan on 10 raws would be about 1 TB. To keep it manageable:
- Export **global** controls at 2048 px on the long edge: tone, white balance, HSL, colour grading, calibration and vignette.
- Export **spatial** controls at full resolution but on a 1024 px crop set in the baseline: Texture, Clarity, Dehaze, sharpening, NR and grain.
- Export **lens** sweeps (group `lens`) at full resolution and **uncropped**: distortion, the blank areas and Constrain Crop need the whole frame. Run the profile sweeps (`LensProfileDistortionScale`, `LensProfileVignettingScale`) only on raws whose lens Lightroom recognises. Note which profile it picked (Lens Corrections → Profile), and whether "Remove Chromatic Aberration" alone changes a high-contrast edge shot (answers whether profile CA data is used).
- That brings the total to about 30–60 GB.

5. Import the synthetic DNGs with their sweep sidecars. Batch-export everything; the volume is about 40 controls × 11 values × the targets.
6. Run the same sweeps on 10 real raws.
7. Run the interaction pairs.
8. **Foreign XMP tags:** put a sidecar with `amb:` settings (e.g. from `amb negative analyze`) next to a raw. Import it, change a slider, run Metadata → Save Metadata to File, and check whether the `amb:` attributes survive.
9. **Preset Amount:** apply 5–10 presets at Amount 0/25/50/75/100/150/200 and export. This measures Adobe's undocumented Amount maths.

**Week 3: profiles, presets, local tools**
10. **Profiles:** render the ColorChecker and real raws with Adobe Color, Adobe Standard, Adobe Neutral, Adobe Landscape, Adobe Portrait, Adobe Vivid, Camera Matching and a few creative profiles at several Amounts. This is how we fit our replacement base profiles without shipping Adobe's files. Also the Calibration panel on the ColorChecker (each primary's hue and saturation at −100, −50, +50, +100, and Shadow Tint; [design/calibration.md](design/calibration.md)), and one photo with two or three Point Color swatches, keeping the sidecar: it shows how `PointColors` is stored, which Point Color needs before we build it.
11. **Preset corpus:** render every preset you own over the real-raw set. This becomes the end-to-end regression suite.
12. **Masks:** linear and radial gradients with known feather values on a flat grey DNG (to measure the falloff curves); luminance and colour range masks on ramps and wheels; each local slider at +50 next to its global one; a radial turned +30° and a short brush stroke with an erased part, saved to the sidecar, to settle the angle sign and how erasing is stored ([design/masks.md](design/masks.md) open questions). Also fix one red eye and one pet eye and keep the sidecar: it shows how `RedEyeInfo` is stored, which red eye needs before we build it.
13. **Effects and detail:** vignette in every style, grain, sharpening (on edge patterns) and noise reduction (on flat ISO-noise frames).

**Week 4: lens, transform, catch-up**
14. Lens profile on/off and CA/defringe on grid shots, for the lenses you own.
15. Upright modes on architecture shots, saving each sidecar (do `UprightTransform_0..5` hold the solved transforms?). Each Transform slider at −100, −50, +50 and +100 on a grid chart, to settle its sign and strength ([design/transform.md](design/transform.md) open questions).
16. Re-run anything that failed. Verify the archive is complete, then cancel.

## Automating the measurements (options, not decided)

Setting hundreds of slider values by hand is the slowest part of the month. Two ways to script it, both black-box (Lightroom renders our inputs; no Adobe code or files enter the repo):

1. **A Lightroom Classic plug-in** written by us with the Classic SDK (Lua, `LrDevelopController`). It can set any develop or local slider, create and edit masks, read slider ranges (`getRange`), step through photos, and export. It runs on the owner's trial, offline, with their own files. The SDK documentation is copyrighted ("for informational use"): read it for names and calls, copy none of it.
2. **The cloud Lightroom API** (`lr.adobe.io`, published OpenAPI spec). Upload a raw once, put our sweep sidecar on it as its develop settings (`PUT …/xmp/develop`), ask for a rendition or export (2560 px, 2048/4096 or full size, JPEG), download, put the next sidecar, repeat. It needs an Adobe developer API key (partner access may need Adobe's approval), a signed-in user with a subscription or trial, and uploads the test raws to Adobe (fine for synthetic targets and CC0 raws). Output is 8-bit JPEG from the cloud engine, whose version may differ from Classic's. Check Adobe's developer terms before relying on it.

Either way the inputs are the sidecars `amb sweep generate` already writes, and the mask questions in [design/masks.md](design/masks.md) become scripted sweeps on a flat grey DNG.

## Storage and legal
- Keep all renders in a **private** storage location, not in the public repo. They're derived from your licensed use of Lightroom. Only the CC0 inputs and our measured numbers (fitted tables) are committed.
- **Black-box measurement only.** That means no decompiling, and no copying Adobe profile or lens files into the project. See [research/05 §18.5](research/05-processing-algorithms.md).
