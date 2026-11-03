# What's next

A short running list, kept up to date as work lands. Newest decisions first. The full roadmap and its tracks are in [NEXT.md](NEXT.md); what's missing is in [GAPS.md](GAPS.md). Projects we researched, and when we last checked them, are in [research/watchlist.md](research/watchlist.md).

## Waiting on the owner

1. **v0.1.0:** try the editor on a Mac and once on Windows (the list is in NEXT.md's v0.1 checklist), then say go: tag, check the draft release, publish, and point the README at it.
2. **Auto Density and Auto Grade on real negatives:** try both switches (Negative panel, beside Auto) on a roll. If they look right, they become Auto's default.
3. **GPU speed:** export a 24 MP raw on the Mac with and without `AMB_GPU=off` (both are tiled or whole as the GPU's memory allows).
4. **Your sample raw** to replace the stand-in sample photo, after the release.

## Up next (in this order)

1. **A paper curve in density** (Film lab item 9).
2. **Film looks tied to stocks:** halation, bloom and grain amounts per film stock, so picking a stock brings its glow and grain.
3. **Convert raws on more cameras:** Canon and Fujifilm next (Nikon and Sony done).
4. **Dust:** the guided rectangle and a heal brush that searches its stroke (Film lab item 5).
5. **Darkroom aids:** density and zone under the cursor, test strip, ring-around (Film lab item 7).
6. **From the competition:** AI Remove on the GPU (our own kernels) and raw AI Denoise (our own builds, after CalibRaw's ideas); open a Lightroom catalogue in place; pick the best in a burst by open eyes ([research/17](research/17-competitors-2026.md)).

## Ideas for later

- **Teach by showing** (owner, 2026-11-03): the app should teach its features by showing them, not by manuals, and one question should open up many related features. Ideas:
  - **Tips while waiting:** while something loads or runs (opening a raw, an export, Convert raws, AI Remove), one short tip about a feature, with a button to try it.
  - **Tips where text normally is:** now and then, a tip as the placeholder of a search or filter box, or in an empty menu or panel area; gone once you click or type there.
  - **One answer leads to more:** a How to shows two or three related features ("also: Keep under N MB, Also a smaller copy"), and the features list can be followed from one to the next.
  - Tips come from [FEATURES.md](FEATURES.md), skip features already used, and are rare enough not to nag.

## Recently done

- 2026-11-03: a Search button (magnifier) in the toolbar of every built-in layout

- 2026-11-03: Help → What Ambrotype Can Do: the features list in the app, by section, with a filter; each opens its How to, and search's How to and "nothing found" link to it

- 2026-11-03: Search Everything (Ctrl+K) finds what you want to do ("whatsapp", "raw by mistake") from FEATURES.md, shows where it is, with buttons for what it names

- 2026-11-03: [FEATURES.md](FEATURES.md), what the app can do by what you want to do, with where each thing is and its `amb` command (linked from the README, llms.txt and the AI guide)

- 2026-11-03: exports, Convert raws and collages default to quality 100 (was 90); Convert raws falls back to Auto when a raw's preview can't be read
- 2026-11-03: Export: Keep under N MB (JPEG; sizes to pick, 16 MB being WhatsApp's max), and Also a smaller copy, up to N MB (NAME-small.jpg beside the export, for social media; `amb render --small-copy 3M`)

- 2026-11-03: Sony raws: Convert raws uses the camera's full-size JPEG where the ARW has one (A7 IV, A7R V; it was matching the raw to the small preview), and the camera's own distortion correction is read and applied (an A6400's kit zoom now frames like its JPEG). Tried on six Sony raws

- 2026-11-03: a watchlist of the projects we researched, with when we last checked each ([research/watchlist.md](research/watchlist.md))

- 2026-11-02: AI Remove: the Spot panel's Remove fills a spot with what belongs there (Big-LaMa, Apache-2.0, an optional 208 MB download in Settings → AI models; CPU, about 10 s a spot). Lightroom's content-aware and generative spots are filled the same way

- 2026-11-02: Edit Replay: File → Export Edit Replay…, `amb replay`: an AV1 MP4 of the edit from its History, steps easing in and named (a 6 s, 1080 px replay takes about 30 s on 4 cores)

- 2026-11-02: Highlight roll-off (Effects → Film finish): highlights ease into white keeping their hue, fading like paper near the top

- 2026-11-02: Defringe → Auto: finds and removes purple, blue, green and cyan fringes by itself (with Remove Chromatic Aberration, which already measured CA without a lens profile)

- 2026-11-02: GPU exports in tiles for any size, sized from the free memory the driver reports (Vulkan, Direct3D 12, Metal)

- 2026-10-31: grain that follows density (Effects → Grain → Film response)
- 2026-10-31: filmstrip multi-select: Ctrl/Shift-click, right-click for preset, paste, export, convert
- 2026-10-31: Auto Density and Auto Grade for negatives (off by default)
- 2026-10-31: subfolders as the first tiles of the library grid; double-click opens or closes a tree folder
- 2026-10-31: Convert raws from the editor; New preset from this photo (Ctrl+Shift+N)
- 2026-10-31: halation and bloom (Effects → Glow)
- 2026-10-31: hub CI installs every submission with the app's own checks (`ambrotype --check-hub`)
- 2026-10-31: Convert raws: three at a time, best of each burst
- 2026-10-31: History values, "exposure 0.7" in Search, auto-advance after rating
- 2026-10-31: full-size exports on the GPU, with the CPU as fallback
