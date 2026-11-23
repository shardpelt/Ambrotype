# Dust and hairs on film scans

Status: v1 built 2026-10-29, with a line tool for straight scratches. Code: `crates/amb-engine/src/dust.rs` (the search), `crates/amb-ops/src/dust.rs` (defects to spots), `amb dust` (`--line` for a scratch), and the spot tool: the **Find dust** button, and Shift-drag for a scratch.

## What it does

Find dust looks at the whole scan and puts a heal spot on each speck of dust and each short hair. The spots are ordinary spots, Lightroom's own `RetouchAreas`:
- they are one undo step ("dust and hairs");
- each can be selected, moved, resized or deleted in the spot tool;
- Lightroom reads them;
- a defect under a spot already there is left out, so running it again adds only new ones.

It is for scanned film: negatives and slides. It doesn't find long straight scratches (a wire or a mast looks the same), dust on busy ground (leaves, grass, lettering), or very faint specks. Those are left to the spot tool.

## How it finds them (ours)

On the scan, before a negative is turned positive, dust and hairs block light, so they are darker than what is around them, on a negative and a slide alike.

1. Log luminance, signed so a defect is a peak.
2. Its difference from the mean of a ring around each pixel: a 31-pixel square minus the middle 11 (at 4,000 pixels on the long side; scaled to the scan, never below half). Over grain it is zero on average; on a speck it is the speck's contrast.
3. The grain's spread around each pixel: the mean size of that difference over a 64-pixel square. Dark and busy parts are noisier in log units than a clear sky, so one figure for the frame would miss the sky's dust.
4. Pixels 4 spreads above their surroundings (and at least 0.05) seed a part; it grows over pixels 2.5 spreads above. A part must stand out as a whole: the sum of its contrast at least 12 spreads × √size.
5. Shape:
   - a speck fits in the middle square;
   - a hair is longer, at most 6 pixels wide and 120 long;
   - a part longer than 40 pixels whose pixels lie within 0.8 of one line is a line, not a hair, and is left alone.
6. Its surroundings:
   - its contrast must be twice the 90th percentile of the contrast around it (grain gives about 1.6 spreads; snow among grass or lettering much more);
   - at least 0.1 (fainter specks are hard to see, and experts leave them);
   - at most 15 % of its rim (the pixels within 2 of it) may be as dark as half its contrast: a twig's tip joins its branch, a hair lies on its own.
7. At most 500, the strongest first.

**Donor:** for each defect, eight directions (across a hair first), at 2.5 and 4 times its radius. The donor's footprint must hold no defect, and its surroundings (the ring means) must be most like the defect's own. A hair's spot is brushed along its line. A speck's is round, 1.5 pixels larger than the defect, with feather 30.

The heal itself is the spot tool's ([spot-removal.md](spot-removal.md)). It copies the donor whole, grain included, and matches it to the edge, so the grain carries across.

## Straight scratches (the line tool)

In the spot tool, Shift-drag along a scratch (or `amb dust PHOTO --line X0,Y0,X1,Y1`). There is no automatic search for them, since a long straight line is as likely a mast or a horizon.

