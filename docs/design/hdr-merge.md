# HDR merge

Lightroom's Photo Merge > HDR combines bracketed raws of one scene into one photo with the highlights of the darker frames and the clean shadows of the brighter ones. Its options are Auto Align, Auto Tone and Deghost (None, Low, Medium, High), and the result is a floating-point DNG that edits like a raw ([research/01](../research/01-lightroom-feature-inventory.md), Photo Merge). Ours does the same.

## Using it

- **Library:** select two or more raws of one scene (the brackets), then **Merge to HDR…** in the panel. Choose Auto align, Auto tone and Deghost, then Merge. The merged photo is written next to the first one as `<first>-HDR.dng` (`-HDR-2` and so on if that name is taken) and opens in the editor.
- **CLI:** `amb merge hdr A.NEF B.NEF C.NEF [--out OUT.dng] [--no-align] [--deghost low|medium|high] [--json]`. It prints each frame's exposure in stops and its shift. For Auto Tone from the command line: `amb plugin run plugins/auto-tone OUT.dng`.

## How it works (ours, not Adobe's)

`amb_engine::merge::hdr`, on demosaiced camera RGB:

1. **Reference:** the middle exposure (by EXIF exposure, else by mean brightness). The merge keeps its brightness (white = 1), its colour matrix, white balance and metadata.
2. **Alignment** (Auto align): one translation per frame, by median threshold bitmaps (Ward, 2003). Each frame's luminance, cut at its own median, gives nearly the same bitmap whatever the exposure. The shift that best matches the bitmaps is searched coarse to fine over six levels (up to 63 pixels). Pixels within 5% of the median are left out of the count, since noise flips them.
3. **Exposure:** each frame's brightness against the reference is the median ratio of green over pixels well exposed in both (after alignment). Real shutter speeds differ from their nominal values, and a wrong ratio shows as a band where one frame takes over from another. EXIF (time × ISO / f-number²) is the fallback.
4. **Merge:** per pixel, every frame brought to the reference exposure, averaged with weights proportional to its exposure (more light, less noise). The weights fall smoothly to zero as any channel goes from 80% to 96% of its clip level. Where every frame is clipped, the darkest one is used as it is.
5. **Deghost:** where a well-exposed frame disagrees with the merge by more than the level allows (natural log of luminance: Low 0.6, Medium 0.35, High 0.2), something moved. Those areas take a single exposure: the reference, or the nearest darker frame where the reference is clipped. The map is widened and softened (box blurs of about 1/300 of the long side), so the switch doesn't show.

## The DNG

`amb_export::dng::write_linear`: one IFD with a `LinearRaw` image (3 samples per pixel) as 16-bit floats. Half precision keeps 11 significant bits over about 40 stops. The image is stored in 256-pixel tiles, compressed with Deflate after the floating-point predictor (byte planes, then horizontal differences: TIFF Technical Note 3, Predictor 3). `WhiteLevel` is 1, the floating-point default. The source's colour matrix (`ColorMatrix1`, labelled D65) and as-shot white are kept, and an EXIF IFD carries the reference's exposure, focal length and lens, so lens profiles still match. A 24 MP merge is about 85 MB, close to Lightroom's.

The encoding was checked three ways: our raw reader (rawler) reads it back; exiftool reads every tag without a warning; and a decoder written from the TIFF specification alone decodes the same values. It has not been opened in Lightroom yet (trial month).

## Speed

Three 24 MP brackets merge in about 9 s in a release build on 4 cores, including decoding them and writing the DNG.

## Open questions and gaps

- Lightroom's reference frame, weights and deghost rule are unknown; ours are a reasonable model. Compare in the trial month: merge the same brackets in both, then check brightness and colour of the merged DNG at default settings.
- Only translation is aligned, not rotation or scale (Lightroom's Auto Align handles some rotation).
- Raws only: JPEG brackets aren't merged yet.
- No "Create stack" (the library has no stacks), no HDR panorama, no panorama merge.
- Auto tone is our built-in Auto tone plugin, not Lightroom's Auto.
