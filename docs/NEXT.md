# Where we are

A short "start here" for contributors and for the next work session. Details are in [ARCHITECTURE.md](ARCHITECTURE.md) (what exists) and [GAPS.md](GAPS.md) (everything missing). New to the repo, or an AI agent? Read [AGENTS.md](../AGENTS.md) first.

## State

- **Branch:** `main`; changes arrive as pull requests. About 590 tests are green.
- **Name:** **Ambrotype**. The GitHub repo is `shardpelt/Ambrotype`.
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
    - 1:1 zoom and pan (only the shown tiles are rendered, and kept while panning); right-drag a box to zoom to it, Z or Space back to the whole photo (Settings can turn the box off); the loupe (I) shows the part under the pointer at 100–400% without zooming in
    - white-balance picker
    - clipping overlay
    - point-curve editor
    - colour-grading wheels
    - saved preset folders
    - feedback button (pre-filled GitHub issue)
    - crop and straighten tool (R), with aspect lock
    - **size presets** (Instagram, Snapchat, TikTok, X, Reddit, YouTube, Pinterest, LinkedIn and print sizes, plus custom ones) for the crop and exact-size export (`amb render --exact WxH` too)
    - **appearance (looks):** the UI is drawn from a layout file (toolbar, side columns, panels, and knob/bar/slider/number per setting) and a theme (colour tokens). Built-in looks (Classic, Beginner, Compact, Big controls, Studio, Left-handed, and since 2026-10-19 Focus, Console, Two benches, and task layouts Film lab, Colour grading, Black & white, Retouch and Collage prep; Dark, Light, High contrast, Neutral grey, and Cupertino Light and Dark in the style of macOS, plus Swiss print, Telemetry, Stone and Graphite after the taste-skill design guides) can be cloned, renamed, shared and imported in the Appearance dialog. AI assistants can write them ([ai/ui-layouts.md](ai/ui-layouts.md), `ambrotype --check-look`)
    - **settings and shortcuts:** the ⚙ button lists every keyboard shortcut on a keyboard map and lets you change them (clashes are caught; only changes are stored); Shift+? or the Shortcuts button shows them all on one card
    - **search everything** (2026-10-20): Ctrl (⌘)+K, or Help → Search Everything…, finds any command, setting, panel, preset, layout or theme by name and takes you there: a command runs, a setting's panel opens and the setting is selected (= and - move it), a layout or theme is switched to. Settings the layout doesn't show are listed as such (`ui/search.js`)
    - **tasks** (2026-10-20): a bar at the top switches between Library, Develop, Color, Retouch, Film and Collage (Ctrl+Alt+1 to 6, Window → Task). Each editing task opens a layout made for that job; a task keeps the layout you pick while in it (`ui/tasks.js`)
    - **custom interfaces** (2026-10-20): Settings → General → Interface replaces the whole window with your own HTML, CSS and JavaScript (a folder with an `index.html`), written by hand or by an AI agent, calling the same commands as the built-in one. The app asks before switching; the menu bar's Interface → Back to the Built-in Interface and `--builtin-ui` always come back. Shared through the hub like plugins ([ai/custom-interface.md](ai/custom-interface.md), example in `examples/interfaces/minimal/`)
    - **resizable columns and a preset choice** (2026-10-21): drag a side column's inner edge to resize it (kept per layout on this computer; double-click for the layout's width; narrow columns stack each setting's name over its slider). Presets → "Apply to the original, not on top": one choice for all presets; on, a preset replaces the look (crop, spots, masks, geometry and a negative's conversion stay); off, presets add up as in Lightroom
    - **connect an AI app** (2026-10-20): Help → Connect an AI App… gives any MCP app (Claude, Codex, Gemini, Cursor, VS Code, LM Studio, or any other) the command and setup to edit with Ambrotype on the user's own plan, and opens a terminal in the photo's folder with `amb` ready for command-line agents ([ai/mcp.md](ai/mcp.md))
    - **menu bar** (2026-10-19): on macOS, the system's menu bar at the top of the screen has every command by name (File, Edit, Photo, Library, View, Window, Help) with its current keys, greyed out where it can't run now. Windows and Linux can show it in the window (Settings → General)
    - **filmstrip:** the other photos in the folder under the photo; ← → or a click opens them, saving edits first
    - **rating, colour labels and keywords:** stars (keys 1–5), labels (6–9) and a keyword field, saved to the sidecar at once and embedded in exports
    - **library** (G): a folder as a grid; sort, filter and search; select many to rate, label and keyword at once; copy one photo's edit and paste it onto others (by setting group); export the selection; start a collage with it ([design/library.md](design/library.md))
    - **hub:** the toolbar's Browse button (Ctrl+Shift+H; also Appearance → Browse hub…) installs shared layouts, themes, develop presets, collage templates, size profiles, plugins and custom interfaces (checked against the index's sha256 and validated; plugins also from the Plugins dialog); Submit to hub… opens GitHub with your look filled in as a pull request; a layout or theme can also be installed from a link (a file, or a GitHub repo with `theme.json` or `layout.json`), and a theme needs only `bg`, `text` and `accent` (the rest are worked out, 2026-10-15)
    - **editor mode:** Edit layout rearranges the screen in place (drag and drop, control types, split and merge, add and remove, column widths) and saves it as your own layout
    - **borders** on export, with presets (White, White with a black line, Polaroid and more) and colour swatches, and a caption line such as the camera and exposure; share a border as a file or through the hub
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
    - **Learn** (F1): guided lessons from a data file (Getting around, Editing basics, Coming from Lightroom); the card points at each control, switches task when a step is about another, and moves on when you've done the step
    - **Surprise me:** six random looks as thumbnails (Random look plugin, by style and strength); click one to apply it as one undo step
    - **Auto:** Basic's Auto button sets the six tone sliders from the photo; WB → Auto takes a cast out of near-grey areas (or keeps the camera's when there's too little grey); `amb auto` too
    - **match look:** Presets → Match look of other photos… picks reference photos, finds the white balance, tone, Vibrance/Saturation, Color Grading and Color Mixer settings that give this photo their look, shows them on the photo, and applies them as one undo step (`amb match` and MCP `match_look` too)
    - **panorama merge:** select overlapping raws in the library, Merge to Panorama… (projection, Auto crop, lens corrections first, Auto tone) writes `<first>-Pano.dng` next to them; `amb merge pano` from the command line ([design/panorama.md](design/panorama.md))
    - **collage maker** (C): layouts with gap, outer border, corners and background; drag photos into cells, then move, zoom, rotate, or edit them in the editor; **arrange freely** (move, resize, turn and overlap frames by hand); save the collage as a `.ambcollage` file to reopen and edit; save and share layouts; batch a whole folder into pages ([design/collage-and-borders.md](design/collage-and-borders.md))

