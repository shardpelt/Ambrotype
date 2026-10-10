# Transform

Status: the manual sliders are implemented (2026-10-10). Upright (Auto, Level, Vertical, Full, Guided) is not rendered yet.

Code: `crates/amb-engine/src/transform.rs` (the model), composed into the resampling pass in `crates/amb-engine/src/lens.rs`. The editor's Transform panel is generated from the registry like the others.

## Tags

| Tag | Range | Ours |
|---|---|---|
| `PerspectiveVertical` | −100..100 | Keystone about the horizontal axis. Negative widens the top: it straightens verticals that lean together in a photo taken looking up. |
| `PerspectiveHorizontal` | −100..100 | Keystone about the vertical axis. Positive makes the right side taller. |
| `PerspectiveRotate` | −10..10 degrees | Turns the photo about its centre. Positive is counter-clockwise, like `CropAngle`. |
| `PerspectiveAspect` | −100..100 | Positive stretches the photo vertically, negative horizontally. The area stays the same. |
| `PerspectiveScale` | 50..150 (default 100) | Scales the photo about its centre. |
| `PerspectiveX`, `PerspectiveY` | −100..100 | Move the photo right and down. ±100 is half the width or height. |
| `EnableTransform` | bool | False turns all of the above off. |
| `CropConstrainToWarp` | bool | Constrain Crop: zooms in until no blank areas remain (shared with the lens corrections). |
| `PerspectiveUpright` | 0..5 | Upright mode: 0 off, 1 Auto, 2 Level, 3 Vertical, 4 Full, 5 Guided. Not rendered; `amb render` and the editor say so. |
| `Upright*` | — | Lightroom's Upright analysis. Image-specific and not read. |

## The model (ours, not Adobe's)

The sliders build one homography on the lens-corrected frame, in stored orientation. In order:

1. **Vertical and Horizontal** turn a virtual pinhole camera about the frame's x and y axes: H = K·R·K⁻¹. The camera's focal length is fixed (a 35 mm equivalent of 35 mm, with the frame's diagonal as 35 mm film's). Then the frame centre is moved back to its place at its old local size, so the photo keystones about its centre. ±100 is a 30° turn.
2. **Rotate** about the centre.
3. **Aspect**: the vertical stretch over the horizontal stretch is 2^(Aspect/100), at the same area.
4. **Scale** about the centre.
5. **X and Y** move the photo.

The constants and signs live in `TransformStrengths` (the `transform` section of the calibration file).

## Pipeline placement

- The Transform comes after the lens corrections and before the crop, as in Lightroom: the crop is drawn on the transformed photo.
- It is part of the single resampling pass: output pixel → crop → transformed frame → inverse homography → lens-corrected frame → zoom → manual distortion → profile distortion → per-channel CA → source. A transformed, cropped, lens-corrected photo is interpolated once.
- The transformed frame has the size of the uncropped photo. Areas the photo no longer covers are blank, filled like the lens pass's.
- Constrain Crop finds the smallest zoom about the centre that leaves no blanks, after the lens's own fill.
- Masks and spots use the crop's frame, which is now the transformed frame. Whether Lightroom places them before or after the Transform is unknown.

## Open questions (for the Lightroom trial)

1. **Signs.** Which way each slider moves the photo. To check: one render per slider at −50 and +50.
2. **Strengths.** The tilt at ±100, the focal length (fixed, or the photo's own, as `UprightFocalLength35mm` hints), the Aspect ratio at ±100, and the offset at ±100. To check: a grid chart, each slider at five values.
3. **Order** of Rotate, Aspect and Scale against the keystone, and whether Lightroom keeps the centre fixed.
4. **Which frame** the crop, masks and spots refer to once a Transform is set.
5. **Upright.** Whether `UprightTransform_0..5` hold the solved transforms (then a sidecar could be rendered without solving), or whether we need our own solver (a line-based one like darktable's `ashift`).
