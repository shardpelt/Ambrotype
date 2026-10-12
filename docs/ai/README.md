# Driving Ambrotype from an AI or a script

Ambrotype edits photos the way Lightroom does: as a non-destructive recipe of Camera Raw settings (`crs:` tags such as `Exposure2012` or `ColorGradeShadowHue`) stored in an XMP sidecar next to the raw file. Features Lightroom lacks (e.g. film negative conversion) use our own `amb:` tags (e.g. `amb:NegativeConvert`, `amb:FilmStock`), which work with the same commands. An AI assistant edits a photo by changing those settings through the **same validated, undoable commands** a human's slider moves produce. It never paints pixels.

This guide covers what works **today, through the `amb` command-line tool**. The same commands are available to your own AI app through the MCP server, `amb mcp` ([mcp.md](mcp.md)). An in-app assistant panel is planned (see [the design doc](../design/ai-control.md)).

Related files:
- [core.md](core.md): the **core card**, a few hundred tokens: the compact edit syntax and the most used settings with their ranges. Read it first. `amb ref` prints it; `amb ref GROUP` prints one group's settings, one line each.
- [settings-reference.md](settings-reference.md): every setting, with its range, default, Ambrotype render status and a description. It is generated from the code (`amb settings reference`, or `--json` for machine use).
- `amb protocol schema`: JSON Schema of the commands and queries below.
- [ui-layouts.md](ui-layouts.md): making editor layouts and themes ("looks") for the user, e.g. "knobs instead of sliders, a warm theme".
- [plugins.md](plugins.md): making a plugin for the user ("a button that warms skin tones"): sandboxed WebAssembly that returns these same commands.
- [film-recipes.md](film-recipes.md): turning a camera film recipe the user found online into settings with the Film recipe plugin.
- [/llms.txt](../../llms.txt): index for language models.

## The short way (fewest tokens)

```sh
amb ref                                         # the core card
amb stats IMG_0001.NEF                          # numbers instead of a picture
amb do IMG_0001.NEF "exp=+0.4 con+=15 hl=-30" --dry-run
amb do IMG_0001.NEF "exp=+0.4 con+=15 hl=-30"
```

`amb do` takes the photo (or its `.xmp`) and a compact line. It is one undoable step, like `amb edit`, and prints only the net changes:

```
Exposure2012 0→+0.4
Contrast2012 0→+15
Highlights2012 0→-30
saved IMG_0001.xmp
```

| Item | Command |
|---|---|
| `Name=v` | `set_param` (a number, `true`/`false`, or text; quote text with spaces: `CameraProfile="Camera Vivid"`) |
| `Name+=d`, `Name-=d` | `adjust_param` |
| `reset:Name`, `reset:group` | `reset_param`, `reset_group` |
| `preset:PATH[@amount]` | `apply_preset` |
| `snap:NAME`, `load:NAME` | `save_snapshot`, `apply_snapshot` |
| `undo`, `redo` | `undo`, `redo` |

Names are the `crs:` names in any case, or the short names on the core card (`exp`, `con`, `hl`, `sh`, ...). Masks, spots and point curves need the JSON commands below. `amb edit --brief` gives the same short report for JSON commands.

`amb stats` develops the photo small (with its edit, or `--preset`) and prints a few lines: CIE L* percentiles, the share of clipped shadows and highlights, the midtone colour cast (a* positive is magenta, b* positive is warm) and mean chroma. Use it to check an edit; render a picture only when the numbers can't answer.

```
684x460
L* p1 26 p5 27 p25 31 p50 78 p75 89 p95 94 p99 97, mean 65
clipped: shadows 0.0%, highlights 19.1% (R 19.1 G 0.0 B 0.0)
midtone cast a* +9.8 b* +10.2 (warm, magenta); chroma 32
```

## Auto