## Help wanted

Things anyone with the right kit can do; open an issue first so two people don't do the same one.

- **Try the editor on a Mac or Windows machine** and report what feels wrong (the Feedback button opens a pre-filled issue). Most testing so far is on Linux.
- **Run `amb preset census <your preset folders>`** and share the output (setting names and counts only, no preset files). It ranks what to build next.
- **Lightroom samples:** if you have Lightroom, the measurements in [lightroom-trial-plan.md](lightroom-trial-plan.md) let us fit our models (sweep kit: `amb sweep generate`). Share only fitted numbers, never Adobe's renders or files.
- **Film simulation references:** raws plus in-camera re-renders with each film simulation, so the Film recipe looks can be fitted ([design/film-look-measurement.md](design/film-look-measurement.md)).
- **Creative and Adobe Raw profiles** (`Look`, `RGBTable`) need two facts: the 85-character alphabet the DNG SDK uses for `Table_<md5>` text, and the names of the `crs:` properties that hold table fingerprints in an Adobe Raw profile `.xmp`. Facts only; copy no table data.

## Next steps

### Pinned: teach by showing, not by cards (owner, 2026-10-20)

Card tips get skipped. Learn's lessons stay in Help → Learn, but first-run teaching should show, not tell. Done on 2026-10-22 (`ui/teach.js`):

