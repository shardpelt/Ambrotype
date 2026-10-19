# Where we left off (2026-10-08)

A short "start here" for the next work session. Details are in [ARCHITECTURE.md](ARCHITECTURE.md) (what exists) and [GAPS.md](GAPS.md) (everything missing). New to the repo, or an AI agent? Read [AGENTS.md](../AGENTS.md) first.

## State

- **Branch:** `claude/lightroom-alternative-scope-xlnf0p`. Everything is committed and pushed; about 430 tests are green.
- **Name:** **Ambrotype**. The GitHub repo is `shardpelt/Ambrotype` (renamed from flightbroom; old links redirect).
- **Built:**
  - preset/sidecar reader and writer (`crs:` and our `amb:` tags)
  - edit protocol with undo, including grouped commands that become one undo step
  - raw engine: core Develop panels, Lens Corrections, film negatives, **crop and straighten** ([design/crop.md](design/crop.md))
  - **JPEG, PNG, TIFF and WebP:** they open like raws (colour from their ICC profile), with Lightroom's relative Temp/Tint; a JPEG next to its raw is hidden, and edits go to `photo.jpg.xmp`
  - **phone and converted DNGs:** their own corrections (lens shading gain maps, distortion, vignetting opcodes) are applied, as Lightroom does
  - **Fujifilm raws:** RAF files (compressed too) open with a proper X-Trans demosaic (Markesteijn, ported from darktable); blown highlights stay neutral instead of turning pink
  - export: JPEG/PNG/TIFF/WebP/AVIF/EXR/JPEG XL, colour spaces with ICC (AVIF: sRGB, Display P3 and Rec. 2020 by code), rating, keywords and metadata as XMP in all but EXR
  - CLI `amb`
  - desktop editor (`amb-gui`, Tauri 2) with:
    - 1:1 zoom and pan (only the shown tiles are rendered, and kept while panning)
    - white-balance picker
    - clipping overlay
    - point-curve editor
    - colour-grading wheels
    - saved preset folders
    - feedback button (pre-filled GitHub issue)
    - crop and straighten tool (R), with aspect lock
    - **size presets** (Instagram, Snapchat, TikTok, X, Reddit, YouTube, Pinterest, LinkedIn and print sizes, plus custom ones) for the crop and exact-size export (`amb render --exact WxH` too)
    - **appearance (looks):** the UI is drawn from a layout file (toolbar, side columns, panels, and knob/bar/slider/number per setting) and a theme (colour tokens). Built-in looks (Classic, Beginner, Compact, Big controls, Studio, Left-handed, and since 2026-10-19 Focus, Console, Two benches, Film lab; Dark, Light, High contrast, Neutral grey, and Cupertino Light and Dark in the style of macOS, plus Swiss print, Telemetry, Stone and Graphite after the taste-skill design guides) can be cloned, renamed, shared and imported in the Appearance dialog. AI assistants can write them ([ai/ui-layouts.md](ai/ui-layouts.md), `ambrotype --check-look`)
    - **settings and shortcuts:** the ⚙ button lists every keyboard shortcut on a keyboard map and lets you change them (clashes are caught; only changes are stored); Shift+? or the Shortcuts button shows them all on one card
    - **filmstrip:** the other photos in the folder under the photo; ← → or a click opens them, saving edits first
    - **rating, colour labels and keywords:** stars (keys 1–5), labels (6–9) and a keyword field, saved to the sidecar at once and embedded in exports
    - **library** (G): a folder as a grid; sort, filter and search; select many to rate, label and keyword at once; copy one photo's edit and paste it onto others (by setting group); export the selection; start a collage with it ([design/library.md](design/library.md))
    - **hub:** the toolbar's Browse button (Ctrl+Shift+H; also Appearance → Browse hub…) installs shared layouts, themes, develop presets, collage templates, size profiles and plugins (checked against the index's sha256 and validated; plugins also from the Plugins dialog); Submit to hub… opens GitHub with your look filled in as a pull request; a layout or theme can also be installed from a link (a file, or a GitHub repo with `theme.json` or `layout.json`), and a theme needs only `bg`, `text` and `accent` (the rest are worked out, 2026-10-15)
    - **editor mode:** Edit layout rearranges the screen in place (drag and drop, control types, split and merge, add and remove, column widths) and saves it as your own layout
    - **borders** on export, with presets and colour swatches, and a caption line such as the camera and exposure
    - **saved setups** (2026-10-15): Appearance → Save this setup… keeps the layout and theme together, as copies, to come back to after trying others; the one before the last change is kept by itself (`looks::save_setup`, files in the looks folder's `setups/`)
    - **toolbar icons** (2026-10-15): the top bar shows icons where they are obvious (our own line drawings, `ui/icons.js`); Spots, Clipping and Appearance keep their words; the **Beginner** layout shows words on every button and only the everyday tools (`"toolbar_style": "words"`); hovering shows the name and shortcut
    - **Frame** in the editor (Shift+B): preview a border in any colour with text below or on the photo, in six fonts (handwritten, pen, marker, typewriter, date stamp, sans), optionally with the photo's details (camera, mm, f-stop, shutter, ISO); it isn't part of the edit, and Export starts from it
    - **highlight recovery:** a blown sky edge, flower or sunlit wall gets its colour and texture back where at least one channel still held detail, instead of a flat white or green patch
    - **preset library** (Presets → Preset library…): 21 free starter presets by style (film, B&W, moody, bright, vivid, portrait, landscape), from the hub or built in; hover to preview on the photo, Apply, or Add to your presets
    - **soft proofing** (S): the photo as it will export to sRGB, Adobe RGB, Display P3, Rec. 2020 or ProPhoto, with a gamut warning
    - **edit in another app** (Ctrl+Alt+E): the photo, as edited, goes to your image editor as `IMG_0001-Edit.tif` next to it (ProPhoto 16-bit by default; Settings → General), and shows in the folder
    - **from Lightroom Classic:** the library's From Lightroom… (or `amb lrcat import`) moves a catalogue's edits, ratings, labels, flags, keywords, stacks, virtual copies and collections into sidecars and our collections; the catalogue is only read
    - **plugins** (Ctrl+Shift+P): sandboxed WebAssembly plugins that make edits (one undo step), with permission prompts, install from a folder, Share as pull request, and "Make one with AI…" instructions for the user's own assistant. Built in: Auto tone, Random look and **Film recipe** (a camera-style picture menu for typing in film recipes, with ready-made ones; [ai/film-recipes.md](ai/film-recipes.md)). Parameter sets can be saved as **recipes**. CLI: `amb plugin new/build/check/run` ([ai/plugins.md](ai/plugins.md))
    - **save as preset:** Presets → Save… (or Save as preset… in the Plugins dialog) keeps the photo's edit, without the crop and other photo-specific settings, as a one-click preset under "Yours"
    - **LUTs:** Basic → Profile → LUT applies a `.cube` look (from the LUT folder or any file) with an amount ([design/luts.md](design/luts.md))
    - **export presets** and Resize by long or short edge, width, height, megapixels or percent; the window opens where it was left
    - **film negatives:** Freeze keeps a frame's conversion; the library's Sync negative roll gives a roll one conversion
    - **export:** file names from a template for batches, text or logo watermarks, chroma subsampling, progressive JPEG and AVIF speed in the dialog
    - **follows outside edits:** when another app or an AI agent (over MCP) changes the open photo's sidecar, the editor shows it at once, as one undo step
    - **Guided Upright:** draw guides; the Transform sliders straighten them ([design/transform.md](design/transform.md))
    - **Black & White panel:** monochrome with a mix per colour; Color Grading tints it
    - **HDR merge:** select bracketed raws in the library, Merge to HDR… (Auto align, Auto tone, Deghost) writes `<first>-HDR.dng` next to them, a floating-point DNG that edits like a raw ([design/hdr-merge.md](design/hdr-merge.md))
    - **Learn** (F1): guided lessons from a data file (Editing basics, Coming from Lightroom); the card points at each control and moves on when you've done the step
    - **Surprise me:** six random looks as thumbnails (Random look plugin, by style and strength); click one to apply it as one undo step
    - **Auto:** Basic's Auto button sets the six tone sliders from the photo; WB → Auto takes a cast out of near-grey areas (or keeps the camera's when there's too little grey); `amb auto` too
    - **match look:** Presets → Match look of other photos… picks reference photos, finds the white balance, tone, Vibrance/Saturation, Color Grading and Color Mixer settings that give this photo their look, shows them on the photo, and applies them as one undo step (`amb match` and MCP `match_look` too)
    - **panorama merge:** select overlapping raws in the library, Merge to Panorama… (projection, Auto crop, lens corrections first, Auto tone) writes `<first>-Pano.dng` next to them; `amb merge pano` from the command line ([design/panorama.md](design/panorama.md))
    - **collage maker** (C): layouts with gap, outer border, corners and background; drag photos into cells, then move, zoom, rotate, or edit them in the editor; **arrange freely** (move, resize, turn and overlap frames by hand); save the collage as a `.ambcollage` file to reopen and edit; save and share layouts; batch a whole folder into pages ([design/collage-and-borders.md](design/collage-and-borders.md))

## Owner to-dos

- [ ] Set up a **local** Claude Code session on the Mac so it can reach your drives and Lightroom: [desktop-setup.md § Working with an AI agent locally](desktop-setup.md#working-with-an-ai-agent-locally).
- [x] Hub repo `Ambrotypehq/ambrotype-hub` filled and public (2026-10-15), and it's now where the built-in layouts, themes and starter presets are edited; the app ships a copy (`tools/sync-hub.py`). Shared plugins arrive there as pull requests to `plugins/<id>/`.

- [ ] Try the editor on the Mac: `cargo run --release -p amb-gui -- photo.nef`. Note what feels wrong (or use the Feedback button).
- [ ] Run `amb preset census <your preset folders>` and share the output (setting names and counts only). It ranks what to build next.
- [x] Digest the big source files on your own machine ([handoff-sources.md](handoff-sources.md)). Done 2026-10-11; the answers are in [adobe-docs/open-questions.md](adobe-docs/open-questions.md) and are now in the code and design docs.
- [ ] Two facts would let us render creative and Adobe Raw profiles (`Look`, `RGBTable`): the base-85 alphabet the DNG SDK uses for `Table_<md5>` text (the 85 characters, in order, from `dng_big_table.cpp`), and, from one Adobe Raw profile `.xmp` you have installed (for example Adobe Color), the names of the `crs:` properties that hold table fingerprints. Property names and an encoding alphabet are facts; copy no table data. A local session can add both to `docs/adobe-docs/dng-sdk-behaviour.md`.
- [ ] Lightroom trial month: follow [lightroom-trial-plan.md](lightroom-trial-plan.md) (sweep kit: `amb sweep generate`).
- [ ] Fujifilm shoot (later): raws plus in-camera re-renders with every film simulation, so the Film recipe looks can be fitted. What to shoot: [design/film-look-measurement.md](design/film-look-measurement.md).

## Next steps

There are two tracks that can run in parallel: the UI track is what users see and share, and the fidelity track depends on the Lightroom trial data.

### UI and sharing track (agreed 2026-10-07, in this order)

1. ~~Crop tool and size presets~~ (done 2026-10-07). Small follow-ups are in [GAPS.md](GAPS.md).
2. ~~Layout files, built-in looks and themes~~ (done 2026-10-07; [design/customization-and-hub.md](design/customization-and-hub.md), [ai/ui-layouts.md](ai/ui-layouts.md)).
3. ~~Editor mode~~ (done 2026-10-07): **Edit layout** moves panels, settings and toolbar buttons by drag and drop, changes control types, labels, titles and columns, splits and merges panels, adds from a list of what isn't shown, and resizes columns. Colours are edited in the Appearance dialog. Since 2026-10-15 it starts from Appearance → Edit layout… (or Ctrl+Shift+L); the built-in toolbars no longer have their own button for it.
4. ~~Hub~~ (done 2026-10-07, for layouts and themes; needs the hub repo, see Owner to-dos): Omarchy-style. Items are git repos, and one index repo takes submissions as pull requests. The in-app browser covers layouts, themes, presets, collage templates and size profiles.
5. ~~Borders~~ (done 2026-10-08): border presets in the export dialog and `amb render --border`, batch with `amb render A B ... --out-dir`. Was: width, colour, caption; border presets; fit to a size preset; batch.
6. ~~Collage maker~~ (done 2026-10-08): shareable layouts (size, background, gap, outer border, rounded corners, cells you can split and remove), per-photo move/zoom/rotate and "Edit photo" in the editor, batch pages for big folders (optionally only rated photos). Follow-ups (resizing cells by their edges, saving a collage to reopen, per-cell edits) are in [design/collage-and-borders.md](design/collage-and-borders.md).
7. ~~Plugins and AI-made features~~ (done 2026-10-08): WASM-sandboxed "action" plugins with permissions, "Make one with AI…" instructions for the user's own assistant, Share as pull request to the hub. ~~Batch runs~~ (from the library, 2026-10-12). ~~Pipeline effects with their own controls~~ (2026-10-15: effect plugins, colour or pixels, in the Effects panel; Duotone and Light leak built in). Next for plugins: free-form plugin panels, ~~the hub listing plugins in the app~~ (2026-10-12; [GAPS.md](GAPS.md)).

The agreed UI track is done, and the library view after it (2026-10-08; [design/library.md](design/library.md)). Next is the parity track below.

### Parity track (agreed 2026-10-08)

Another open-source Lightroom alternative, [LightCraft](https://github.com/storytold/lightcraft) (MIT/Apache, egui, its own catalogue), appeared with much more breadth. We keep our own direction (faithful Lightroom presets and sidecar round trip, customising and sharing, plugins) and build towards what it has, in this order:

1. ~~**Fujifilm raws:** proper X-Trans demosaic, neutral blown highlights~~ (done 2026-10-08).
2. ~~**JPEG, PNG, TIFF and WebP input**, with a raw and its same-name JPEG shown as one photo (as Lightroom does), and Lightroom's relative white balance for non-raw files~~ (done 2026-10-08).
3. ~~**Local Highlights and Shadows** (edge-aware tone mapping), the biggest fidelity gap (fidelity track 3)~~ (done 2026-10-08).
4. ~~**Masks:** linear, radial, brush, luminance and colour range, add/subtract/intersect; read and write Lightroom's mask tags~~ (done 2026-10-08; [design/masks.md](design/masks.md)). ~~Select Subject~~ (2026-10-15: U²-Net small, run locally, downloaded on first use). ~~Sharper subject edges~~ (2026-10-15). ~~Background and People (everyone), with the model chosen per task and the user's own ONNX models~~ (2026-10-17: Settings → AI models, `amb models`, MCP `ai_models`). Next for masks: fitting in the trial month, people and objects. ~~Reordering and duplicating masks~~ (2026-10-12).
5. ~~**Spot removal:** heal and clone~~ (done 2026-10-10; [design/spot-removal.md](design/spot-removal.md)). Red eye waits for a Lightroom sidecar: the `RedEyeInfo` format isn't documented anywhere we can read, and a guess could break Lightroom's files.
6. ~~**Transform and Upright**~~ (done 2026-10-10: the sliders, and Upright Auto, Level, Vertical and Full from the photo's lines; [design/transform.md](design/transform.md)). ~~Guided Upright~~ (2026-10-12, as Transform sliders). Next for it: fitting in the trial.
7. ~~**Library:** pick/reject flags, loupe, compare and survey views, import with copy and rename, collections, virtual copies~~ (done 2026-10-10; [design/library.md](design/library.md)). ~~Subfolders, smart collections~~ (done 2026-10-12). ~~Metadata panel~~ (title, caption, creator, copyright; 2026-10-12). ~~Stacks, several folders at once, collection sets~~ (2026-10-12). ~~Moving from a Lightroom Classic catalogue~~ (2026-10-13: `amb lrcat import`, From Lightroom…; to try on real catalogues). ~~The catalogue~~ (2026-10-15: a SQLite index of what was read from the sidecars and raw headers, so big folders open about 50× faster once seen; All photos; camera, lens, ISO and focal length in search and smart collections; `amb catalog`). Next for it: smart collections and All photos straight from the index without checking every file.
8. **Profiles, Calibration panel and Point Color** (fidelity track 4–5). ~~Calibration panel~~ (done 2026-10-10; [design/calibration.md](design/calibration.md)). Point Color waits for a Lightroom sample of `PointColors` (the trial keeps one). ~~Reading the user's own camera profiles (`.dcp`) in place~~ (done 2026-10-10; [design/camera-profiles.md](design/camera-profiles.md)). Next for profiles: Adobe Raw `.xmp` profiles, and fitted base looks of our own from the trial.
9. ~~**MCP server** for AI agents~~ (done 2026-10-10: `amb mcp`, [ai/mcp.md](ai/mcp.md)), ~~look matching~~ (done 2026-10-12: `amb match`), ~~the in-app assistant panel~~ (done 2026-10-12: [ai/assistant.md](ai/assistant.md)). ~~Showing it the photo~~ (opt-in, 2026-10-12). ~~Sky masks from the assistant~~ (2026-10-12: `sky:exp=-0.5` in the compact syntax). ~~Subject masks~~ (2026-10-15).
10. **Speed and reach:** ~~installers per OS~~ (the `Release` workflow, 2026-10-10; [releasing.md](releasing.md); to run on GitHub, then signing), ~~tiled 100% view~~ (2026-10-11), ~~HDR merge~~ (2026-10-11: library Merge to HDR… and `amb merge hdr`, a floating-point DNG; [design/hdr-merge.md](design/hdr-merge.md)), ~~panorama merge~~ (2026-10-11: library Merge to Panorama… and `amb merge pano`; [design/panorama.md](design/panorama.md)), GPU preview (2026-10-11: `amb-gpu`, used by the editor's preview; every stage after Lens Corrections, Transform and crop runs on the GPU; [design/gpu.md](design/gpu.md); every edit, with masks, spots, LUTs and camera profiles as CPU steps inside it, since 2026-10-12. Next: measure on the Mac).

### Fidelity and engine track

1. **Lightroom trial month:** capture the sweeps, then fit the model constants ([lightroom-trial-plan.md](lightroom-trial-plan.md)). Check the crop's angle sign and corner semantics there ([design/crop.md](design/crop.md) open questions).
2. ~~**Transform/Upright**~~ (done 2026-10-10); their signs, strengths and Upright's results to be checked in the trial.
3. ~~**Local Highlights/Shadows** (edge-aware)~~ (done 2026-10-08), then ~~**masks** (linear/radial/brush/range)~~ (done 2026-10-08).
4. **Camera profiles:** ~~read the user's DCPs in place~~ (done 2026-10-10; two maps blended and Exposure placed as in the DNG reference render, 2026-10-11); decode `Table_` look and RGB tables once the two facts in the owner to-dos are in; fitted base looks once trial data exists.
5. ~~**Calibration panel**~~ (done 2026-10-10), **Point Color** (the B&W mixer renders since 2026-10-08; fitting both is part of the trial). Fit the Film recipe looks to the owner's Fujifilm shots ([design/film-look-measurement.md](design/film-look-measurement.md)).
6. ~~**Token-efficient AI layer**~~ (done 2026-10-11: the core card [ai/core.md](ai/core.md) and `amb ref`, `amb do` compact syntax with net-change output, `amb edit --brief`, `amb stats`, MCP `do` and `photo_stats`; ~~the MCP server~~ 2026-10-10). ~~Look matching~~ (done 2026-10-12: `amb match`, MCP `match_look`; [design/ai-control.md](design/ai-control.md)). ~~Match look in the editor~~ (done 2026-10-12). ~~HSL in matching~~ (done 2026-10-12). ~~Assistant panel~~ (done 2026-10-12, with an opt-in picture of the photo). ~~Sky masks from the assistant~~ (2026-10-12). ~~Subject masks~~ (2026-10-15).
7. ~~**Auto**~~ (done 2026-10-12: Basic → Tone → Auto, WB → Auto, `amb auto`), ~~**Randomize**~~ (done 2026-10-12: Presets → Surprise me…), ~~**Tutorial mode**~~ (v1 done 2026-10-12: Learn, F1) ([design/future-ideas.md](design/future-ideas.md)).
8. **Performance:** measure release-build preview and 1:1 times on the Mac (`AMB_TIMING=1` prints each render stage's time). On 4 Linux cores a 24 MP render of a typical edit takes about 3.5 s: Texture/Clarity 0.75 s, colour 0.7 s, noise reduction 0.65 s, Highlights/Shadows 0.4 s, Dehaze 0.4 s. The editor's screen-size preview (860 × 570) takes about 70 ms. ~~Tiled 1:1 render~~ (done 2026-10-11: the 100% view renders only its tiles, `pipeline::render_region`; about 10× faster than the full render it replaced). ~~GPU preview~~ (done 2026-10-11: every stage after stage 0 on the GPU; [design/gpu.md](design/gpu.md)). Next: measure on the Mac.
9. ~~**Installers** per OS~~ (the `Release` workflow, 2026-10-10). Left: run it on GitHub, Windows signing (SignPath) and Apple signing secrets.

### Codebase track (later, when there's budget for a long run)

0. **First, measure what helps** (plan: [design/agent-navigation.md](design/agent-navigation.md)): fresh agents on a fixed task set, with and without recipes and on-demand maps. Rounds A1 and B1 are done: with the recipes, Sonnet passed all 12 tasks at the same cost. The steps below follow from it.
1. **A map for agents:** a detailed map of the code (per crate and per UI file: what it owns, its main types and functions, how data flows from a slider to the render and to the sidecar) with recipes for common changes ("add a setting", "add a panel", "add a toolbar button", "add a mask kind", "add a hub item kind"). Builds on [ARCHITECTURE.md](ARCHITECTURE.md) and AGENTS.md; checked by a test where it can be (like the settings reference).
2. **A cleanup pass to match it:** ~~split the largest files~~ (2026-10-16: `amb-gui/src/commands/`; `ui/app.js` into preview, crop, presets and export files; the CLI by area; the engine's `pipeline/` and `detail/` as folders), one clear home per concern, consistent names, dead code out. Behaviour unchanged; the tests and a by-hand run of the editor confirm it.

## Working notes

- This repo was built with Claude Code. Large text sweeps went to smaller models, and parallel work to worktree agents that were then merged.
- **Parallel agents** need separate `CARGO_TARGET_DIR`s, or they overwrite each other's binaries. A debug `target/` grows to about 20 GB.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. On Linux the GUI needs the WebKitGTK dev packages (see desktop-setup.md).
