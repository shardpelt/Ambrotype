# LUTs (.cube looks)

Status: built 2026-10-12.

Many looks are shared as `.cube` files (the 3D LUT format of Resolve and most look packs). Lightroom only takes LUTs inside its own profiles, so ours is a setting of our own: `amb:LUT` (the file's path) and `amb:LUTAmount` (0–100). Lightroom ignores both.

## How it renders

- After the colour and curves stage, before Detail and Effects (`pipeline.rs`, `apply_lut`).
- The pixel is turned into sRGB-encoded values, as LUTs expect, looked up (trilinear), turned back, and mixed with the original by the amount. Colours outside sRGB are clipped for the table.
- 1D, 3D and 1D-then-3D (shaper) files, `DOMAIN_MIN`/`DOMAIN_MAX` and the `LUT_*_INPUT_RANGE` lines are read (`crates/amb-engine/src/lut.rs`). 3D tables up to 129 points a side.
- A file is read once and again only when it changes. A missing or broken file is listed as not rendered.
- The GPU preview keeps running with a LUT: the LUT is a CPU step between its stages (`plan::Step::Cpu`), checked against the CPU render.

## In the editor

Basic → Profile → **LUT**: the `.cube` files in the LUT folder (in the app's settings folder; "Open the LUT folder…" makes it), or "Choose a .cube file…" anywhere. **LUT amount** below it.

## Not yet

- The path is stored as it is, so a sidecar moved to another computer needs the file at the same place.
- No input or output colour space choice (log or Rec.2020 LUTs look wrong); sRGB is assumed.
- No tetrahedral interpolation; LUTs in presets aren't shared through the hub.