1. ~~**A sample photo that teaches through its History.**~~ The empty screen's **Try a sample photo** (and Help → Open the sample photo) downloads a stand-in raw (the CC0 D750 still life from raw.pixls.us, 18 MB, checked) with a nine-step edit. Clicking a History step (or Ctrl+Z) lights up the controls it changed and slides them to their value. With History focused (clicked, or Tab) the arrow keys step through it and Home/End go to the ends. Left: swap in the owner's photo below (a new `data/sample/` sidecar, URL and checksum in `commands/sample.rs`).
2. ~~**Hold ⌘/Ctrl to see every shortcut**~~ on the toolbar buttons and tasks (not on sliders: they have no keys).
3. ~~**Empty states that teach:**~~ the library says "drop a folder here", and a folder dropped anywhere opens in it; on an unedited photo Exposure, Highlights, Shadows, Temp and Vibrance glow faintly until the first edit.
4. ~~**Task bar hover previews:**~~ a sketch of the task's columns and panels (open ones highlighted), with its key.
5. ~~**Watch mode:**~~ Help → Watch an edit (or the empty screen's button) opens the sample afresh, goes back to Original, and a ghost cursor replays its nine steps in about 20 seconds, each named beside it. Any key, click or scroll stops it; the rest stays in History.
6. ~~**A "?" by every setting**~~ (`ui/help.js`): hover for a line on what it does, click for a card: how it shows in a photo, when to raise it, when to lower it, its range and start value. The longer text (`coach` in `crates/amb-protocol/data/descriptions.json`) is written for 78 settings and the 32 colour-band sliders (HSL and B&W, from templates): every panel but Film Negative and the defringe hue ranges; the rest show their description.
7. ~~**Photography basics**~~ (Help → Photography basics, Shift+F1; `ui/basics.js`, text in `ui/basics.json`): an eight-step guide to how a photo is made and edited, and 26 terms. In a "?" card, terms such as *midtones* or *clipping* are underlined faintly: hover for a line, click to open the guide there. Nothing shows unless asked for, so it stays out of a pro's way. Tone and colour terms show **on your own photo** (`ui/zones.js`): hovering *midtones*, *shadows*, *highlights*, *clipping* or *saturation*, or the "?" of Highlights, Shadows, Whites, Blacks, Saturation or Vibrance, lights up those pixels on the preview, dims the rest and says what share of the photo they are. Drawings (`ui/figures.js`, SVG in the theme's colours) show what a photo can't: aperture sizes, depth of field, shutter blur, ISO noise, stops, the histogram's zones, clipping, contrast, dynamic range, the kelvin scale, saturation, hue, luminance, halos, vignettes and crop shapes; six of the eight chapters have one. Left: more terms, terms in Learn's lessons, translations.

There was no automatic "Getting around" card to retire: Learn only opens on F1 or the empty screen's button.

**The sample raw (the owner can provide it).** One photo that needs every basic slider:
- a bright sky and deep shadows in one frame, about 1 stop under (Exposure, Highlights, Shadows, Whites/Blacks)
- a visible colour cast to fix: warm indoor light, or blue shade (Temp/Tint), with something white or grey in it
- muted and saturated colours side by side (Vibrance vs Saturation), e.g. foliage and a painted door
- a slightly tilted horizon or vertical (Crop/Straighten); a dust spot is a bonus (Spot removal)
- no identifiable people, logos or private places; GPS stripped
- your own shot, shared as CC0 (or CC BY); DNG preferred (NEF/CR3/ARW fine), under 30 MB

### First release (v0.1) checklist

State on 2026-10-20 (evening): the repo is public and CI runs. Linux, macOS and the licence check are green. Windows found two real bugs, both fixed: NaN pixels in the GPU colour stage (`b0d17dd`), and CRLF checkouts breaking the hub checksums (`.gitattributes`, `2e61191`). That last fix still needs a green Windows run.

- [x] Make `shardpelt/Ambrotype` public
- [x] CI green on Windows (x64, ARM): green since `74d455d` (2026-10-22): plugins no longer call into the app (it crashed on x64), and the GPU tests take turns (ARM)
- [x] Run the `Release` workflow by hand once: green on every platform since `2bf5a58` (2026-10-22; Macs unsigned, Intel Macs built on Apple Silicon). Installing each one is the hand tests below
- [x] `strip = true` in `[profile.release]` (about 60 MB smaller installs; downloads shrink less)
- [ ] Try the editor on the Mac: menu bar (and Interface → Back to the Built-in Interface), library, a custom interface, Connect an AI App… → Open a terminal here, export
- [ ] Try it once on Windows (same list; the terminal opens with `cmd`)
- [ ] Hub history clean-up: the hub's first 8 of 12 commits carry a model name in their Co-Authored-By trailer. Rewrite only those lines to `Co-Authored-By: Claude <noreply@anthropic.com>` (git filter-repo, owner-approved force-push with a lease on `ba3a0c8`; tree `6e6320d` must not change). The step-by-step hand-off was a local, untracked file in the cloud session; ask for it to be written again if it's gone. Then re-sync `data/hub/SOURCE`
- [x] Release notes: `docs/release-notes/v0.1.0.md` (the draft release starts with it). Read it before publishing
- [ ] After publishing: README's "no installers yet" becomes a link to the release
- [ ] Tag `v0.1.0`, check the draft release, publish

