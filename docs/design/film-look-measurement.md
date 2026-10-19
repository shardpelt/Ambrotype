# Measuring Fujifilm's film simulations

Status: planned (2026-10-08). It needs someone with a Fujifilm camera to make the shots (see Help wanted in [NEXT.md](../NEXT.md)).

## Goal

Make the Film recipe looks ([ai/film-recipes.md](../ai/film-recipes.md)) match Fujifilm's film simulations closely, by measuring them: same raw, rendered by the camera with each simulation, then fit our numbers to the camera's JPEGs. Today the looks are tuned by eye and only go in the right direction.

## What is allowed

We measure the camera's output, as we do for Lightroom ([legal.md](../legal.md) §1): run the camera normally, look at its JPEGs, and fit our own numbers. A colour look isn't protected; only the files that carry it are. So:

- **Yes:** a photographer's own raws and the JPEGs their camera makes from them; fitting our settings or our own lookup tables to them; publishing the fitted numbers.
- **Never:** firmware, X RAW STUDIO's or any app's internals, the Fujifilm camera-matching profiles inside Lightroom or Capture One, or anyone else's LUTs. Never commit the raws or the camera JPEGs.
- **Names:** Fujifilm's simulation names stay out of the app; docs may say which of our looks is closest.

## What to shoot

One shoot gives everything: raw files plus camera JPEGs of the same frames.

1. **Camera setup:** RAW + JPEG (Fine). White balance fixed to Daylight, not Auto, for the chart shots. Dynamic range DR100. Highlight 0, Shadow 0, Color 0, Sharpness 0, Clarity 0, noise reduction at its lowest (−4). Grain, colour chrome effect and colour chrome blue off. Lens corrections as the camera does them (we compare in the middle of the frame).
2. **Colour chart:** a 24-patch colour chart (ColorChecker Classic or Passport, or a good clone), in open shade or overcast daylight, filling about a third of the frame, square to the camera. Three exposures: −1, 0 and +1 EV. If there's no chart, skip it; the scenes below still work, just less precisely.
3. **Scenes:** five to ten ordinary photos that cover what people care about: a face (skin), foliage, blue sky, a high-contrast street scene, a warm indoor or tungsten scene, and a night scene. Keep them on a tripod or fast shutter so the JPEG and raw match.
4. **Every simulation:** in the camera, use **Playback → RAW conversion** (or Fujifilm's X RAW STUDIO) to render each raw again with every film simulation, all other settings as in step 1. Name the files after the simulation, e.g. `DSCF1234_classic-chrome.jpg`.
5. **Each menu setting on its own:** with one simulation (Provia/Standard) and one chart raw, render once per setting step: Highlight −2 to +4, Shadow −2 to +4, Color −4 to +4, colour chrome effect Weak/Strong, colour chrome blue Weak/Strong, Grain Weak/Strong and Small/Large, DR200 and DR400, and a few white balance shifts (R+4, R−4, B+4, B−4). This fits the recipe settings, not just the looks.
6. **Monochrome:** Acros and Monochrome with the Yellow, Red and Green filters, and the warm/cool and magenta/green toning at ±4.

That's about 20 simulations × 10 raws plus 40 setting steps: a few hundred JPEGs, made in the camera in an evening. Keep them in one folder per raw.

## How we fit

1. **Line up the images.** Camera JPEGs are sRGB and have their own sharpening, so compare small (about 1000 px), and the chart by patch averages. The camera's lens correction can shift the framing slightly; a quick alignment handles that.
2. **The base first.** Fit Provia/Standard as a camera profile for the camera body used: our base rendering against the camera's. Everything else is fitted on top of it, so a simulation is "Provia plus this".
3. **Phase 1, our existing settings:** for each simulation, search Contrast, the parametric curve, HSL, colour grading and (for monochrome) the B&W mix to minimise the colour difference (ΔE 2000) between our render and the camera JPEG, over the chart patches and a grid of samples from the scenes. The results replace the hand-tuned numbers in `plugins/film-recipe/src/lib.rs`. Report the mean and worst ΔE per look, before and after.
4. **Phase 2, a lookup table:** a 3D LUT stage in the engine (also needed for camera profiles and Lightroom's creative profiles; see [GAPS.md](../GAPS.md)). Fit one table per simulation from the same data; these are our own measurements and can ship as data files.
5. **The menu settings:** fit the Highlight/Shadow tone steps, Color steps, colour chrome effect and the white balance shift grid from step 5 of the shoot, and update the mapping in [ai/film-recipes.md](../ai/film-recipes.md).

The fitting is a CLI tool (planned as `amb look fit`), so others with other Fujifilm bodies can run it on their own shots and share the fitted numbers, never the images.

## Open questions

- How much simulations differ between sensor generations (X-Trans III, IV, 5). Shots from a second body would show it.
- Whether "Provia on a Fujifilm raw" can be matched well enough on other brands' raws, which start from different sensor colour. Expect close, not exact.
- Grain: Fujifilm's grain is applied in the camera's processing; we fit amount and size by eye against our grain, not exactly.
