# Camera profiles

Status: implemented 2026-10-10 for DNG Camera Profiles (`.dcp`). Adobe's "Adobe Raw" profiles (Adobe Color, Landscape, Portrait, …), which are `.xmp` files with a look table, are not read.

Code: `crates/amb-engine/src/profile.rs` (reading, finding, applying), used by `render` in `crates/amb-engine/src/pipeline/mod.rs`. `amb profile list` and `amb render --dcp-dir`; the editor's Profile menu in the Basic panel.

## Where profiles come from

- **Read in place, never copied or shipped.** Adobe's profiles are Adobe's copyright; like Lens Profiles (LCP), we read the ones the user already has where Camera Raw or Lightroom installed them: `CameraProfiles` under `C:\ProgramData\Adobe\CameraRaw` and `%APPDATA%\Adobe\CameraRaw` on Windows, `/Library/Application Support/Adobe/CameraRaw` and the same under the home folder on macOS.
- **Your own:** profiles made with a ColorChecker (X-Rite, Calibrite) or dcamprof, in any folder named by `AMB_DCP_DIRS` (OS path-list syntax) or `--dcp-dir`.
- A profile counts for a photo when its `UniqueCameraModel` is the camera's make and model ("Nikon D750", "Fujifilm X-T3").

## Which profile is used

- `CameraProfile` names it, as in Lightroom: a preset's or sidecar's "Camera Standard" uses the `.dcp` called that for this camera.
- No `CameraProfile`: our own colour and base look, as before.
- A name we can't find (Lightroom's default "Adobe Color" is an `.xmp` profile, not a `.dcp`): our own look, and `amb render` and the editor say so.
- Rendered images (JPEG, PNG, …) and converted negatives don't use camera profiles.

## How a profile is applied

From the DNG specification's camera profile chapter, written from the specification:

1. **Matrices.** `ColorMatrix1/2` (and `ForwardMatrix1/2` when present) for `CalibrationIlluminant1/2` are mixed by the inverse of the white's colour temperature. The white's chromaticity is found by iteration, since the matrices depend on it. With a forward matrix, white-balanced camera RGB goes straight to XYZ D50; without one, the colour matrix is inverted and adapted (Bradford) to D50. Then to linear ProPhoto, with the white at 1. A custom white balance uses the profile's matrices to find its camera neutral.
2. **Hue/saturation map** (`ProfileHueSatMap*`), in the DNG's HSV of linear ProPhoto: hue shift in degrees, saturation and value scales, looked up trilinearly (hue wraps; value on sRGB-encoded values when the encoding says so). With two maps, each belongs to its illuminant; they are blended entry by entry, the cooler map's weight going from 1 at its temperature to 0 at the warmer one's, linear in inverse temperature, as the DNG SDK does (Adobe's help (maintainers' notes) §3).
3. **Exposure offset** (`BaselineExposureOffset`) and the user's Exposure, then the **look table** (`ProfileLookTable*`) the same way. As in the DNG reference render, a brightening Exposure goes in before the look table and a darkening one after it (DNG spec, `ProfileLookTableData`; Adobe's help (maintainers' notes) Q7). The SDK folds a darkening Exposure into its tone curve; we apply it as a plain gain after the table.
4. **Tone curve** (`ProfileToneCurve`, linear in and out) replaces our placeholder base look, applied the DNG way: the largest and smallest channels go through the curve and the middle one keeps its place, so hues don't shift. A profile without a curve keeps our base look (Camera Raw would use its own default curve).

The Calibration panel's primaries still apply on top of the profile's matrix.

## Not yet

- Adobe Raw `.xmp` profiles and creative profiles (`Look`, `RGBTable`). The `Table_<md5>` encoding is now documented (the maintainers' source notes). Two things are still missing: the base-85 alphabet, which the digest left out, and which `crs:` property points at which table.
- The DNG 1.6 third illuminant.
- `DefaultBlackRender`, monochrome profiles, the DNG's own `BaselineExposure`.
- A profile Amount slider (Lightroom has one for creative profiles only).

## Open questions

1. ~~Whether the user's Exposure goes before or after the look table.~~ Answered for the DNG reference render (brightening before, darkening after). Whether Lightroom does the same is not stated anywhere.
2. The exact white-temperature mixing near and beyond the illuminants (we clamp).
