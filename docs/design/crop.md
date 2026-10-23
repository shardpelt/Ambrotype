# Crop and straighten

Status: implemented (`crates/amb-engine/src/crop.rs`, folded into the lens resampling pass in `crates/amb-engine/src/lens.rs`). The coordinate frame and the angle's sign follow darktable's Lightroom importer. Neither has been checked against Lightroom renders yet.

## Tags

| Tag | Range | Ours |
|---|---|---|
| `HasCrop` | bool | Crop on or off. When off, the other values are kept but ignored. |
| `CropLeft`, `CropTop` | 0..1 (default 0) | Top-left corner of the crop rectangle. |
| `CropRight`, `CropBottom` | 0..1 (default 1) | Bottom-right corner of the crop rectangle. |
| `CropAngle` | −45..45 degrees (default 0) | Straighten. The crop rectangle turns over the photo. |
| `CropConstrainToWarp` | bool | Constrain Crop. The lens pass zooms the frame so no blank areas remain ([lens-corrections.md](lens-corrections.md)). Not changed here. |
| `CropWidth`, `CropHeight`, `CropUnits`, `CropConstrainToUnitSquare` | — | Not rendered. They describe the crop tool's aspect and size settings; the rectangle itself is in the four corner tags. |

## Semantics we use

**Frame.** The corners are fractions of the image *after* the lens corrections, at the size of the uncropped image. They are in the photo's stored (sensor) orientation, before the EXIF orientation is applied. darktable's importer (`src/develop/lightroom.c`) reads them this way: it rotates the corners in sensor coordinates first, then flips and swaps them for the orientation.

**Angle.** (`CropLeft`, `CropTop`) and (`CropRight`, `CropBottom`) are two opposite corners of the crop rectangle: its top-left and bottom-right in the unrotated image. With an angle the rectangle turns, and the other two corners follow from the angle. In pixels, with the crop's axes u = (cos θ, sin θ) and v = (−sin θ, cos θ) (y down):

- crop width = (BR − TL) · u
- crop height = (BR − TL) · v
- output pixel (i, j) samples the frame at TL + (i + ½)·u·sx + (j + ½)·v·sy

Here sx and sy stretch the crop by less than half a pixel, so a whole number of output pixels covers it exactly. darktable does the equivalent: it turns both corners about the image centre into the straightened image's coordinates, and crops there.

**Sign.** θ = `ANGLE_SIGN` × `CropAngle`. A positive θ turns the photo counter-clockwise as seen on screen. darktable's importer stores `angle = −CropAngle` in its clipping module, and that module turns the image clockwise for a positive angle. So a positive `CropAngle` means counter-clockwise, in the sensor frame, and we follow that. darktable negates the angle again for mirrored EXIF orientations because it crops after its flip. We crop before orientation, so we do not need to.

**Output size.** The output is the crop rectangle's size in source pixels, rounded. Everything after the camera-space pass runs on the cropped image: negative conversion (including its auto analysis), colour, tone, detail, then the post-crop vignette and grain. So the vignette is centred on, and scaled to, the crop.

**Out-of-frame areas.** If the rectangle reaches outside the frame, the pixels outside become blank. They use the same mask and `blank_fill` as the lens pass. Lightroom always keeps a straightened crop inside the image, so this only happens with hand-written or mismatched values.

## Pipeline placement

- One resampling pass, Catmull-Rom: output pixel → crop map → Transform ([transform.md](transform.md)) → lens-corrected frame → fill zoom → manual distortion → profile distortion → per-channel CA → source.
- A crop with lens corrections is interpolated once.
- A crop on its own runs the same pass with an identity lens map. A crop aligned to whole pixels then copies pixels exactly.
- Export (`amb-export`) and the GUI preview orient the image after `render`. The crop is defined in stored orientation, so this order is correct (`crop_in_stored_orientation_then_orient` test).
- The GUI preview renders from a downscaled `CameraImage`. The crop is normalised, so the preview is cropped the same way. `PhotoInfo` keeps the uncropped size.

## GUI crop tool

