# Spot removal (heal and clone)

Status: implemented 2026-10-10. Round heal and clone spots, read and written as Lightroom's own `RetouchAreas`; Lightroom's brushed spots, content-aware and generative fills, and the pre-2018 `RetouchInfo` are read and rendered. Red eye is not done yet.

Code: `crates/amb-presets/src/retouch.rs` (the shared model), `crates/amb-engine/src/retouch.rs` (rendering and the donor search), `crates/amb-protocol/src/spots.rs` (commands), `crates/amb-gui/ui/spots.js` (the tool).

## Lightroom's format

Facts from Lightroom-written files, as reported by the autoshade project (MIT, see NOTICE), which read them from a library of real sidecars, and an Adobe forum post showing a heal area:

- **`RetouchAreas`** is a list of areas. Each has `SpotType` (`heal`, `clone`, or `heal_patchmatch` for Lightroom's content-aware fill, with `fill_method="firefly"` when it is generative), `SourceState` (`sourceAutoComputed` or `sourceSetExplicitly`), `Method` (`gaussian`), `Feather` 0..1, `Opacity` 0..1, `Seed`, and its shape in `Masks`.
- **Shape:** a `Mask/Ellipse` component (`X`, `Y` the centre; `SizeX`, `SizeY` half-extents as fractions of the frame's *width*, so a round spot has equal sizes) or `Mask/Paint` brush strokes (the same dab format as brush masks, [masks.md](masks.md)).
- **Donor:** `SourceX` and `OffsetY` are the donor's centre as absolute fractions, despite the second name. Content-aware and generative areas store a search window instead, not a donor, and their pixels are not in the file.
- **`RetouchInfo`** (pre-2018) is a list of strings, `centerX = 0.5, centerY = 0.4, radius = 0.02, sourceState = …, sourceX = 0.6, sourceY = 0.4, spotType = heal` (darktable's importer reads them the same way). We assume its radius is in frame widths too.
- Coordinates are fractions of the uncropped photo in stored orientation, as for masks and the crop. `EnableRetouch` false turns them all off.


## Visualise spots

Spot panel → **Visualise** (A) shows the photo as Sobel edges, white on black, so dust and specks stand out, as Lightroom's Visualize Spots does; the slider raises the threshold so only stronger edges show. It is drawn in the page from the preview (`spots.js`, `drawEdges`), so it costs no render.

## How we render them

Spots run in order on the scene-linear image at the end of the first pipeline stage, before masks are drawn, so range masks see the cleaned photo.

- **Alpha:** a round spot is full inside 1 − `Feather` of its radius and fades to 0 at its edge (smoothstep), times `Opacity`. A brushed spot uses the brush model.
- **Clone:** the donor area copied as it is.
- **Heal:** the donor copied at a whole-pixel offset (so its texture isn't blurred), then matched to the spot's surroundings. In log space, the difference between the spot's surroundings and the donor's (a band just outside the spot, smoothed along it) is carried smoothly across the inside as a membrane (push-pull interpolation), and added. Brightness, colour and curved shading carry over from the whole edge, which a plane (used until 2026-10-22) couldn't do for long brushed strokes; the donor's texture stays. This is ours, not Lightroom's algorithm.
- **No donor** (content-aware, generative, or a spot added without one), and the editor's pick for a new spot: the search follows the spot's own shape, round or brushed. Candidates lie in 24 directions, just clear of the spot along each one and up to 2.5 times further. Each is scored on the band around the spot (log luminance with a small floor, so the noise of near-black areas doesn't swamp it): how far the donor's band differs from the spot's in shape, and a little in level (brightening a much darker donor brings up its noise and faint lines); how far the donor's inside strays from what its own band predicts (an object or a line in the donor); how different its fine texture is; and its distance, counted in the spot's narrowest width (beside a long stroke is near, along it is far). A donor may not overlap the spot. `amb render` and the editor say that content-aware and generative spots are approximated this way.
- A test scores a brushed heal across a curved scar on skin-like texture with rounded shading (`brushed_heal_blends_in`): leftover blemish 0.019 (0.087 before), seam 0.008 (0.027), fine texture kept at 88% (84%).
- Spots work on the cropped image, so a donor outside the crop is clamped to the crop's edge.

## Commands

- `add_spot {at, radius?, kind?, source?, source_auto?, feather?, opacity?}`: a round spot; `kind` heal (default) or clone, `radius` a fraction of the width (default 0.02), `source` the donor centre (searched for at render time when absent), `feather` and `opacity` 0–100.
- `set_spot {spot, values}`: `at`, `radius` (round spots), `source` (`null` to search again), `kind`, `feather`, `opacity`, `active`.
- `remove_spot {spot}`; query `list_spots`.

## The editor

`ui/spots.js`, opened with the Spots button or Q. Click a blemish: the backend's `spot_source` runs the donor search on a small render, and the spot is added with that donor (`sourceAutoComputed`), drawn as a dashed circle joined to the spot. Drag the spot (its donor moves with it) or the donor. The panel has Heal/Clone, Size (% of the width), Feather and Opacity, for the selected spot or the next one; [ and ] or the wheel resize; Delete removes the selected spot; Remove all is one undo step. Dragging on the photo paints a brushed spot instead (`add_spot` with `points`: one `Mask/Paint` stroke in the brush-mask dab format, the brush size and Feather from the panel); its donor is searched for the stroke's own shape (`spot_source` with `points`), and the stroke's centre copies from it, as the engine reads Lightroom's brushed spots. Brushed spots aren't drawn as circles and can't be moved yet.

## Open questions (for the Lightroom trial)

1. How Lightroom's heal blends (we fit a plane; Lightroom's result is smoother at the edges) and what `Feather` means exactly.
2. Whether `RetouchInfo`'s radius is in widths.
3. Which fields Lightroom needs to read our areas: we write `SpotType`, `SourceState`, `Method`, `SourceX`, `OffsetY`, `Feather`, `Opacity` and the ellipse; not `Seed` or the patch-match fields.
4. Whether spots, like brushes, are placed before the lens corrections.
5. Red eye (`RedEyeInfo`) and pet eye: the format is unknown; the trial keeps a sample ([lightroom-trial-plan.md](../lightroom-trial-plan.md) item 12).