After the release: the owner's sample raw (spec above) replaces the stand-in.

Hub repo to-do: the app installs borders from `borders/*.json` (kind `border`), but the hub's `tools/build_index.py` doesn't index that folder yet. Add it there, with White, White with a black line and Polaroid to start.

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
5. ~~**Spot removal:** heal and clone~~ (done 2026-10-10; [design/spot-removal.md](design/spot-removal.md)). **To do (owner, 2026-10-19): blemish removal with the brush isn't good enough yet;** see GAPS. Red eye waits for a Lightroom sidecar: the `RedEyeInfo` format isn't documented anywhere we can read, and a guess could break Lightroom's files.
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

### Photo manager track (owner, 2026-10-19)

Make Ambrotype a great place to browse, sort and manage a very large photo collection, not only to edit. It builds on the library ([design/library.md](design/library.md)): sidecars stay the source of truth and the catalogue stays a cache that can be thrown away. Ideas, roughly in order:

1. **Scale** (started 2026-10-19: library folders, `amb catalog add`/`index`/`folders`; 2026-10-20: the editor's Folders panel with counts, background index, offline photos listed in grey with their thumbnails. the grid draws only what's on screen, and the thumbnail cache stays within 2 GB): index whole drives in the background (hundreds of thousands of photos), with a preview cache, a grid that only draws what's on screen, and a folder tree with counts. Photos on drives that aren't plugged in stay listed, with their thumbnails, and come back when the drive does.
2. **Ways to browse:** a timeline (years, months, days, with a scrubber), a calendar, and a map from GPS tags (photos without GPS can be placed from a GPX track or by dragging).
3. **Duplicates and bursts:** exact duplicates by file hash, near-duplicates and bursts by a picture fingerprint, grouped into stacks, with "keep the best, reject the rest".
4. **Culling help:** score sharpness, closed eyes and exposure, so the best of a burst comes first. Local models only.
5. **Search by what's in the photo:** a local image-and-text model ("dog on a beach at sunset") and optional local face grouping, both off by default and never uploaded. Models must have a licence we can use (AI models list).
6. **File care:** move, rename and delete with sidecars and JPEG pairs kept together (to the system trash), batch rename, reorganise into dated folders, a keyword tree manager, metadata templates, and "find missing photos" after files moved outside the app.
7. **Safety:** checksums kept in the catalogue to catch silent file corruption, a backup check (is every photo also on the backup drive?), and how much space each folder, year and camera takes.
8. **Several computers:** a library on a NAS or synced folder used from more than one machine, each with its own catalogue cache.
9. **Sharing:** export a static web gallery and a slideshow from a collection.
10. **Videos:** list and play the clips next to the photos, with ratings and keywords, so a card's whole import lives in one place.

### Codebase track (later, when there's budget for a long run)

0. **First, measure what helps** (plan: [design/agent-navigation.md](design/agent-navigation.md)): fresh agents on a fixed task set, with and without recipes and on-demand maps. Rounds A1 and B1 are done: with the recipes, Sonnet passed all 12 tasks at the same cost. The steps below follow from it.
1. **A map for agents:** a detailed map of the code (per crate and per UI file: what it owns, its main types and functions, how data flows from a slider to the render and to the sidecar) with recipes for common changes ("add a setting", "add a panel", "add a toolbar button", "add a mask kind", "add a hub item kind"). Builds on [ARCHITECTURE.md](ARCHITECTURE.md) and AGENTS.md; checked by a test where it can be (like the settings reference).
2. **A cleanup pass to match it:** ~~split the largest files~~ (2026-10-16: `amb-gui/src/commands/`; `ui/app.js` into preview, crop, presets and export files; the CLI by area; the engine's `pipeline/` and `detail/` as folders), one clear home per concern, consistent names, dead code out. Behaviour unchanged; the tests and a by-hand run of the editor confirm it.

## Working notes

- This repo was built with Claude Code. Large text sweeps went to smaller models, and parallel work to worktree agents that were then merged.
- **Parallel agents** need separate `CARGO_TARGET_DIR`s, or they overwrite each other's binaries. A debug `target/` grows to about 20 GB.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. On Linux the GUI needs the WebKitGTK dev packages (see desktop-setup.md).
