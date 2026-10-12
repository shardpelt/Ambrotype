# Transform

Status: implemented 2026-10-10. The manual sliders and Upright Auto, Level, Vertical and Full render. Guided (2026-10-12) works our own way; see below.

Code: `crates/amb-engine/src/transform.rs` (the sliders' model), `crates/amb-engine/src/upright.rs` (finding the Upright correction), composed into the resampling pass in `crates/amb-engine/src/lens.rs`. The editor's Transform panel (an Upright choice and the sliders) is generated from the registry like the others.

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
| `PerspectiveUpright` | 0..5 | Upright mode: 0 off, 1 Auto, 2 Level, 3 Vertical, 4 Full, 5 Guided. The five names are Adobe's (help and Kost); the numbers are our reading, not stated anywhere ([open-questions.md](../adobe-docs/open-questions.md) Q3). Guided is not rendered; `amb render` and the editor say so. |
| `Upright*` | — | Lightroom's own Upright analysis. Image-specific and not read: we find the correction again. Not reported as missing. |


## Guided Upright (ours)

Lightroom keeps Guided's lines in `UprightFourSegments_*`, which nothing we can read describes, and writing a guess could break Lightroom's files. So our Guided tool doesn't use `PerspectiveUpright` 5:

- Transform → Upright → **Guided…** shows the photo without crop or Transform. Drag up to four guides along lines that should be vertical or horizontal (each counts as whichever it is closer to); drag an end to adjust, right-click it to remove; Enter applies, Esc cancels.
- `upright::guided` finds the Transform sliders that straighten the guides: Rotate always, Vertical with two or more vertical guides, Horizontal with two or more horizontal ones (a least-squares fit, so more guides than needed are averaged). Aspect, Scale and the offsets stay as they are, since they don't change whether a line is straight.
- One undo step sets Vertical, Horizontal and Rotate, turns Upright off, clears the crop (Alt keeps it, as with the other modes) and keeps the guides in `amb:UprightGuides` so they can be edited again. Lightroom reads the sliders, so the photo opens corrected there too, as a manual Transform.
- Limits: Rotate only goes to ±10°, and Vertical and Horizontal to ±100 (the model's 30° tilt); guides that need more are fitted as far as they go, and the editor says how far off the worst one still is.

## The model (ours, not Adobe's)

The sliders build one homography on the lens-corrected frame, in stored orientation. In order:

1. **Vertical and Horizontal** turn a virtual pinhole camera about the frame's x and y axes: H = K·R·K⁻¹. The camera's focal length is fixed (a 35 mm equivalent of 35 mm, with the frame's diagonal as 35 mm film's). Then the frame centre is moved back to its place at its old local size, so the photo keystones about its centre. ±100 is a 30° turn.
2. **Rotate** about the centre.
3. **Aspect**: the vertical stretch over the horizontal stretch is 2^(Aspect/100), at the same area.
4. **Scale** about the centre.
5. **X and Y** move the photo.

The constants and signs live in `TransformStrengths` (the `transform` section of the calibration file).

## Upright (ours, not Adobe's)

Upright finds a correction from the photo's own straight lines. It comes before the sliders, which apply on top of it, as in Lightroom.

As in Lightroom, picking an Upright mode in the editor clears the manual Transform sliders and the crop in the same undo step; holding Alt keeps the crop (Kost, cited in Q3). Adobe's help describes Full as Level, Vertical and Auto together.

1. **Analysis image.** The photo is shrunk by a whole factor to at most 1024 pixels on the long side, then lens-corrected (so straight lines are straight). The editor and export start from the same full-size photo, so they find the same correction. The editor finds it once per mode and lens setting, not per preview.
2. **Lines.** A small line-segment detector, in the spirit of LSD: a light blur, Sobel gradients on the square root of luminance, then pixels with a strong gradient grow into regions whose edge direction stays within 22.5°. Regions that are long (3% of the long side), thin and well filled become segments.
3. **Fit.** Segments within 25° of vertical or horizontal are candidates. A pinhole camera (focal length from EXIF times the crop factor, or 35 mm) is turned: tilt about the x axis, about the y axis, then roll. First a robust fit from no correction, then least squares on the segments within 2.5°.

| Mode | Turns | Lines used |
|---|---|---|
| Level | roll | horizontals, verticals at half weight |
| Vertical | vertical tilt, roll | verticals, horizontals at 0.3 weight |
| Full | both tilts, roll | both |
| Auto | both tilts, roll | both, with a larger penalty on the tilts, so strong corrections are made only partly |

A small penalty on the tilts in the robust fit keeps a tilt that few lines settle from running away. Too few lines (about two lines a fifth of the frame long) means no correction. The constants are in `UprightStrengths` (the `upright` section of the calibration file).

Full on a building seen at an angle squares up its receding sides, as Lightroom's Full does. That is often a strong change; Auto is the gentler choice.

## Pipeline placement

- The Transform comes after the lens corrections and before the crop, as in Lightroom: the crop is drawn on the transformed photo.
- It is part of the single resampling pass: output pixel → crop → transformed frame → inverse homography → lens-corrected frame → zoom → manual distortion → profile distortion → per-channel CA → source. A transformed, cropped, lens-corrected photo is interpolated once.
- The transformed frame has the size of the uncropped photo. Areas the photo no longer covers are blank, filled like the lens pass's.
- Constrain Crop finds the smallest zoom about the centre that leaves no blanks, after the lens's own fill.
- Upright is applied first, then the sliders: H = sliders · Upright, on the lens-corrected frame.
- Masks and spots use the crop's frame, which is now the transformed frame. Whether Lightroom places them before or after the Transform is unknown.

## Open questions (for the Lightroom trial)

1. **Signs.** Which way each slider moves the photo. To check: one render per slider at −50 and +50.
2. **Strengths.** The tilt at ±100, the focal length (fixed, or the photo's own, as `UprightFocalLength35mm` hints), the Aspect ratio at ±100, and the offset at ±100. To check: a grid chart, each slider at five values.
3. **Order** of Rotate, Aspect and Scale against the keystone, and whether Lightroom keeps the centre fixed.
4. **Which frame** the crop, masks and spots refer to once a Transform is set.
5. **Upright.** How close our corrections come to Lightroom's for each mode, and how strong Auto is. Whether `UprightTransform_0..5` hold Lightroom's solved transforms (one per mode?); if they do, a Lightroom sidecar could be rendered exactly, without solving.
6. **The mode numbers** 0..5 for Off, Auto, Level, Vertical, Full and Guided, and how Guided's lines are stored (`UprightFourSegments_*`). The SDK and the collected pages name the modes but give no numbers or storage (Q3).
