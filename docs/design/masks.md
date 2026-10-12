# Masks and local adjustments

Status: implemented 2026-10-08. Linear and radial gradients, brush, luminance range and colour range masks, combined by add, subtract and intersect, each mask with its own local sliders. They are read and written as Lightroom's own tags, so masks round-trip with Lightroom Classic 11+ and Camera Raw 14+. The fall-off shapes and slider strengths are ours and wait for the Lightroom trial.

Code: `crates/amb-presets/src/masks.rs` (the shared model: tags, slider units, parts), `crates/amb-engine/src/mask.rs` (drawing and rendering), `crates/amb-protocol/src/masks.rs` (commands), `crates/amb-gui/ui/masks.js` (the tool).

## Lightroom's format

Masks are plain settings: `crs:MaskGroupBasedCorrections` is a list of *corrections*, one per mask in Lightroom's Masks panel. A correction has its local sliders, `CorrectionAmount` (0–1), `CorrectionActive`, `CorrectionName`, `CorrectionSyncID` and `CorrectionMasks`, the list of its *components* in the order they combine. Pre-2021 files keep one list per tool (`GradientBasedCorrections`, `CircularGradientBasedCorrections`, `PaintBasedCorrections`) with the same correction layout, sometimes with a `CorrectionRangeMask` on the whole correction (Lightroom 7–10 range masks). Each list has a panel switch (`Enable…Corrections`).

