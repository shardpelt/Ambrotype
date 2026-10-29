# Film negatives

Status: v1 implemented (`crates/amb-engine/src/negative.rs`). It turns a camera scan of a colour or black-and-white negative into a positive, with an auto mode, built-in film stock looks and full manual control. Everything is settable from presets and sidecars.

Lightroom has no negative conversion; photographers buy the Negative Lab Pro plug-in for it. So these settings live in **our own XMP namespace**: `amb:` = `http://ns.ambrotype.org/develop/1.0/`. The URI changes with the product name before the first public release (see legal.md). Lightroom ignores the tags. Whether it keeps them when it rewrites a sidecar (Save Metadata) is still to be checked in the Lightroom trial.

## Settings

| Tag | What it does |
|---|---|
| `amb:NegativeConvert` | Turn the conversion on. |
| `amb:NegativeType` | `color` (default), `bw`, or `positive` for slide film: the same model on log transmittance instead of density, so nothing is inverted; there is no film base, and the levels take out the light's colour (`amb negative analyze --slides`). |
| `amb:NegativeAuto` | **Auto** (default True) measures the levels and midtone for every photo, inside its crop. False uses the stored levels and midtone; missing ones come from the fixed density range below, so every frame of a roll is printed the same way (a frame exposed a stop more prints a stop brighter). |
| `amb:NegativeRange` | With Auto off and no stored levels: green's density range above the base that spans the print (default 1.0, ours). Red and blue keep the share of it measured on the photo. Slides, which have no base, measure their levels instead. |
| `amb:NegativeCastRemoval` | **Cast removal**, 0–100 (default 0): moves the measured levels of red and blue so the frame's greys come out grey, read at five levels instead of assumed at the ends. Stored levels already have it (Freeze, Sync). Does nothing on black-and-white film. |
| `amb:NegativeExpired` | **Expired film**: also matches red and blue to green along the grey axis, at a shadow, a midtone and a highlight level (the crossover). Measured, or stored in `amb:NegativeCrossoverRed/Blue` (three numbers each) by Freeze and Sync. Does nothing on black-and-white film. |
| `amb:FilmStock`, `amb:FilmStockAmount` | The look and its strength (0–100). `amb negative stocks` lists them. |
| `amb:FilmBaseRed/Green/Blue` | Film base (orange mask) transmittance. Picked on the photo (**Pick base**), stored by Freeze, or measured from the whole scan when missing, with Auto on or off. |
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
   - Measured on the whole scan, not the crop, since the rebate between frames is usually outside it. It is the median of the brightest 0.3 % of unclipped samples: the rebate if it's in the scan, else the thinnest shadows.
   - **Bare light** (2026-10-28): sprocket holes, or a gap between film and holder, can be brighter than the base without clipping. Film can't be brighter than its base, so the brightest area counts as bare light when a flat area at least 0.1 D denser in every channel lies along most of its edge (holes sit inside the rebate), and, on colour film, that area is orange against it (0.2 D denser in blue than in red). The base is then that area. A dark picture edge beside the rebate is neither all around it nor orange against it.
2. **Levels:** per channel, map D_low → 0 (deepest shadow) and D_high → 1 (brightest highlight).
   - Auto uses the 0.2 % and 99.8 % percentiles inside the crop, ignoring a 5 % border for the film holder. Crop to frame first, so the holder and rebate don't count.
   - Auto off without stored levels: D_low = 0.05 and green's D_high = 0.05 + the density range, with the midtone in the middle of the range (ours). Red and blue get the share of that range they have on the photo: on the scans we tried, blue spans only 0.2–0.5 of green's range, so a fixed share for every film turned prints yellow. The idea of a fixed range with Auto off comes from negaflow ([research/16](../research/16-film-scan-apps-survey.md)).
   - **Expired film** reads the grey axis at three levels of green (0.25, 0.5, 0.75, each ±0.15): the most common colour of the pixels there, as (red − green, blue − green). Each of red and blue then gets a curve through those points and the ends (0 and 1 stay put), so its neutral lands on green's, within ±0.18 and always rising. Faded film is usually off in an S, too dense in the shadows and too thin in the highlights, which one midtone bend can't fix.
   - This removes the orange mask's colour and balances the dye layers. It assumes the deepest shadows and brightest highlights are close to neutral, which is true of most scenes; when it isn't (a sunset, a lamp), Cast removal helps, or freeze good values from another frame.
   - **Cast removal** (2026-10-29, ours) reads the greys at five levels of green (0.15 to 0.85, each ±0.08). At each, the colours that at least 3 % of its pixels share are candidates. The greys are the straight line (a lift and a slope per layer against green) that the most pixels' candidates agree with, within 0.04: greys of one cast line up, a coloured sky sits off the line at its own level. The levels of red and blue then move by the slider's share of that line. With fewer than two agreeing levels, or greys covering under 10 % of the frame (a night scene), nothing changes. The idea of balancing along the greys comes from NegPy ([research/16](../research/16-film-scan-apps-survey.md)).
