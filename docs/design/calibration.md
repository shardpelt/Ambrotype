# Calibration panel

Status: implemented 2026-10-10, our own model, unfitted.

Code: `crates/amb-engine/src/primaries.rs`, applied with the camera matrix in `crates/amb-engine/src/pipeline.rs`. The editor's Calibration panel is generated from the registry like the others.

## Tags

| Tag | Range | Ours |
|---|---|---|
| `RedHue`, `GreenHue`, `BlueHue` | −100..100 | Turn a primary about the grey axis. Positive moves red toward yellow, green toward cyan, blue toward magenta. ±100 is 30°. |
| `RedSaturation`, `GreenSaturation`, `BlueSaturation` | −100..100 | Move a primary away from grey (positive) or toward it. +100 puts it 1.6 times as far, −100 at 0.4. |
| `ShadowTint` | −100..100 | Positive adds magenta to the shadows, negative green. ±100 is 0.35 stops on the green channel in the deepest shadows, fading out by scene-linear 0.18. |
| `EnableCalibration` | bool | False turns all of the above off. |

## The model (ours, not Adobe's)

Camera Raw says these sliders move the camera's primaries. We move the primaries of the linear ProPhoto working space, right after the camera matrix and white balance:

1. Each primary (1, 0, 0), (0, 1, 0), (0, 0, 1) is split into grey (1/3, 1/3, 1/3) and its difference from grey. The difference is turned about the grey axis by the hue slider and scaled by the saturation slider.
2. The three new primaries make a matrix. Its columns are rescaled so (1, 1, 1) maps to (1, 1, 1): white and greys don't change, so the sliders never shift the white balance.
3. The matrix multiplies into the camera matrix, so it costs nothing per pixel.
4. Shadow Tint scales green down (or up) and red and blue the other way by half as much, weighted by a smoothstep from the darkest values to 0.18.

The constants are in `PrimariesStrengths` (the `primaries` section of the calibration file).

## Open questions (for the Lightroom trial)

1. **The space.** Whether Camera Raw moves the camera's own primaries (before the camera matrix) or the working space's, and how its hue scale maps to degrees. To check: a ColorChecker with each slider at −100, −50, +50 and +100.
2. **Signs.** The slider gradients in Lightroom suggest our directions; the sweeps settle them.
3. **Shadow Tint's range:** how far into the midtones it reaches.