- `crates/amb-gui/ui/crop.js` (Crop button or R). It previews the photo with `HasCrop` overridden off and draws the rectangle over it. The photo is drawn turned by the straighten angle, so the rectangle stays level on screen, as in Lightroom. Drag an edge or a corner to resize, drag inside to move, drag outside to straighten (or use the Angle slider). The rectangle stays inside the photo: drags stop at the edges, and straightening shrinks it about its centre.
- The tool works upright, in full-size pixels of the uncropped photo. `amb_export::geometry::{upright_crop, stored_crop}` convert to and from the stored-orientation tags (exposed as the `crop_state` and `crop_commands` commands). A quarter-turn orientation swaps the sides. A mirrored one reverses the sense of the angle, so the slider shows `−CropAngle` for orientations 2, 4, 5 and 7. The `upright_crops_match_the_engine_crop_in_every_orientation` test checks this pixel by pixel through `Crop::geometry` for all 8 orientations.
- Done (Enter) writes `HasCrop` and the five corner and angle tags as one undo step ("crop"). The whole photo with no angle writes `HasCrop` false and removes the others. Reset does the same at once, and Esc cancels.
- Aspect locks: Original, Free, 1:1, 4:5, 2:3, 3:4, 16:9, 9:16, and every size preset (`crates/amb-gui/data/size-presets.json`, plus the user's own, kept in the app's `settings.json`). X swaps portrait and landscape. `CropWidth`, `CropHeight` and `CropUnits` are not written.
- Export can produce an exact size from a size preset (`Resize::Exact`, CLI `--exact WxH`). When its aspect differs from the crop, the image is scaled to cover the size and centre-cropped, and the dialog says so.

## Crop to the view

The crop tool opened while zoomed in (Settings → General, on by default) starts with what the view shows: the view's centre and size in the developed photo's pixels, mapped into the current crop along its turned axes (`zoomViewRect` in `ui/croptool.js`). Done crops to it. Lens corrections and Transform bend the mapping a little; the rectangle can be adjusted before Done.

## Guides and snapping

While turning (a drag outside the frame, or the Angle slider), the crop tool draws the photo's long straight lines near horizontal or vertical: the segments Upright's detector finds (`amb_engine::upright::guide_lines`, at most 40, longest first), in the tool's upright pixels (`guide_lines` command). A line turns green when the angle makes it level or upright. A drag snaps to such an angle within 0.6°; Alt turns freely. The crop keeps a locked aspect and shrinks to the largest rectangle inside the turned photo, as with the slider.

## Level

The crop tool's **Level ⟂** (L) asks for the angle Upright's Level fit finds (`Session::level_angle`: `amb_engine::upright::analyse` in Level mode, on the lens-corrected photo's long, near-horizontal and near-vertical lines), turned into the tool's upright view, and sets it like the Angle slider, so the rectangle shrinks to fit. A photo with too few lines says so. It is a plain straighten, stored as `CropAngle`, which Lightroom reads; Upright's Level stays a separate Transform setting. Tried on a horizon turned 4°: Level gave −4.00° and the export's horizon measured level.

## Open questions (for the Lightroom trial)

1. **Angle sign.** We take darktable's word that a positive `CropAngle` turns the photo counter-clockwise. To check: straighten a photo in Lightroom by +5° and read the sidecar. If Lightroom disagrees, flip `crop::ANGLE_SIGN`.
2. **Corner semantics.** Are the stored values the rotated rectangle's corners (darktable's and our reading)? Or are they an axis-aligned box that is then turned about the image centre? The two agree for a centred crop with no angle, and differ for off-centre straightened crops.
3. **Which frame?** We assume coordinates relative to the lens-corrected frame, after profile auto-scale and our Constrain Crop zoom. In Lightroom, Constrain Crop *moves the crop rectangle*. So a sidecar with `CropConstrainToWarp` probably already stores a constrained rectangle in the unzoomed frame, and applying our zoom as well would crop too tight. Measure, then decide whether to skip the zoom when `HasCrop` is set.
4. **Transform.** It comes before the crop, as in Lightroom: the crop is drawn on the transformed image, and the crop map composes after the homography in the same pass ([transform.md](transform.md)). Whether the corners are relative to the transformed image's scaled frame (Transform `Scale`, Constrain Crop) is unknown.
5. **Rounding of the output size,** and whether Lightroom uses `CropWidth`/`CropHeight` to set the exported pixel size for fixed-size crops.
