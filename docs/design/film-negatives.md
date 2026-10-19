# Film negatives

Status: v1 implemented (`crates/amb-engine/src/negative.rs`). It turns a camera scan of a colour or black-and-white negative into a positive, with an auto mode, built-in film stock looks and full manual control. Everything is settable from presets and sidecars.

Lightroom has no negative conversion; photographers buy the Negative Lab Pro plug-in for it. So these settings live in **our own XMP namespace**: `amb:` = `http://ns.ambrotype.org/develop/1.0/`. The URI changes with the product name before the first public release (see legal.md). Lightroom ignores the tags. Whether it keeps them when it rewrites a sidecar (Save Metadata) is still to be checked in the Lightroom trial.

## Settings

| Tag | What it does |
|---|---|
| `amb:NegativeConvert` | Turn the conversion on. |
| `amb:NegativeType` | `color` (default), `bw`, or `positive` for slide film: the same model on log transmittance instead of density, so nothing is inverted; there is no film base, and the levels take out the light's colour (`amb negative analyze --slides`). |
| `amb:NegativeAuto` | **Auto** (default True) estimates film base, levels and midtone for every photo. False uses the stored values below, which keeps a roll consistent; missing values are still estimated. |
| `amb:FilmStock`, `amb:FilmStockAmount` | The look and its strength (0–100). `amb negative stocks` lists them. |
| `amb:FilmBaseRed/Green/Blue` | Film base (orange mask) transmittance. |
| `amb:NegativeDensityLow*/High*` | Per-channel shadow and highlight densities above the base (the levels). |
| `amb:NegativeMidtone` | Which point of the range becomes 18 % grey. |
| `amb:NegativeExposure` | Print exposure in EV. |
| `amb:NegativeContrast` | Paper grade, ±100. |
| `amb:NegativeCyanRed`, `amb:NegativeMagentaGreen`, `amb:NegativeYellowBlue` | Colour balance, ±100. |
| `amb:NegativeToe`, `amb:NegativeShoulder` | Shadow and highlight roll-off, ±100. |
| `amb:NegativeSaturation` | Saturation, ±100. |

After the conversion, every regular setting works on the **positive**. Temperature/Tint, Exposure, the tone curve, HSL and colour grading behave normally, which avoids the inverted-slider confusion of converting in Lightroom. White balance reads 5500 K / 0 for the balanced conversion and shifts relative to that.

## How it works (model v1, ours)

On camera RGB, after Lens Corrections and before white balance:

1. **Film base:** divide by the base colour, so unexposed film becomes 1, and take density D = −log10(transmittance).
   - Auto takes the median of the brightest 0.3 % of unclipped samples: the rebate between frames if it's in the scan, else the thinnest shadows.
2. **Levels:** per channel, map D_low → 0 (deepest shadow) and D_high → 1 (brightest highlight).
   - Auto uses the 0.2 % and 99.8 % percentiles, ignoring a 5 % border for the film holder.
   - This removes the orange mask's colour and balances the dye layers. It assumes the deepest shadows and brightest highlights are close to neutral, which is true of most scenes; when it isn't (a sunset roll), freeze good values from another frame.
3. **Film stock look**, in these normalised log units:
   - per-channel contrast (colour crossover)
   - dye crosstalk
   - shadow and highlight tints
   - saturation
   - or a black-and-white channel mix

   It is blended by the stock amount.
4. **Back to scene-linear:** the midtone lands on 18 % grey, over a scene range of 2.2 decades × contrast. Toe and shoulder compress the ends.

Based on the published idea of darktable's `negadoctor` (density, Dmin, paper response), written from scratch. All constants are in the `negative` section of `amb calibration defaults`.

## Film stocks

The stocks are our own looks:
- **Colour:** Neutral, Warm Portrait 400, Pastel Portrait 160, Vivid 100, Golden 200, Everyday 400, Cinema Tungsten 500T, Cinema Daylight 50D.
- **B&W:** Classic 400, Fine 100, High Contrast, Red Filter, Orthochromatic.

They are described by character and **named generically on purpose**: no manufacturer trademarks, and no claim to match a specific film. Measured stock profiles (from scans of real film shot with a reference target) could be added later as data.

## Workflow

```sh
amb testimage negative neg.dng                       # a synthetic scanned negative to try it on
amb render scan.dng out.jpg --preset auto-neg.xmp    # preset with amb:NegativeConvert=True (+ amb:FilmStock)
amb negative analyze frame01.nef frame02.nef … --out roll.xmp   # the "Auto" button, frozen: roll median
amb render frame05.nef out.jpg --preset roll.xmp     # same base/levels for the whole roll
amb negative stocks                                  # list the looks
```

- `amb edit` / the AI protocol set any `amb:` setting like a Camera Raw one, with validation and clamping (`set_param amb:FilmStock vivid-100`).
- In the editor, Film Negative → **Crop to frame** crops away the rebate and the holder's edges (`negative::detect_frame`: border is near the film base, nearly black or clipped; each side moves in while most of its row or column is border, plus 0.5 % for the soft edge). **Freeze** stores the open frame's analysis with Auto off (one undo step). In the library, **Sync negative roll** analyses the selected frames and gives them all the roll median, as `amb negative analyze` does (`amb_library::negative`).

## Next

- Scan quality helpers: dust and scratch removal (infrared channel where available).
- Halation and grain matched to the stock.
- Measured stock profiles, and a way for users to share them as data.
