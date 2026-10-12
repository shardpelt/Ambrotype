# Lens Corrections

Status: v1 implemented (`crates/amb-lens`, `crates/amb-engine/src/lens.rs`). Every constant is a first guess, to be fitted against Lightroom with the `lens` sweeps (docs/lightroom-trial-plan.md).

## What Lightroom's panel does, and what we render

| Lightroom control | XMP tag(s) | Ours |
|---|---|---|
| Enable Profile Corrections | `LensProfileEnable`, `LensProfileSetup`, `LensProfileName`, `LensProfileFilename`, `LensProfileDigest` | Profile from the user's own Adobe LCP files (preferred: Lightroom's own data), else the lensfun database. Distortion and vignetting. |
| Profile amounts: Distortion, Vignetting (0–200, default 100) | `LensProfileDistortionScale`, `LensProfileVignettingScale` | Distortion: the displacement is scaled linearly. Vignetting: the gain is raised to the power amount/100. |
| (legacy) profile CA amount | `LensProfileChromaticAberrationScale` | Scales the profile's CA displacement. |
| Remove Chromatic Aberration | `AutoLateralCA` | Estimated from the image's own edges (red/blue radial shift, δ(r) = a·r + b·r³). Falls back to the profile's CA model when the image has too few edges. |
| Manual Distortion (−100..100) | `LensManualDistortionAmount` | r′ = r·(1 + k r²)/(1 + k): corners fixed, edge midpoints move. Positive corrects barrel distortion, leaving curved blank areas at the edge midpoints. |
| Lens Vignetting Amount / Midpoint | `VignetteAmount`, `VignetteMidpoint` | Up to ±1.5 stops at the corners, rising smoothly from a start radius set by Midpoint. |
| Defringe: Purple/Green Amount (0–20) and Hue ranges (0–100) | `DefringePurple*`, `DefringeGreen*` | At high-contrast edges (OkLab lightness range in a window that widens with Amount), pull chroma towards grey for hues inside the range. |
| Constrain Crop | `CropConstrainToWarp` | Scale up until no blank area is left. |

Not yet (Transform and Upright are in [transform.md](transform.md)): fisheye profiles, local Defringe in masks, and built-in profiles embedded in raw files (see Next).


## Getting the lensfun database

`amb lens fetch` and the editor's Lens Corrections → **Lens database…** download lensfun's database from its own update server (`https://lensfun.github.io/db/`, `version_2.tar.bz2`, about 0.5 MB), as `lensfun-update-data` does, into `~/.local/share/lensfun/updates/version_2` with its `timestamp.txt`. That folder is searched by default (after `AMB_LENSFUN_DB`), and lensfun and darktable read it too. The editor loads it at once, and offers the download once when it opens a photo and finds no lens profiles at all. `--from FILE` installs a copy downloaded by hand. The data is CC BY-SA 3.0; we don't change it (`amb_library::lensdb`).

## Order in the pipeline

On camera RGB, before white balance and the camera matrix, in the sensor's orientation:

1. Vignetting gains: the profile's (×amount), then the manual vignetting.
2. Lateral CA model: auto-estimated, or the profile's.
3. **One** resampling pass (Catmull-Rom). It composes: fill zoom → manual distortion → profile distortion → per-channel CA offsets for red and blue.

Then, in the working space (scene-linear ProPhoto, after Exposure), Defringe. At the very end, the blank areas are painted `blank_fill` (white, as Lightroom shows them; to verify).

Scaling:
- Profile corrections always scale to exactly fill the frame. Adobe's release notes mention profile auto-scaling.
- Manual distortion keeps the profile's scale, so it can leave blanks, unless Constrain Crop is on. Constrain Crop then scales the whole chain to fill.

## Profile data: where it comes from (and the legal line)

Resolution order (`amb_lens::resolve`):