3. **Film stock look**, in these normalised log units:
   - per-channel contrast (colour crossover)
   - dye crosstalk
   - shadow and highlight tints
   - saturation
   - or a black-and-white channel mix

   It is blended by the stock amount.
4. **Back to scene-linear:** the midtone lands on 18 % grey, over a scene range of 2.2 decades × contrast. Toe and shoulder compress the ends.

Based on the published idea of darktable's `negadoctor` (density, Dmin, paper response), written from scratch. All constants are in the `negative` section of `amb calibration defaults`.

## Tried on real scans (2026-10-28)

Eight frames from the BlueNeg dataset ([research/16](../research/16-film-scan-apps-survey.md) has the source): four from a Fuji 400 roll, four from a Kodak Gold 100 roll whose blue layer has faded, scanned on a Plustek OpticFilm 8200i to 16-bit DNGs, with the lab prints of the time to compare.

- The DNGs open and convert; a 68 MP frame takes about 23 s to render from the command line.
- The scans are cropped to the picture, so there is no rebate: the base is the thinnest shadow.
- **Auto** gives a usable picture on every frame, a little cooler and more contrasty than the old prints (which have yellowed).
- **Sync negative roll** is better than Auto on the night frame (a black sky instead of grey) and keeps a roll alike.
- **Fixed range** first came out strongly yellow: one share per layer for all films was wrong. With the shares measured per photo it is usable, with a slight green-cyan tint on some daylight frames that Sync or the colour balance sliders take out.
- **Cast removal** (2026-10-29), on two frames downloaded again: on the daylight airport frame it warms the bluish Auto result toward the lab print (pink clouds); the night frame has too few greys, so it is left as it was (without that check it turned the black sky green). A synthetic sunset (greys under an orange sky) goes from a strong cast to none.
- **Expired film** first did almost nothing: one midtone bend, clamped, matching the layers' average. Matching red and blue to green at three levels takes out the blue-purple shadows and yellow-green sky of the faded roll, and leaves the intact roll almost as it was.

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
- In the editor, Film Negative → **Pick base**: click the clear film between frames (before Crop to frame, or with the crop off). Alt-click measures it from the scan again. To give a roll one base: pick it on one frame, Copy settings, then Paste settings → Film negative on the others (Auto stays on, so each frame keeps its own levels).
- In the editor, Film Negative → **Crop to frame** crops away the rebate and the holder's edges (`negative::detect_frame`: border is near the film base, nearly black or clipped; each side moves in while most of its row or column is border, plus 0.5 % for the soft edge). **Freeze** stores the analysis the open frame shows now, with Auto off (one undo step). In the library, **Sync negative roll** analyses the selected frames as Auto would (each with its crop and any picked base) and gives them all the roll median (`amb_library::negative`). `amb negative analyze` does the same for raws without sidecars.

## Next

The Film lab track in [NEXT.md](../NEXT.md) has the order. In short:

- Per-film layer shares for Auto off (now one set for every colour film).
- Scan quality helpers: straight scratches and an infrared channel for dust (specks and hairs are found since 2026-10-29: [dust.md](dust.md)).
- Halation and grain matched to the stock.
- Measured stock profiles, and a way for users to share them as data.
