# Film recipes

**Film recipe** is a built-in plugin (Plugins, Ctrl+Shift+P). It gives you the settings a camera's picture menu has: a film look, grain, colour depth, white balance shift, dynamic range, highlight and shadow tone, colour, sharpness, noise reduction and clarity. You set them as you would on the camera, and the plugin turns them into ordinary develop settings. The result is one undo step, and every slider stays free to change afterwards.

People share "film recipes" for their cameras online. You can type one in here as it is written, using the table below to find our closest look.

## Our looks are ours

The 17 looks are our own, built from the tone curve, HSL, colour grading and the B&W mix. They are not copies of any camera maker's processing, and they are not measured against one. They are starting points: they go in the same direction as the camera style named in the table, not to the same place. Fitting them better is future work ([GAPS.md](../GAPS.md)).

Camera makers' names are used here only to say which of our looks to pick. They never appear in the app.

## Which look to pick

| A recipe says (camera film simulation) | Pick this look |
|---|---|
| Provia / Standard | Standard |
| Velvia / Vivid | Vivid Slide |
| Astia / Soft | Soft Slide |
| Classic Chrome | Muted Chrome |
| Reala Ace | Real Colour |
| Pro Neg. Hi | Studio Negative |
| Pro Neg. Std | Studio Negative Soft |
| Classic Neg. | Street Negative |
| Nostalgic Neg. | Amber Negative |
| Eterna / Cinema | Cine Flat |
| Eterna Bleach Bypass | Bleach Bypass |
| Monochrome (+Ye, +R, +G filter) | Mono (Mono Yellow, Mono Red, Mono Green) |
| Acros (any filter) | Fine Mono |
| Sepia | Sepia |

## The other settings

| A recipe says | Set | Notes |
|---|---|---|
| Grain Effect: Weak / Strong, Small / Large | Grain, Grain size | |
| Color Chrome Effect | Colour depth | Deeper, darker saturated colours |
| Color Chrome FX Blue | Blue depth | Deeper blues and skies |
| White Balance (Auto, Daylight, Cloudy, Shade, Tungsten, Fluorescent, a Kelvin value) | White balance, Kelvin | "Auto" is "As shot" here |
| WB shift, e.g. "+2 Red, −4 Blue" | WB shift: red, WB shift: blue | Same −9..+9 grid |
| Dynamic Range: DR100 / DR200 / DR400 | Dynamic range | DR-Auto: use DR200 |
| Highlight, Shadow (tone curve) | Highlight tone, Shadow tone | −2..+4, half steps allowed |
| Color | Colour | Ignored by the monochrome looks |
| Sharpness, Noise Reduction (High ISO NR), Clarity | Sharpness, Noise reduction, Clarity | |
| Monochromatic Color: WC / MG | Mono toning: warm / cool, magenta / green | Monochrome looks only |
| ISO, Exposure compensation | (not here) | These are how the photo was taken; adjust Exposure yourself if needed |

## How the settings map

So you can predict a result, or check it in the panels:

- **Look:** sets Contrast, Saturation, the parametric curve, HSL and colour grading. The monochrome looks turn on Black & White (`ConvertToGrayscale`) with their own B&W mix.
- **Dynamic range:** DR200 lowers Highlights by 20 and Whites by 5; DR400 lowers Highlights by 40 and Whites by 10, and lifts Shadows by 5.
- **Highlight / shadow tone:** each step moves the parametric curve's Highlights or Shadows by 8 and Lights or Darks by 3.
- **Colour:** Saturation +6 and Vibrance +2 per step.
- **Sharpness:** 40 + 10 per step. **Noise reduction:** Luminance 10 + 2.5 per step. **Clarity:** 6 per step on top of the look.
- **Grain:** amount 25 (Weak) or 45 (Strong); size 15 (Small) or 35 (Large).
- **Colour depth:** per step, darker red, yellow and green (−6) and orange (−3), with a little more saturation. **Blue depth:** per step, blue −8 and aqua −4 luminance, blue saturation +4.
- **White balance shift:** red-minus-blue warms in mireds (4 per step); red-plus-blue adds magenta (Tint +3 per step).
- **Mono toning:** a tint through global colour grading.

Every setting a recipe touches is set outright, so picking another recipe replaces the last one instead of adding to it.

## Recipes and presets

- **Ready-made recipes** come with the plugin: Everyday colour, Faded street, Warm afternoon, Moody film, Cinema night, Soft portrait, Gritty mono and Old print.
- **Your own:** set the parameters, type a name and press **Save recipe**. Recipes are kept in the app's settings, per plugin.
- **As a preset:** after running a recipe, press **Save as preset…** to keep the result as a one-click preset with an Amount slider. It lands under "Yours" in the Presets list and is a Lightroom-style `.xmp` file.
- **Command line:** `amb plugin run plugins/film-recipe photo.nef --params "look=Muted Chrome,grain=Weak,colour_depth=Strong,highlight=-1,shadow=1"`.

## Trademarks

Fujifilm, Provia, Velvia, Astia, Classic Chrome, Reala Ace, Pro Neg., Classic Neg., Nostalgic Neg., Eterna and Acros are trademarks of FUJIFILM Corporation. They are named here only to describe which of our looks is closest in style. Ambrotype is not affiliated with or endorsed by FUJIFILM, and its looks are not FUJIFILM film simulations.
