# Competitors in 2026: what they do, what to take, and our niche

Read 2026-10-30 from each project's README or website, not their code. Numbers (stars, prices) are as of that day.

## The field

| | What | Licence, price | Stack, platforms | Size |
|---|---|---|---|---|
| **LightCraft** ([repo](https://github.com/storytold/lightcraft)) | Lightroom rebuilt in pure Rust, alpha | MIT or Apache-2.0, free | Own raw decoders, wgpu, egui; Windows, macOS, Linux, FreeBSD, browser (WebAssembly) | ~15k stars, 833 open PRs |
| **Rembrandt** ([repo](https://github.com/thesnarkitecht/rembrandt)) | Lightroom Classic-style editor with on-device AI | GPL-3, free | JS + WebGL2/WebGPU in Tauri, LibRaw; desktop and self-hosted browser | ~190 stars |
| **Redlamp** ([repo](https://github.com/pdcgomes/redlamp)) | Raw developer, darkroom-minded, alpha | MPL-2.0, free | Swift, Metal, Core ML; Apple Silicon Macs only | ~350 stars |
| **FlashView** ([issues](https://github.com/merlin1de/FlashView), [site](https://flashview.net)) | Fast culling browser, basic editing in beta | Closed, one-off licence after a 60-day trial | Windows only, SSD required | Small |
| **Tenzen Light Studio** ([site](https://tenzen.studio)) | Lightroom alternative in a suite of Adobe replacements | Closed, $59 once (suite $89); Photoshop and InDesign alternatives free | Windows, macOS, Linux | Commercial |
| darktable, RawTherapee, ART, digiKam | The established free editors | GPL | C/C++, all desktops | Large, mature |
| Capture One, DxO PhotoLab, ON1, Luminar Neo, Affinity (now free) | The established paid editors | Paid or freemium | Desktop | Large |

The new projects share our shape: XMP sidecars, Lightroom panel order, local AI, an MCP server. Their UIs look like ours because all of us copy Lightroom's panel order and dark neutral greys. That is convergence, not copying.

## What each does that we don't (yet)

**LightCraft**
- Agents can drive every menu item and brush stroke, not only edits; undo covers agent actions.
- Describe masks: type what to mask (SAM 3).
- Print layouts and contact sheets; slideshows; HDR export.
- An append-only operation log as the catalogue.
- A browser build, and Flatpak and AppImage packages.
- An honest coverage score in the README ("about 61 % useful today").

**Rembrandt**
- Before/after split and side-by-side, and a shareable before/after page with a slider (our split view came 2026-10-30).
- Touch: drag on the photo to change tone or colour.
- Ask: type or speak an edit ("shadows +25") with a small on-device language model.
- Smart search (CLIP on the device) and Find similar.
- AI tools: Relight, Sky, Sunrays, Super Resolution, AI Denoise, Refocus, Generative Remove and Expand, portrait retouching and face reshaping.
- Edit like me, Match my camera, Make these consistent, Pick the best (sharpness, open eyes).
- A self-hosted server to edit the library from a browser on the home network.
- Share how: a replay video of the edit.

**Redlamp**
- History shows each step's before and after values.
- The command palette sets values by typing ("exposure 0.7").
- 36 film looks from 30 stocks, modelled from datasheets, with grain, halation and bloom; some fitted to cameras' own renders.
- Focus stacking in the editor.
- Test Your Camera: users measure their camera, results go to a public page.
- Bug reports to GitHub without a GitHub account.
- A published performance card: slider render time, open time, memory, download size.
- Design rules: neutral grey surfaces, no modal dialogs while editing, undo for everything.

**FlashView**
- Speed as the product: under 50 ms between photos on folders of 100,000.
- Auto-advance after rating; batch rating with undo.
- Cloud-aware: knows when a file is online-only.
- A companion web plugin (StarRate) that collects ratings from guests or clients.

**Tenzen**
- Opens a Lightroom catalogue (`.lrcat`) in place.
- Keeps Adobe's shortcuts in every app.
- "Blocks the network": a visible privacy promise.
- A CLI and MCP server, like ours.

## Why people pay for Tenzen

Tenzen is not better at developing raws than the free projects. People pay for other things:

1. **Subscription fatigue, answered in one line.** "No subscriptions." "Any app — $59 once." The price is the headline.
2. **A whole suite.** Photoshop, Lightroom, Illustrator and InDesign replacements under one brand, two of them free. Leaving Adobe is one decision, not four.
3. **Nothing to relearn.** Adobe's shortcuts, Adobe's files (PSD, IDML, `.lrcat`) opened as they are.
4. **Trust signals.** A polished site, signed installers, a launcher that updates everything, a store listing, a clear licence FAQ.
5. **Free apps that bring people in.** The free Photoshop alternative brings people to the brand; some of them then buy Light.

What it means for us: we are free, so the lessons are 1, 3 and 4. Say "free, no account, your Lightroom edits come with you" in the first line. Ship signed installers that update themselves. Make leaving Lightroom one click.

## Our niche

Many projects now aim at "Lightroom, but free". That is not a niche. What only we have:

1. **Lightroom compatibility you can check.** Lightroom's own XMP and presets, read and written as they are, and catalogue import. Our Lightroom Classic plug-in measures Lightroom, so our sliders can match its numbers.
2. **The film lab.** Negative conversion, roll baseline by scene, dust and scratch repair, film stocks, Fuji recipes. The others have at most a dust button. Negative Lab Pro costs about $99 and needs Lightroom.
3. **Made to be changed.** Layouts and themes as files, whole custom interfaces, sandboxed plugins, and a hub to share them, with screenshots in the README since 2026-10-30.
4. **AI that edits through the same undoable commands as a person**, over MCP or the CLI, with the compact syntax and `amb stats` so it costs few tokens.

The one-line pitch: **free, open Lightroom for film shooters and tinkerers: your presets and edits come with you, and you can reshape every part of it.**

## Ideas to take, by fit

Sizes: S a day or two, M a week, L more.

**Fits our niche (do first)**
1. ~~Before/after split~~ (done 2026-10-30). S.
2. A shareable before/after page (a single HTML file with a slider). S.
3. Contact sheets and print layouts, film strips with edge print for the film lab. M.
4. Film looks with halation and bloom, modelled from datasheets (Redlamp; already Film lab item 6). M.
5. Open a Lightroom catalogue in place, not only import (Tenzen). M.
6. Before and after values in History (Redlamp). S.
7. Typing values in the command palette: "exposure 0.7" (Redlamp). S.
8. A blank-canvas layout editor: start a layout from nothing. S.

**Good for everyone**
9. Touch: drag on the photo to change tone or colour (Rembrandt; Lightroom's targeted adjustment). M.
10. Smart search with CLIP on the device, and Find similar (Rembrandt). M.
11. Pick the best in a burst by sharpness and open eyes (Rembrandt). M.
12. Auto-advance after rating; speed budgets published like Redlamp's performance card (FlashView, Redlamp). S.
13. Bug reports without a GitHub account (Redlamp). S.
14. Agents driving the whole UI, not only edits (LightCraft). M.

**Later or not ours**
- Generative fill and expand, face reshaping, Relight: large models, outside our niche.
- A self-hosted browser library (Rembrandt), a web build (LightCraft): large.
- Focus stacking (Redlamp): useful, but a niche of its own; after HDR and panorama it would fit the merge tools.
- A suite of Adobe replacements (Tenzen): not ours.

## Merging projects

Their stacks differ (Rust + egui, JS + WebGL, Swift + Metal, Rust + plain JS), so code can't be merged by a script. Features can be ported from their descriptions. Licences: Rembrandt's GPL-3 code could come in as is; LightCraft's MIT/Apache code could come in (one way only); Redlamp's MPL-2.0 files can be combined with GPL code but stay MPL. Any code taken needs the owner's yes first and a credit in `NOTICE`. Rembrandt shares our licence and our idea (XMP, local AI), so working together is worth a message.
