# Lightroom trial: what to do, step by step

A month of Lightroom Classic is our one chance to gather **ground truth**: Lightroom's renders of known pictures at known settings, and the facts of its interface. The renders stay usable after the subscription ends, so the month is for batch capture, not for exploring.

**Rule 1: build the kit before subscribing.** Each day of the month should be spent mostly waiting on exports.

Most of the month is two clicks per batch: **Import** a kit folder, then **Export** it with a saved export preset. The pictures come with settings already attached (an `.xmp` sidecar per file, or settings inside a DNG), so Lightroom applies them on import. Only part 4 is done by hand.

## Can Claude click through it?

Yes, for the parts done by hand, using computer use in the **Claude desktop app** on the computer that runs Lightroom:
- Turn on Settings → This computer → Computer use in the app (it's off by default), then ask in a chat there.
- In a claude.ai chat in a browser, the composer's + menu may offer Devices: pick your computer there before sending.
- You approve each app it may use. Lightroom gets full access; a browser only read access.
- Claude sees the screen and clicks like a person, so it's slow. It suits part 4 (reading slider ranges, drawing a mask, saving a sidecar), not thousands of exports.

The batches don't need it: two clicks each. A Lightroom Classic plug-in of our own could do even those, and read every slider's exact range ("Automating" below).

## Short version

1. Before: build `amb`, run the kit script, shoot the own-photos list, set up a drive.
2. Day 1, on the free 7-day trial: install, write down versions, set preferences, make the export presets, run **0-check** and check the result.
3. Then subscribe and run the batches: **1a, 1b, 2, 3, 4**, about a day of exports in all.
4. By hand: interface facts, Auto, preset Amount, profiles, masks, red eye, Point Color, Upright, our `amb:` tags.
5. Copy the exports to two drives, then cancel.

## 1. Before you subscribe

### The kit

On the computer that will run Lightroom, from the repo:

```sh
cargo build --release -p amb-cli
tools/lightroom-kit/make-kit.sh /Volumes/Kit/lr-kit
```

It downloads ten CC0 raws from raw.pixls.us (about 260 MB, checked against their SHA-256) and writes one folder per session. On Windows, run it in Git Bash.

- **Put the kit on an APFS (Mac) or NTFS (Windows) drive, not exFAT.** The raws in a session are then hard links to the ten downloads and cost no space. On exFAT every step is a full copy: 2-real alone would be about 150 GB.
- Each folder has a `manifest.csv` listing every file with its settings. Keep it; the fitting reads it.

| Folder | Pictures | Files | Kit size |
|---|---|---:|---:|
| `0-check` | The Sony foliage raw and the synthetic grey ramp, the tone sliders only | 134 | 0.6 GB |
| `1a-synthetic` | Synthetic grey ramp, hue × saturation grid and colour chart, every global slider | 1,845 | about 16 GB |
| `1b-synthetic-detail` | Synthetic edge and frequency pattern: Texture, Clarity, Dehaze, sharpening, noise reduction, grain | 83 | 1.5 GB |
| `2-real` | All ten raws, every global slider | 6,150 | links only |
| `3-real-detail` | The Nikon still life and the Canon chart in snow: the detail sliders | 166 | links only |
| `4-lens` | All ten raws: distortion, vignetting, lens profile amounts, defringe | 470 | links only |

**The ten raws** (`tools/lightroom-kit/raws.tsv`, all CC0):

| File | Camera | Shows |
|---|---|---|
| 2414.ARW | Sony A7 III | Bare trees and a pine, overcast: foliage, fine branches |
| 2469.PEF | Pentax K-1 | River, mountains, blue sky with clouds |
| 3151.ARW | Sony A6400 | Sea with two ships, grey sky |
| 4650.CR3 | Canon EOS 90D | Town roofs under a dull sky |
| 3581.NEF | Nikon Z 6 | Indoor still life in tungsten light |
| 2783.RAF | Fujifilm X-T3 | Saturated product shot (X-Trans) |
| 4662.CR3 | Canon EOS R6 | Terracotta figures: oranges, skin-like tones |
| 983.CR2 | Canon EOS 5D Mark IV | Colour chart in snow, blue sky |
| 1516.RW2 | Panasonic GH5 | Colour chart and grey scale on wood |
| 1993.ORF | Olympus E-M1 II | Colour chart held in a hand, dim light |

### Your own photos (shoot these yourself)

The CC0 set has no portrait, night or backlit scene, and some tests need a particular shot. Raw files from your camera (CR2/CR3/NEF/ARW/RAF..., not DNG), in a folder `5-own/`:

| Shot | For |
|---|---|
| A portrait in daylight (yourself is fine) | Skin in profiles and presets |
| A grey card or plain wall, evenly lit | Mask falloff |
| A portrait with flash and red eyes; a pet's eyes with flash | How Lightroom stores red eye and pet eye |
| A night street with lights | Highlights, noise |
| A backlit scene (into the sun) | Highlights, Dehaze |
| A building shot tilted upwards; a straight horizon taken crooked | Upright and Transform |
| A brick wall or tiles, square on, with each lens you own | Lens profiles and distortion |
| Three brackets, ±2 EV, on a tripod; one more set handheld | HDR merge |
| Any raw from your camera with a negative's sidecar from `amb negative analyze` next to it | Whether Lightroom keeps our `amb:` tags |

### The drive

- Keep Lightroom's catalogue and Camera Raw cache on the fastest internal drive.
- With two external drives, use one for the kit and one for exports, so reads and writes don't fight.
- The exports come to about 80 GB (table in part 3). Keep a copy of them on a second drive at the end.
- **On a Mac:** keep it plugged in and awake during long exports (`caffeinate -dis`). See [desktop-setup.md](desktop-setup.md#macos-on-apple-silicon-m1-and-later-with-an-external-ssd).

### Which plan

- Start with Adobe's **7-day free trial** to run 0-check.
- Then the **monthly, no-commitment** plan. "Annual, billed monthly" charges a fee to cancel early. Check the terms when you subscribe.

## 2. Day 1: set up and check

1. **Versions.** Help → System Info: write down the Lightroom Classic version and Camera Raw version. In Develop, Settings → Process Version: write down the current one.
2. **A new catalogue** for the measurements only: File → New Catalog, on the internal drive.
3. **Preferences** (Lightroom Classic → Settings on a Mac, Edit → Preferences on Windows):
   - Presets: Raw Defaults = Adobe Default; no auto tone.
   - Performance: Use Graphics Processor = Auto (write down what it says).
4. **Catalog Settings → Metadata:** turn **off** "Automatically write changes into XMP", so Lightroom never rewrites the kit's sidecars.
5. **Import settings,** used for every folder: Add (not Copy), Build Previews = Minimal, no smart previews, Develop Settings = None, Metadata = None, no keywords.
6. **Three export presets.** In File → Export, set these, then Add (bottom left) to save each by name:

   | Preset | Format | Size | Used for |
   |---|---|---|---|
   | `amb full TIFF16` | TIFF, ZIP compression, ProPhoto RGB, 16 bits | Not resized | 0-check, 1a, 1b, 3, own photos |
   | `amb 1024 TIFF16` | the same | Long edge 1024 px | 2-real |
   | `amb full JPEG` | JPEG, quality 95, sRGB | Not resized | 4-lens |

   In all three: Rename To = Filename (the names must match `manifest.csv`), Output Sharpening off, Metadata = All Metadata, no watermark, After Export = Do nothing, export to a folder named after the session (for example `lr-out/0-check`).
7. **Run 0-check.** Import `0-check`, select all (Ctrl/⌘+A), Export with `amb full TIFF16`.
8. **Check it.** In Develop, open `2414__Exposure2012_p1.ARW`: Exposure should read +1.00. Open `ramp__Contrast2012_m100.dng`: Contrast should read −100. If they read 0, the sidecars weren't read: stop and ask.
9. Tell Claude where `lr-out/0-check` is, or run `amb render` on the same files to compare. We confirm the kit works before you subscribe.

## 3. The batches

For each folder: Import it, select all, Export with its preset into `lr-out/<folder>`. One folder at a time; remove it from the catalogue (select all, Remove) before the next, so the catalogue stays small.

| Folder | Export preset | Exports | Export size | Note |
|---|---|---:|---:|---|
| `1a-synthetic` | `amb full TIFF16` | 1,845 | about 18 GB | Small pictures; quick |
| `1b-synthetic-detail` | `amb full TIFF16` | 83 | about 2 GB | |
| `2-real` | `amb 1024 TIFF16` | 6,150 | 15–25 GB | The longest batch: a few hours |
| `3-real-detail` | `amb full TIFF16` | 166 | about 25 GB | Full size, so sharpening and noise can be measured |
| `4-lens` | `amb full JPEG` | 470 | about 7 GB | Before exporting, note which raws Lightroom found a lens profile for (Lens Corrections → Profile) |
| `5-own` | `amb full TIFF16` | your shots | | At default settings, as a baseline |

Not in the kit yet (to add before the month): the slider pairs that work together (Highlights × Whites, Shadows × Blacks, Contrast × Exposure, Vibrance × Saturation, Texture × Clarity).

## 4. By hand

Save every sidecar asked for: select the photo, Metadata → Save Metadata to File, and keep the `.xmp`. Keep screenshots in `lr-out/ui/`.

1. **Every slider's range, default and step.** For each slider in each Develop panel: the lowest and highest value, the default, how far one arrow-key press moves it, and what double-clicking does. A table or screenshots. Adobe's documentation leaves most of these out.
2. **Auto.** On each of the ten raws, press Auto (Basic panel) and save the sidecar. That shows what Auto writes.
3. **Preset Amount.** Apply five to ten presets at Amount 0, 25, 50, 75, 100, 150 and 200; export each. It measures Adobe's undocumented Amount maths.
4. **Profiles.** Render the three chart raws and three real raws with Adobe Color, Standard, Neutral, Landscape, Portrait, Vivid, Camera Matching and a few creative profiles, at several Amounts.
5. **Calibration.** On the chart raws: each primary's Hue and Saturation at −100, −50, +50 and +100, and Shadow Tint ([design/calibration.md](design/calibration.md)).
6. **Point Color.** One photo with two or three Point Color swatches; save the sidecar. It shows how `PointColors` is stored.
7. **Masks** ([design/masks.md](design/masks.md) open questions):
   - On a flat grey picture (a grey card or plain wall, evenly lit, from your camera): a linear and a radial gradient at a few known Feather values, to measure the falloff.
   - Luminance and colour range masks on the ramp and the hue grid.
   - Each local slider at +50 beside its global one.
   - A radial turned +30°, and a short brush stroke with part erased; save the sidecar.
8. **Red eye and pet eye** on your flash portraits; save the sidecar. It shows how `RedEyeInfo` is stored.
9. **Upright** on the building shot: each mode (Auto, Level, Vertical, Full), saving the sidecar each time. Then each Transform slider at −100, −50, +50 and +100 on the brick wall ([design/transform.md](design/transform.md)).
10. **HDR.** Merge your brackets with Photo Merge → HDR (Auto Tone off). Also merge them with `amb merge hdr` and import our DNG. Export both at default settings ([design/hdr-merge.md](design/hdr-merge.md)).
11. **Our `amb:` tags.** Import the raw with the negative's sidecar, change one slider, Metadata → Save Metadata to File, and check whether the `amb:` lines are still in the `.xmp`.
12. **Your presets.** Apply every preset you own to the ten raws and export with `amb 1024 TIFF16`. This becomes the end-to-end test of preset rendering.

## 5. Finish

1. Re-run anything that failed.
2. Check each `lr-out/<folder>` has as many files as its `manifest.csv` has rows.
3. Copy `lr-out` to two drives.
4. Cancel the subscription.

The fitting (matching our sliders to these renders) runs afterwards on your computer, offline, from `lr-out` and the manifests. Only fitted numbers go into the repo.

## Automating (options, not decided)

1. **A Lightroom Classic plug-in** written by us with the Classic SDK (Lua). It could import the ten raws once and apply each kit sidecar in turn before exporting, with no copies, and step through masks. It could also read every slider's exact range (`LrDevelopController.getRange`) instead of part 4.1 by hand. It runs on your trial, offline, with your own files. The SDK documentation is copyrighted ("for informational use"): read it for names and calls, copy none of it.
2. **The cloud Lightroom API** (`lr.adobe.io`). Upload a raw once, put a kit sidecar on it (`PUT …/xmp/develop`), ask for a rendition, download, repeat. Limits:
   - It needs an Adobe developer key (partner access may need Adobe's approval) and a signed-in subscription.
   - It uploads the test raws to Adobe (fine for CC0 ones).
   - Its output is 8-bit JPEG from the cloud engine, which may differ from Classic's.

   Check Adobe's developer terms first.
3. **Claude with computer use** for part 4 (above).

## Storage and legal

- Keep all renders in a **private** place, never in the public repo. They come from your licensed use of Lightroom. Only CC0 inputs and our fitted numbers are committed.
- **Black-box measurement only.** No decompiling, and no copying Adobe profile or lens files into the project ([research/05 §18.5](research/05-processing-algorithms.md)).
