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
- **Heal:** the donor copied, then matched to the spot's surroundings: in log space, the difference between the ring just outside the spot and the same ring around the donor is fitted with a plane per channel (least squares) and added across the spot. Brightness, colour and gentle gradients carry over; the donor's texture stays. This is ours, not Lightroom's algorithm.
- **No donor** (content-aware, generative, or a spot added without one): we heal from the donor the search picks. Candidates sit 1.6 to 4 radii away in 16 directions; each is scored on how well the two rings around it match the spot's rings (log luminance), plus how much busier its inside is than its own surroundings (a donor holding an object is; a stretch of the same edge or texture isn't), with a slight preference for near ones. `amb render` and the editor say that content-aware and generative spots are approximated this way.
- Spots work on the cropped image, so a donor outside the crop is clamped to the crop's edge.

## Commands

- `add_spot {at, radius?, kind?, source?, source_auto?, feather?, opacity?}`: a round spot; `kind` heal (default) or clone, `radius` a fraction of the width (default 0.02), `source` the donor centre (searched for at render time when absent), `feather` and `opacity` 0–100.
- `set_spot {spot, values}`: `at`, `radius` (round spots), `source` (`null` to search again), `kind`, `feather`, `opacity`, `active`.
- `remove_spot {spot}`; query `list_spots`.

## The editor

`ui/spots.js`, opened with the Spots button or Q. Click a blemish: the backend's `spot_source` runs the donor search on a small render, and the spot is added with that donor (`sourceAutoComputed`), drawn as a dashed circle joined to the spot. Drag the spot (its donor moves with it) or the donor. The panel has Heal/Clone, Size (% of the width), Feather and Opacity, for the selected spot or the next one; [ and ] or the wheel resize; Delete removes the selected spot; Remove all is one undo step. Dragging on the photo paints a brushed spot instead (`add_spot` with `points`: one `Mask/Paint` stroke in the brush-mask dab format, the brush size and Feather from the panel); its donor is searched for the stroke's bounding box, whose centre copies from it, as the engine reads Lightroom's brushed spots. Brushed spots aren't drawn as circles and can't be moved yet.

## Open questions (for the Lightroom trial)

1. How Lightroom's heal blends (we fit a plane; Lightroom's result is smoother at the edges) and what `Feather` means exactly.
2. Whether `RetouchInfo`'s radius is in widths.
3. Which fields Lightroom needs to read our areas: we write `SpotType`, `SourceState`, `Method`, `SourceX`, `OffsetY`, `Feather`, `Opacity` and the ellipse; not `Seed` or the patch-match fields.
4. Whether spots, like brushes, are placed before the lens corrections.
5. Red eye (`RedEyeInfo`) and pet eye: the format is unknown; the trial keeps a sample ([lightroom-trial-plan.md](../lightroom-trial-plan.md) item 12).