- The line is cut into pieces of at most 160 pixels (at 4,000 on the long side), so a scratch that drifts a little is followed.
- Each piece's ends move up to 12 pixels across, half a pixel at a time, to where the line is a ridge: darker, or lighter, than the bands 4 pixels to both sides, by the smaller of the two steps. A picture's edge is a step to one side only, so it doesn't pull the line. Where several lines are nearly as good (a scratch wider than a pixel), the middle one is taken. Where nothing stands out (under 0.04), the piece stays where it was drawn.
- Neighbouring pieces meet halfway.
- Each piece becomes a brushed heal spot, as wide as the scratch (at most a hair's width), healed from the evener side.

On the long scratch of the FILM-R Provia scan, drawn roughly along it, it follows the scratch over 1,600 pixels and takes it out; a second scratch beside it stays until it is drawn too. Faint seams can show where a piece crosses an edge of the picture.

## Strength

Each repair is a spot, so its strength is the spot's Opacity, and it can be selected, moved or deleted in the spot tool. There are no separate repair layers.

## The bench

The FILM-R dataset (Ivanova, Williamson and Henderson, Eurographics 2023; CC BY 4.0) is 44 damaged 35 mm scans with an expert's repairs, as positive JPEGs. It is never committed:
- `tools/film-r.sh <DIR>` downloads it;
- `AMB_FILMR=<DIR> cargo test --release -p amb-engine --test dust_bench -- --ignored --nocapture` runs the bench;
- `AMB_DUST=seed=4,stand_out=1.5` tries other constants.

A repair is where the damaged and restored scans differ. On negatives, dust shows light in these positives; on slides, dark.

On six of the scans (2026-10-29; three negatives, two slides, one Cinestill):
- 73 % of the defects found were repairs the expert made. Most of the rest are faint specks the expert left.
- It found 21 % of the repairs, and 37 % of those on smooth ground (sky, walls).
- Most misses are long straight scratches along the film (Cinestill and Provia), dust in dark foliage, and tiny specks next to bright edges.
- About 0.3 s for a 6-megapixel scan.

## Not yet

- A scan's infrared channel (a fourth channel, or an `_ir` file), which shows dust directly: parked until there is a scan with one to test on ([GAPS.md](../GAPS.md) §9).
- A guided rectangle that searches harder in one area, and a heal brush that searches along its stroke.
- Light defects on a scan (emulsion scratches on a negative show light there, dark once inverted).

## Card: the guided rectangle and a heal brush that searches its stroke (`film-dust-guided`)

Tier: medium. Risk: low (adds spots only, one undo step each). Decided 2026-11-23.

Why: Find dust misses faint specks and dust on busy ground, on purpose, to keep false finds low. Where the person points, it can look harder.

Read first: `crates/amb-engine/src/dust.rs` (`find`, `DustStrengths`, `DEFAULT_DUST`, `distance_to_path`), `crates/amb-ops/src/dust.rs` (`find_image`, `spots_for`, `line_image`: the example to copy), `crates/amb-cli/src/dust.rs` (`--line`), `heal_line` and `find_dust` in `crates/amb-gui/src/commands/select.rs`, `crates/amb-gui/ui/tools/spots.js` (Shift-drag near line 409, `heal_line` near 519, Find dust near 572).

Design:
- Engine (`dust.rs`, stays under 800 lines before its tests):
  - `pub const GUIDED_DUST: DustStrengths = DustStrengths { seed: 3.0, significance: 9.0, stand_out: 1.5, faintest: 0.06, most: 200, ..DEFAULT_DUST };` with a comment: looser, because the person pointed at the area; starting values, to be benched on FILM-R.
  - `pub fn find_in(rgb, w, h, dark, k, area: (usize, usize, usize, usize)) -> Vec<Defect>`: crop the area grown by `k.noise_window` (scaled as `find` scales it) on each side, clamped; run `find` on the crop; shift `centre`, `path`, `donor` and re-index `pixels` back to the whole image; keep defects whose centre lies inside `area`.
  - `pub fn find_along(rgb, w, h, dark, k, stroke: &[(f64, f64)], radius: f64) -> Vec<Defect>`: `find_in` over the stroke's bounding box grown by `radius`, then keep defects whose centre is within `radius` of the stroke (`distance_to_path`).
- Ops (`amb-ops/src/dust.rs`): `pub fn area_image(img, settings, area: [f64; 4]) -> Found` and `pub fn along_image(img, settings, stroke: &[[f64; 2]], radius: f64) -> Found`, coordinates in fractions of the photo as `--line` takes them; both through `spots_for`, so spots under existing ones are left out; `_file` twins as `line_file` has.
- `amb dust PHOTO --area X0,Y0,X1,Y1` and `amb dust PHOTO --along X,Y;X,Y;… --radius R` (fractions; R a fraction of the long side), as `--line`; `USAGE`, the man page.
- Editor commands `find_dust_in(area)` and `find_dust_along(points, radius)` beside `find_dust` in `select.rs`, each one undo step named "dust in an area" / "dust along a stroke"; in `handler`.
- Spot tool: **Alt-drag** draws a rectangle (dashed, as the zoom box) and runs `find_dust_in`. A **Find in stroke** switch in the tool's block (key action `dust-brush`): while on, a brushed stroke runs `find_dust_along` with the brush's radius instead of becoming one brushed spot. Nothing found: `toast(t("No dust found there. A plain spot heals it by hand."))`. The tool's hint text (line 238) gains "Alt-drag a box to look harder for dust there".

Tests: in `dust.rs`, a synthetic grain field with a faint speck (contrast 0.07): `find` with `DEFAULT_DUST` misses it, `find_in` with `GUIDED_DUST` over it finds it at the right place (whole-image coordinates); a speck outside the stroke's band is not returned by `find_along`. A CLI test runs `amb dust --area` on a test photo. A UI step in `crates/amb-gui/tests/ui/retouch.js`: Alt-drag a box over a planted speck, one undo step added.

Don't: change `find`, `DEFAULT_DUST`, the line tool or the spot format.

Done when: the two commands and the two gestures add spots for faint dust where pointed, each one undo step; checks green. Then move "A guided rectangle…" out of "Not yet" above.

Commit: `dust: look harder in a box or along a stroke`