`amb auto IMG_0001.NEF` shows what Lightroom-style Auto would set: the six tone sliders (from zero, so the result doesn't depend on where they were) and the white balance (marked `Auto`). `--tone` or `--wb` for one of them, `--apply` to make it one undo step. White balance changes only when near-grey covers at least 15% of the photo; otherwise the camera's is kept.

## Matching a look

"Make this look like these photos": `amb match` finds the settings for you, in code, instead of trying sliders one by one.

```sh
amb match IMG_0001.NEF ref1.jpg ref2.jpg              # show the changes
amb match IMG_0001.NEF ref1.jpg ref2.jpg --apply      # apply them as one undoable step
amb match IMG_0001.NEF ref.jpg --parts wb,tone        # only white balance and tone
amb match IMG_0001.NEF ref.jpg --out look.xmp         # also save them as a preset
```

References can be JPEGs (or PNG, TIFF, WebP) or raws, developed with their own edits. It changes white balance, Exposure, Contrast, Highlights, Shadows, Whites, Blacks, Vibrance, Saturation, Color Grading's shadow and highlight tints and the Color Mixer's hue, saturation and luminance per colour (`--parts` wb, tone, color, grading, hsl). The Color Mixer only moves colours that both photos have, and leaves out moves under 5. It measures the look (lightness spread, the colour of shadows, midtones and highlights, colourfulness), not the content, so references of other scenes work. It prints how close it got (`look distance 778 → 2.4`). Review the changes with the user before `--apply`, then fine-tune with `amb do`.

## Safety rules (read first)

These come from [docs/design/ai-control.md](../design/ai-control.md). An assistant driving Ambrotype must follow them.

1. **Optional.** Never assume the user wants AI help; only act when asked.
2. **Only the user's controls.** Change settings with `amb edit` commands. Never modify image pixels, never write raw files, never edit sidecars by hand.
3. **Preview, then apply.** Run `amb edit ... --dry-run` first and show the user the diff ("8 settings changed: ..."). Write only after they accept. Render a preview (`amb render`) when the change is visual.
4. **Ask before anything big or irreversible:** batch edits over many photos, overwriting files you did not create, deleting or moving files, and **sending any image to a cloud model**. Images stay on the machine unless the user picks a cloud model and confirms.
5. **Stay within a step and cost budget** per request, and tell the user what it is.
6. **Keep a visible log.** Show the commands you ran and their arguments (the JSON you passed to `amb edit` is the log).
7. **No AI in correctness paths.** Rendering, preset import and catalog integrity never depend on a model; your job is to orchestrate and explain.

Practical consequences:
- `amb edit` is a transaction: if any command in the list fails, **nothing** is written. Fix the command and rerun the whole list.
- When unsure, write to a new file with `--out` instead of overwriting the sidecar.
- `amb edit` keeps everything in the sidecar that is not a Camera Raw setting (rating, labels, keywords, EXIF copies, history) and writes atomically. With `--out` to another `.xmp`, that metadata is copied from the source sidecar (an existing file at the `--out` path is replaced, not merged).

## The loop: inspect, edit, render, iterate

### 1. Inspect

```sh
amb preset inspect IMG_0001.xmp            # human-readable summary, grouped
amb preset inspect IMG_0001.xmp --json     # same, as JSON (tag, group, value, render status)
amb preset dump IMG_0001.xmp               # the parsed settings as JSON
amb query IMG_0001.xmp --queries q.json    # protocol queries (below), JSON answers
```

A setting that is **absent** from the sidecar means "untouched": its default applies (Exposure 0, Contrast 0, ...). White balance is special: with no `WhiteBalance` tag (or `"As Shot"`) the camera's recorded white balance is used, and that value lives in the raw, not the sidecar. `amb render` prints it (`as shot: 5150 K, tint +4`).

### 2. Edit

Write the commands to a file (or pass `-` and pipe them on stdin):

```json
[
  {"cmd": "set_param", "name": "Exposure2012", "value": 0.4},
  {"cmd": "adjust_param", "name": "Contrast2012", "delta": 15},
  {"cmd": "set_param", "name": "ToneCurvePV2012", "value": [[0, 18], [128, 130], [255, 250]]}
]
```

```sh
amb edit IMG_0001.xmp --commands cmds.json --dry-run     # show the diff, write nothing
amb edit IMG_0001.xmp --commands cmds.json               # apply and write the sidecar
amb edit IMG_0001.xmp --commands cmds.json --out alt.xmp # write elsewhere
amb edit IMG_0001.xmp --commands cmds.json --json        # machine-readable diff
```

Text output:

```
set_param Exposure2012
  Exposure2012: +0.65 -> +0.4
adjust_param Contrast2012 +15
  Contrast2012: (unset) -> +15
set_param ToneCurvePV2012
  ToneCurvePV2012: (unset) -> [(0, 18) (128, 130) (255, 250)]
wrote IMG_0001.xmp (3 change(s))
```

If the sidecar does not exist yet, `amb edit` starts from an empty one and creates it. It writes only `.xmp` or `.json` files and refuses anything else (it will never write into a DNG or JPEG).

### 3. Render a preview

```sh
amb render IMG_0001.CR3 /tmp/preview.jpg --preset IMG_0001.xmp
```

`render` reads the settings from `--preset` (sidecars work too), prints the as-shot white balance and timing, and lists the settings present that the engine **does not render yet**. Those are still saved and Lightroom will honour them, but the preview will not show them. The **Status** column of the [settings reference](settings-reference.md) shows what renders. If you can look at images, compare the preview with the user's intent or reference photos and iterate. Keep the step budget in mind.

### 4. Presets

Apply a preset (`.xmp`, `.lrtemplate`, or a DNG/JPEG with embedded settings) as an undoable command, optionally at reduced strength or for some groups only:

```json
[{"cmd": "apply_preset", "path": "presets/Warm Matte.xmp", "amount": 60, "exclude_groups": ["white_balance", "crop"]}]
```

- `amount` is a percentage, 0-200 (default 100). Numeric settings move linearly from the current value (or default) towards the preset's value, and are clamped to their range; the clamping is reported, as for `set_param`. Temperature and Tint have no fixed default, so when the photo has no value for them they are copied **unscaled** (the result carries a note). Non-numeric settings (curves, profiles, masks) are taken from the preset whenever amount > 0. Auto, lens corrections and geometry (Transform, Upright, crop) are never scaled: they apply at the preset's values at any amount, even 0, as in Lightroom. Adobe does not document its Amount maths; this is our current model.
- Preset values are validated like `set_param` values: out-of-range numbers are clamped (and reported), and values of the wrong kind (text on a slider, an unknown white balance, an enum out of range) are skipped with a note; the rest of the preset still applies.
- `include_groups` / `exclude_groups` take group names from the reference (`tone`, `hsl`, `color_grading`, `white_balance`, ...).
- The preset's own metadata (name, UUID, group, `SupportsAmount`, writer version) is never copied into the photo.

For a quick look without editing anything: `amb render IMG.CR3 out.jpg --preset look.xmp --amount 50`. To see what a preset contains: `amb preset inspect look.xmp`.

### Undo across calls

The history is kept in the sidecar, so `undo` and `redo` work across calls and sessions (`amb do IMG_0001.NEF undo`), unless another app changed the edit since.

Every diff also reports the `before` values, so you can always revert by setting them back.

## Commands

Every command returns a diff: a list of `{name, before, after}` where `null` means unset. Commands are JSON objects tagged by `cmd`:

| Command | Fields | Notes |
|---|---|---|
| `set_param` | `name`, `value` | Numbers are clamped to the range and the clamping is reported. Booleans for switches, strings for text/enum settings, `[[input, output], ...]` (0-255) for point curves. |
| `adjust_param` | `name`, `delta` | Numeric settings only. Starts from the current value, or the default when unset. Temperature has no default, so set it absolutely. |
| `reset_param` | `name` | Removes the setting so the default applies. |
| `reset_group` | `group` | Removes every setting of a group, e.g. `"hsl"`. |
| `apply_preset` | `path`, `amount`?, `include_groups`?, `exclude_groups`? | See above. |
| `add_mask` | `kind`, `mask`?, `mode`?, `values`? | A linear, radial, luminance or color mask; with `mask`, a new part of that mask (`mode` add, subtract, intersect). See [Masks](#masks). |
| `set_mask` | `mask`, `part`?, `values` | A mask's local sliders, `name`, `active`, `amount`; with `part`, that part's geometry, `mode`, `invert`. |
| `paint_mask` | `points`, `mask`?, `part`?, `mode`?, `size`?, `feather`?, `flow`?, `density`?, `erase`? | A brush stroke; without `part`, a new brush part (a new mask without `mask`). |
| `remove_mask` | `mask`, `part`? | A mask, or one part. |
| `duplicate_mask` | `mask` | A copy just after it. |
| `move_mask` | `mask`, `to` | A new position in the list. |
| `add_spot` | `at`, `radius`?, `kind`?, `source`?, `feather`?, `opacity`? | A round heal (default) or clone spot; see [Spot removal](#spot-removal). |
| `set_spot` | `spot`, `values` | `at`, `radius`, `source` (null to search again), `kind`, `feather`, `opacity`, `active`. |
| `remove_spot` | `spot` | |
| `save_snapshot` | `name` | Keeps the current settings as a named snapshot, stored with the photo (replaces one of that name). Not an edit: no undo step. Save one before trying a different direction. |
| `apply_snapshot` | `name` | Sets every setting to the snapshot's, as one undoable step. |
| `rename_snapshot` | `name`, `to` | |
| `delete_snapshot` | `name` | |
| `undo`, `redo` | | One step = one command. The history is kept in the sidecar, so `undo` also reaches steps from earlier sessions (unless another app changed the edit since). |

Validation:
- **Unknown names are rejected** with suggestions: `"Temp"` gives `unknown setting "Temp" (did you mean Temperature, IncrementalTemperature?)`. Some short names *are* valid legacy PV2010 tags (`Exposure`, `Contrast`, `Shadows`, `Clarity`). Using them works, but the result carries a note pointing to the current tag.
- **Wrong kinds are rejected** (e.g. a string for a slider).
- **Side effects match Lightroom.** Setting `Temperature` or `Tint` while white balance is As Shot/Auto also sets `WhiteBalance` to `"Custom"`, in the same undo step; otherwise the new temperature would be ignored. Lightroom keeps the as-shot value of the other slider, but that value lives in the raw file, so **set both**, Temperature first: `Tint` alone under As Shot/Auto is rejected (a custom white balance without a temperature is not rendered), and `Temperature` alone gets a note because the tint becomes 0 instead of the as-shot tint. `amb render` prints the as-shot pair (`as shot: 5150 K, tint +4`).
- Use the current-process tags (`Exposure2012`, `Contrast2012`, `Highlights2012`, `Shadows2012`, `Whites2012`, `Blacks2012`, `Clarity2012`), not the legacy ones.

## Queries

`amb query <sidecar> --queries <file|->` takes one query or an array, tagged by `query`:

| Query | Fields | Returns |
|---|---|---|
| `get_param` | `name` | value, default, effective value, range, kind, group, label |
| `describe` | `name` | the above plus description, units, render status |
| `list_params` | `group`?, `only_set`? | without `group`: the settings present; with `group`: every setting of the group |
| `list_groups` | | all groups with labels, descriptions, tag counts and how many are set |
| `list_masks` | | every mask: name, amount, active, local sliders (UI units), parts with kind, mode, invert and geometry |
| `list_spots` | | every spot: kind, position, radius, source, feather, opacity |
| `list_snapshots` | | every snapshot: name, number of settings, and whether it matches the current settings |

```json
[{"query": "get_param", "name": "Temperature"}, {"query": "list_params", "group": "color_grading"}]
```

## Masks

Masks are Lightroom's own (`MaskGroupBasedCorrections`), so they open in Lightroom. Positions are **fractions of the uncropped photo in its stored (sensor) orientation**: (0, 0) is the top-left of the raw as the camera stored it, before rotation. `amb render` says when a photo is stored turned or mirrored (`stored turned left (shown turned 90° clockwise)`); then convert positions, or check the result on a render. Masks and parts are numbered from 0, as `list_masks` shows them.

```json
[
  {"cmd": "add_mask", "kind": "linear", "values": {"FullX": 0.5, "FullY": 0.0, "ZeroX": 0.5, "ZeroY": 0.45}},
  {"cmd": "set_mask", "mask": 0, "values": {"name": "Sky", "Exposure": -0.7, "Highlights": -40, "Temp": -10}},
  {"cmd": "add_mask", "kind": "luminance", "mask": 0, "mode": "intersect", "values": {"LumRange": [0.4, 0.55, 1, 1]}},
  {"cmd": "add_mask", "kind": "radial", "values": {"Top": 0.3, "Left": 0.35, "Bottom": 0.8, "Right": 0.65, "Feather": 70}},
  {"cmd": "set_mask", "mask": 1, "values": {"Exposure": 0.3, "Clarity": 15}}
]
```

- **Linear:** no effect at `ZeroX/ZeroY`, full at `FullX/FullY` (above: full at the top, fading out at 45% down).
- **Radial:** the ellipse's box (`Top`, `Left`, `Bottom`, `Right`), `Angle` in degrees, `Feather` 0-100; the effect is inside unless `"invert": true`.
- **Luminance:** `LumRange` [none below, full from, full to, none above], 0-1 brightness.
- **Color:** `ColorSamples` [[x, y], ...] positions of colours to select, `ColorAmount` (refine) 0-100.
- **Brush:** `{"cmd": "paint_mask", "points": [[0.2, 0.8], [0.4, 0.82]], "size": 0.04}` paints a new brush mask (`size` is the radius as a fraction of the photo's width); add `"mask": 0, "part": 2` to paint more into a brush part, `"erase": true` to erase.
- **Sliders** (`set_mask` without `part`) are in UI units, by label or tag: Exposure (EV, -4..4), Contrast, Highlights, Shadows, Whites, Blacks, Temp, Tint, Texture, Clarity, Dehaze, Saturation, Sharpness, Noise (-100..100); "Color hue" (0-360) and "Color amount" (0-100) tint; `amount` 0-100 scales them all.
- AI masks (subject, sky, people) from Lightroom are kept and reported as not rendered; they can't be made here yet.

## Spot removal

Spots are Lightroom's own `RetouchAreas`. Positions are fractions of the uncropped photo in stored orientation, like masks; `radius` is a fraction of the photo's width (0.01 is 1% of it).

```json
[
  {"cmd": "add_spot", "at": [0.31, 0.12], "radius": 0.012},
  {"cmd": "add_spot", "at": [0.62, 0.48], "radius": 0.02, "kind": "clone", "source": [0.66, 0.48]}
]
```

A heal copies from the source and matches the surroundings; a clone copies as is. Without `source` the engine picks a donor nearby each time it renders; render and look, and give an explicit `source` if it picked badly. Spots Lightroom filled with content-aware or generative fill are healed from a nearby area here, and reported.

## Importing photos

`amb import SOURCE DEST` copies photos from a card or folder into dated folders (`--into "{yyyy}/{yyyy}-{mm}-{dd}"`, the default), optionally renamed (`--rename "{yyyy}{mm}{dd}_{seq}"`), with `--keywords "a, b"` for all. A raw's JPEG and sidecar move with it; photos already there are skipped. Run it with `--dry-run` first and show the user the plan; `--json` gives the plan or the report as JSON.

## Intent to settings cheat sheet

These are **starting points**. Apply them with `--dry-run`, render, look, and adjust. Use smaller values for subtle requests and larger ones for "much more". Values are Camera Raw slider units; check ranges in the [reference](settings-reference.md). Settings marked *(planned)* are saved and Lightroom honours them, but Ambrotype's preview may not render them yet (see the Status column).

| Intent | Settings |
|---|---|
| **Warmer** | Temperature +300 to +800 K above the current value. If the photo is As Shot, read the as-shot pair from `amb render` and `set_param` Temperature to the sum, then Tint to the as-shot tint. Optional: Tint +3 to +6 on top. Warm the highlights only with ColorGradeHighlightHue 40-50, ColorGradeHighlightSat 10-20. |
| **Cooler** | Temperature -300 to -800 K below the current value (As Shot: as for Warmer, set Temperature then the as-shot Tint). Cool shadows only: ColorGradeShadowHue 200-220, ColorGradeShadowSat 10-20. |
| **Green or magenta cast** | Green cast: Tint +5 to +15. Magenta cast: Tint -5 to -15. If the photo is As Shot, first `set_param` Temperature to the as-shot temperature from `amb render`, then Tint to the as-shot tint plus the correction. Cast only in the shadows: ShadowTint *(planned)*. |
| **Brighter** | Exposure2012 +0.3 to +1.0. If the sky or highlights then clip: Highlights2012 -20 to -50, Whites2012 -5 to -15. |
| **Darker, moodier** | Exposure2012 -0.3 to -0.7, Highlights2012 -20 to -40, Blacks2012 -5 to -15, Vibrance -5 to -15. Optional vignette: PostCropVignetteAmount -15 to -30. |
| **More contrast without crushing blacks** | Contrast2012 +15 to +30, Blacks2012 +5 to +15 (lifts the black point), Shadows2012 +10 to +20, Highlights2012 -10 to -25. Or a gentle S point curve: ToneCurvePV2012 `[[0,0],[64,56],[192,200],[255,255]]`. Midtone punch: Clarity2012 +10 to +20. |
| **Less contrast, softer** | Contrast2012 -10 to -25, Highlights2012 -10, Shadows2012 +10. Dreamy: Clarity2012 -10 to -30, Texture -10 to -20. |
| **Recover a bright sky** | Highlights2012 -40 to -80, Whites2012 -10 to -20, LuminanceAdjustmentBlue -10 to -25, SaturationAdjustmentBlue +5 to +15. Dehaze +10 to +20. |
| **Open up shadows, backlit subject** | Shadows2012 +30 to +60, Blacks2012 +5 to +10, Exposure2012 +0.2 to +0.4, Highlights2012 -20 to -40. |
| **Matte / faded film** | Lift the black point and dim the white point with the curve: ToneCurvePV2012 `[[0,30],[64,70],[192,195],[255,245]]`. Contrast2012 -10 to -20, Saturation -10 to -20, Vibrance +5. Optional cool-shadow tint: ColorGradeShadowHue 210, ColorGradeShadowSat 8. Grain: GrainAmount 15-25, GrainSize 25-35. |
| **Moody teal-orange** | Shadows teal, highlights orange: ColorGradeShadowHue 190-200, ColorGradeShadowSat 20-35, ColorGradeHighlightHue 35-45, ColorGradeHighlightSat 15-25, ColorGradeBlending 50-70. Push greens towards muted teal: HueAdjustmentGreen +20 to +40, SaturationAdjustmentGreen -20 to -40. Deepen blues: SaturationAdjustmentBlue +10 to +20, LuminanceAdjustmentBlue -10 to -20. Mood: Exposure2012 -0.2, Highlights2012 -30, Blacks2012 -10, Saturation -5 to -10, Vibrance +10 to +20. |
| **Brighter, cleaner skin** | Skin lives mostly in the orange band (and some red): LuminanceAdjustmentOrange +10 to +25, LuminanceAdjustmentRed +5. Too orange: SaturationAdjustmentOrange -5 to -15. Raise colour with Vibrance rather than Saturation: Adobe's Vibrance protects skin tones. Smoother skin: Texture -10 to -25. |
| **Punchy landscape** | Vibrance +20 to +35, Highlights2012 -40, Shadows2012 +30, Whites2012 +10, Blacks2012 -10, SaturationAdjustmentBlue +10, LuminanceAdjustmentBlue -10. Clarity2012 +10 to +25 and Dehaze +5 to +15. |
| **Muted, desaturated** | Saturation -15 to -30 with Vibrance 0 to +5 (keeps some colour in muted areas), or Vibrance -20 to -40 for a gentler effect. |
| **Black & white** | ConvertToGrayscale `true` *(planned)*. Darker sky: GrayMixerBlue -20 to -40. Lighter skin: GrayMixerOrange +10 to +20. Then Contrast2012 +15 to +30. |
| **Vignette** | PostCropVignetteAmount -10 to -30 (darker corners), PostCropVignetteFeather 50-80, PostCropVignetteMidpoint 30-50. |
| **Sharper for screen** | Sharpness 40-60, SharpenRadius 1.0, SharpenDetail 25, SharpenEdgeMasking 30-60 (more for portraits, so smooth areas stay smooth). |
| **Less noise (high ISO)** | LuminanceSmoothing 20-40, keep ColorNoiseReduction at 25 or raise it to 40. |
| **Undo a look** | `reset_group` on the groups it touched (`color_grading`, `hsl`, `tone_curve`, ...), or `reset_param` per setting. |

### Worked example

User: *"Make it moodier, teal and orange, but keep the faces natural."*

```json
[
  {"cmd": "adjust_param", "name": "Exposure2012", "delta": -0.2},
  {"cmd": "set_param", "name": "Highlights2012", "value": -30},
  {"cmd": "set_param", "name": "Blacks2012", "value": -10},
  {"cmd": "set_param", "name": "ColorGradeShadowHue", "value": 195},
  {"cmd": "set_param", "name": "ColorGradeShadowSat", "value": 28},
  {"cmd": "set_param", "name": "ColorGradeHighlightHue", "value": 40},
  {"cmd": "set_param", "name": "ColorGradeHighlightSat", "value": 18},
  {"cmd": "set_param", "name": "ColorGradeBlending", "value": 60},
  {"cmd": "set_param", "name": "HueAdjustmentGreen", "value": 30},
  {"cmd": "set_param", "name": "SaturationAdjustmentGreen", "value": -30},
  {"cmd": "set_param", "name": "SaturationAdjustmentOrange", "value": -8},
  {"cmd": "set_param", "name": "Vibrance", "value": 12}
]
```

1. Run it with `amb edit IMG.xmp --commands moody.json --dry-run`. Show the user the 12 changes and why (orange saturation pulled back slightly so the warm highlights do not over-saturate skin).
2. After they accept, run it without `--dry-run`, then `amb render IMG.CR3 /tmp/preview.jpg --preset IMG.xmp`.
3. Iterate with small `adjust_param` steps. Stop at the step budget and summarise what you changed.