These facts were checked against Lightroom-written files (from the field notes of a project that tested its reader on a corpus of real sidecars and Adobe's bundled presets, read for the format only):

- **Frame.** Coordinates are fractions of the *uncropped* photo in its stored (sensor) orientation, the same frame as the crop corners ([crop.md](crop.md)). Values may leave 0..1.
- **Slider units.** Local sliders are stored as UI value / 100 (`LocalHighlights2012="-0.4"` is −40), except Exposure, stored as EV / 4 (`LocalExposure2012="0.25"` is +1 EV). `LocalToningHue` is in degrees as it is.
- **Combination.** Add is `MaskBlendMode="0"` with `MaskValue="1"`. Subtract is `MaskBlendMode="1"` with `MaskValue="0"`. Intersect is subtract with `MaskInverted="true"` (A ∩ B = A − not B). So subtracting an inverted shape *is* intersecting, and our editor shows it that way.
- **Mirrors.** A radial gradient's `Flipped` is always `!MaskInverted`; a range mask's `CorrectionRangeMask.Invert` always equals `MaskInverted`. We write both. Old files without `MaskInverted`: a radial with `Flipped="false"` acts outside the ellipse, as Lightroom 5's radial filter did by default.
- **Kinds** (`What`): `Mask/Gradient` (linear: `ZeroX/ZeroY` where it has no effect, `FullX/FullY` where it is full), `Mask/CircularGradient` (radial: the ellipse's box `Top/Left/Bottom/Right`, `Angle`, `Feather` 0–100, `Midpoint` 50, `Roundness` 0, `Version` 2), `Mask/Aggregate` (a brush: it carries the combination, and its `Masks` list holds the `Mask/Paint` strokes; older files put strokes straight into `CorrectionMasks`), `Mask/Paint` (a stroke: `Radius`, `Flow`, `CenterWeight` (hardness), `MaskValue` as density, `Dabs`), `Mask/RangeMask` (with `CorrectionRangeMask`: `Type` 1 colour, 2 luminance, 3 depth; `LumRange` as four numbers: none below the first, full from the second to the third, none above the fourth), `Mask/Image` (AI masks: `MaskSubType` 1 subject, 2 sky, 3 people, 0 objects and landscape parts, with `MaskSubCategoryID`).
- **Brush strokes.** `Dabs` is a token list: `r <radius>`, `f <flow>`, `h <hardness>` change the state, `d <x> <y>` stamps a dab. The radius is a fraction of the frame's *width* (a dab is a circle in pixels), and Lightroom spaces dabs 0.2 × the radius apart. Newer Lightroom versions may keep a brush's strokes in the photo's `.acr` file instead (the aggregate then has `MaskBrushTable`, an MD5 key into Brotli-compressed records); we don't read those yet and report them.
- **Ids** are 32 upper-case hex digits; numbers have up to 6 decimals; booleans in masks are `true`/`false`.

The brush facts and the brush model below come from the autoshade project (MIT, see NOTICE), which measured them against 29 controlled Lightroom exports and a library of real sidecars.


## Select Sky (ours)

Lightroom's AI masks keep no pixels in the file, so they have to be found again. Select Sky (`Mask/Image`, `MaskSubType` 2; `add_mask` kind `sky`, the Sky button) is found without a model (`mask::sky_map`):

1. On a grid of at most 128 cells a side, each cell scores by its colour (OkLab of the tone-encoded light: bright, and blue, cyan or grey, i.e. not yellow) times how little its brightness changes against its neighbours.
2. Cells above 0.5 in the top row start a fill through cells above 0.35, so a blue lake or a white wall that doesn't touch the sky is left out.
3. The region grows two cells into sky-coloured cells however textured (sky between branches).
4. Each pixel's alpha is the grid (bilinear) times that pixel's own colour score, so branches and edges against the sky stay sharp.

A region render (the 100% view) takes the grid from the small render of the whole photo, like colour range targets. It misses orange sunset skies and dark storm clouds, and can take in a pale distant hill that meets the sky; a model would do better (see GAPS).

## Our model

**Parts.** A *part* is what the editor shows as one mask component. Each brush stroke is its own `Mask/Paint` entry, so consecutive strokes with the same `MaskSyncID` (or, in old files, none) form one brush part. Every other entry is a part on its own.

**Drawing a mask** (an alpha plane the size of the output). Starting from nothing, each part in order:
- Add: alpha = max(alpha, shape × `MaskValue`).
- Subtract or intersect: alpha = alpha × (1 − shape), where an inverted shape is 1 − shape.
- A correction-level range mask (old files) multiplies the result.

Shapes, evaluated per output pixel at its frame position (through the crop's map, so masks stay on the subject when the crop changes):
- **Linear:** smoothstep of the projection between the Zero and Full points, in pixels.
- **Radial:** d = the distance in the ellipse's own axes (1 on the ellipse). Full inside 1 − Feather/100, smoothstep down to 0 at the ellipse. Positive `Angle` turns it counter-clockwise on screen.
- **Brush** (`crates/amb-engine/src/brush.rs`, the measured model): a dab's falloff is k(ρ) = (1 − ρ^m)^n with ρ the distance over the radius and ln m, ln n cubic in the hardness (soft brushes are nearly gaussian, hard ones nearly a top hat); a dab deposits density × D(flow), D(f) = κf / (1 − f + κf) with κ = 0.1284; dabs combine by screen, α = 1 − Π(1 − deposit·k). Erasing is ours, since how Lightroom stores it is not known: inside an aggregate a stroke with `MaskValue` 0 erases (α × Π(1 − D·k)); in older files a stroke whose zero-ness differs from the part's first stroke erases.
- **Luminance range:** on the photo's perceptual brightness (the base tone look, sRGB-encoded) right after white balance and Exposure, before any local adjustment.
- **Colour range:** Lightroom's own colour samples (`AreaModels`) are in an undocumented format, so we keep our own sample positions in `amb:ColorSamples` (a list of `"x y"` image fractions) inside `CorrectionRangeMask`. The mask is the closest sample's OkLab distance (lightness weighted by ½) through a smoothstep, with the tolerance set by `ColorAmount` (Refine). Lightroom keeps the field it doesn't know; a colour range from Lightroom selects nothing here and is reported.
- **AI, depth and unknown kinds** select nothing, and `amb render` and the editor report them as not rendered yet. Applying them to the whole photo would be worse than leaving them out.

**Local sliders** act right after their global counterparts, through the mask's alpha, scaled by `CorrectionAmount`:

| Stage | Sliders | How |
|---|---|---|
| after white balance and Exposure (scene-linear) | Defringe | above 0, a Defringe of its own (both colours at `MaskStrengths.defringe` × the slider, the global hue ranges) weighted by alpha; below 0, the global Defringe weakened by alpha × the slider |
| after white balance and Exposure (scene-linear) | Exposure, Temp, Tint | gain 2^(EV × alpha); Temp/Tint as red/blue and green gains that keep luminance |
| after global Dehaze | Dehaze | a dehazed copy blended in |
| after the global tone controls | Contrast, Highlights, Shadows, Whites, Blacks | the tone curve with the local values, as a luminance ratio raised to alpha (hue-preserving) |
| after Texture/Clarity | Texture, Clarity; Saturation and Color (toning) | a copy through the local-contrast filter blended in; chroma scale and an OkLab tint |
| after sharpening | Sharpness (below 0: a soft blur), Noise | a sharpened, blurred or denoised copy blended in |

Mask curves (`MainCurve`, `RedCurve`, `GreenCurve`, `BlueCurve`) apply after the global curves, the same way, faded by the mask (`Drawn::apply_curves`); we read them like the global point curve, which is a guess until a Lightroom sample confirms it.

Not rendered yet (kept in the file, reported): Moire, `LocalHue`, `LocalGrain`, `LocalPointColors`. Strengths are in `Calibration.masks` (`MaskStrengths`) for fitting.

**Overlay.** The editor's preview can tint one mask red (`DevelopSettings.masks.overlay`, by its position in `MaskGroupBasedCorrections`), after everything else, as Lightroom's overlay does.

## Commands

All four are undoable protocol commands (`crates/amb-protocol/src/masks.rs`), so the editor, the CLI and AI clients make masks the same way. Masks and parts are numbered from 0 as `list_masks` shows them.

- `add_mask {kind, mask?, mode?, values?}`: `kind` linear, radial, luminance or color. Without `mask`, a new mask; with it, a new part of that mask joined by `mode`.
- `set_mask {mask, part?, values}`: without `part`, the local sliders in UI units (by tag, `LocalExposure2012`, or label, `Exposure`), `name`, `active` and `amount` (0–100); with `part`, its geometry (Lightroom's field names: `ZeroX`…, `Top`…, `Angle`, `Feather`, `LumRange`, `ColorSamples`, `ColorAmount`), `mode`, `invert`, `active`, `name`. Geometry is checked (two different linear points; Top < Bottom and Left < Right).
- `paint_mask {mask?, part?, mode?, points, size, feather, flow, density, erase?}`: a stroke through `points` (image fractions; gaps filled with dabs 0.2 × the size apart, as Lightroom spaces them). `size` is the radius as a fraction of the photo's width. Without `part` it starts a brush part (a `Mask/Aggregate`), in a new mask without `mask`.
- `remove_mask {mask, part?}`: removing a mask's last part removes the mask.
- `duplicate_mask {mask}`: a copy just after it, "… copy", with new sync IDs.
- `move_mask {mask, to}`: a new position; the panel lists masks in this order.
- Query `list_masks`: names, amount, active, sliders in UI units, and each part's kind, mode, invert and geometry (for brushes, the stroke and dab counts).

Applying a preset with masks *adds* them to the photo's own (replacing one with the same `CorrectionSyncID`), with the preset Amount scaling their sliders. The old per-tool tags are rendered but not editable with these commands.

## The editor

`ui/masks.js`, opened with the Masks button or M. The panel sits with the crop panel; a canvas over the photo draws the handles. `mask_frame` gives the affine map from the shown (upright, cropped) preview to mask coordinates, so the tool works in every orientation and crop.

- New mask: Linear (L), Radial (Shift+R), Brush (B), Luminance, Color. Then draw: a linear from where it is full to where it ends; a radial from the centre out; a brush stroke (Alt erases; [ and ] or the wheel change the size); a colour by clicking it (Shift-click adds samples).
- The selected mask lists its parts, each with its mode (add, subtract, intersect), invert and delete; the same buttons add a part to it. Under them: the part's settings (radial Feather and Angle; luminance From, To and Smoothness; colour Refine and samples; brush stroke count) and the mask's Amount and local sliders, grouped like Lightroom's.
- Dragging a handle (a linear's ends or line, a radial's centre or axis ends) or a slider previews live with an edited copy of the tag as an override; release runs one command, so each change is one undo step with a readable history line ("Mask 2: edit part 1").
- Duplicate, ↑ and ↓ above the parts copy and move the selected mask; dragging a mask's row onto another moves it there too.
- O shows or hides the red overlay of the selected mask (on by default while drawing). Delete removes the selected part, or the mask with its last part. Esc stops drawing, then leaves.

## Open questions (for the Lightroom trial)

1. The fall-off curves of linear and radial gradients and how Feather maps onto them ([lightroom-trial-plan.md](../lightroom-trial-plan.md) item 12: exposure through a gradient on flat grey).
2. The sign of a radial's `Angle`.
3. How Lightroom stores erasing inside a brush.
4. Whether luminance ranges select on the original or the adjusted photo, and on what brightness scale.
5. The strength of every local slider against its global counterpart (a local +50 against a global +50 on the same photo).
6. How several masks stack where they overlap (we apply them one after another).
7. Lightroom's colour-range sample format (`AreaModels`), so its colour ranges can be read.
8. The frame: Lightroom draws brush masks before the lens corrections (measured by autoshade); we draw every mask on the lens-corrected frame, so with a strong distortion correction a mask lands slightly off. Check radials and gradients too.
9. Reading brushes from the `.acr` file (`MaskBrushTable`).