1. **The user's Adobe LCP files, matched by `LensProfileFilename`.** Lightroom and Camera Raw installs keep them in `…/Adobe/CameraRaw/LensProfiles/1.0`.
   - We read them in place, the same way RawTherapee does.
   - We never copy, bundle or upload them: they are Adobe content.
   - When present, this is the most faithful option, because it is Lightroom's own profile.
2. **LCP auto-match** by lens and camera names.
3. **lensfun database** (CC BY-SA 3.0), from a system install, `AMB_LENSFUN_DB`, or `--lens-db DIR`.
   - Lensfun calibrations are independent of Adobe's, so results differ a little from Lightroom: their distortion fits and vignetting measurements are not the same.
   - Shipping the database in our installers later is fine. It is data under its own licence: keep the attribution and the CC BY-SA terms for the data files.
4. Nothing: profile corrections are skipped, and `amb render` says so.

The models:
- **lensfun**: Hugin `poly3`/`poly5`/`ptlens` distortion, with r = 1 at half the short side of the calibration sensor. lensfun's centre normalisation is kept. TCA `linear`/`poly3`; vignetting `pa`, with r = 1 at the half diagonal. Calibrations made on another sensor size are rescaled by crop factor.
- **Adobe Camera Model** (LCP): radial k1–k3 plus tangential distortion, with coordinates normalised by the focal length in units of the image's longest side. CA is modelled as red and blue relative to green (scale factor plus polynomial). Vignetting is radial, with the same normalisation.

Interpolation:
- Distortion and CA: linear in focal length between the two nearest calibrations.
- Vignetting: across focal length and aperture (in stops), using the calibration at the nearest focus distance.

## Using it

- `amb lens info photo.nef` shows the lens metadata and which profile it gets, with the interpolated models.
- `amb lens match --camera-make … --lens-model … --focal …` does the same without a file.
- `amb render` resolves the profile whenever the settings enable profile corrections or Remove CA, and prints it.
- Sources: `--lens-db DIR` / `AMB_LENSFUN_DB` for the lensfun database (e.g. a checkout of lensfun's `data/db`, or `lensfun-update-data`); `--lcp-dir DIR` / `AMB_LCP_DIRS` for LCP folders. Adobe's default folders are searched automatically.

Matching (`amb-lens`):
- Lens names are compared by word overlap, after splitting glued EXIF spellings (`EF24-70mm`, `XF35mmF1.4`).
- The focal-length words must agree.
- Small penalties apply for a different aperture, a different lens maker or an incompatible mount.
- With several calibrations, the largest calibration crop factor at or below the camera's is chosen, as lensfun does.

Checked on real raws with the lensfun database:
- D750 + Tamron SP 35mm f/1.8 → `ptlens`.
- EOS R10 + Sigma 150-600 C → the crop-1.613 calibration.
- Both fill the frame and lift the wide-open corner falloff.

## Open questions for the Lightroom trial

- Does Lightroom fill blank areas with white? Does profile auto-scale also zoom *out* when a profile leaves room (barrel lenses)?
- Profile CA: with Remove CA off, does an LCP's CA model still apply? (`profile_tca_always`, default no.)
- Profile amount semantics: linear displacement for distortion and a gain power for vignetting are guesses.
- Manual Distortion strength, the Lens Vignetting stops and Midpoint mapping, and the Defringe hue-slider-to-hue mapping and edge width all need fitting.

## Next

- **Built-in profiles in raw files.**
  - Micro Four Thirds, Fujifilm X, Sony E and many compacts store correction data in maker notes. Lightroom applies it automatically ("Built-in Lens Profile Applied").
  - DNGs carry it as `WarpRectilinear` / `FixVignetteRadial` opcodes.
  - Reading these covers lenses that no database has.
- Transform and Upright: done, a homography in the same resampling pass ([transform.md](transform.md)).
- Fisheye profiles and projection changes.
- GPU resampling: the warp is about 1.5 s for 24 MP on 4 CPU cores.
