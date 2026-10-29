# Dust and hairs on film scans

Status: v1 built 2026-10-29, with a line tool for straight scratches. Code: `crates/amb-engine/src/dust.rs` (the search), `crates/amb-library/src/dust.rs` (defects to spots), `amb dust` (`--line` for a scratch), and the spot tool: the **Find dust** button, and Shift-drag for a scratch.

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
