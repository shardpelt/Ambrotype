# Where we are

A short "start here" for contributors and for the next work session. The running short list of what comes next, and what waits on the owner, is [WHATS-NEXT.md](WHATS-NEXT.md). Details are in [ARCHITECTURE.md](ARCHITECTURE.md) (what exists) and [GAPS.md](GAPS.md) (everything missing). New to the repo, or an AI agent? Read [AGENTS.md](../AGENTS.md) first.

## State

- **Branch:** `main`. About 735 Rust tests and 3 UI test scripts (89 steps) are green.
- **Name:** **Ambrotype**. The GitHub repo is `shardpelt/Ambrotype`.
- **Built:** what people can do is in [FEATURES.md](FEATURES.md), how it is built in [ARCHITECTURE.md](ARCHITECTURE.md), and what's missing in [GAPS.md](GAPS.md).

## Help wanted

Things anyone with the right kit can do; open an issue first so two people don't do the same one.

- **Try the editor on a Mac or Windows machine** and report what feels wrong (the Feedback button opens a pre-filled GitHub issue, or an email without a GitHub account). Most testing so far is on Linux.
- **Run `amb preset census <your preset folders>`** and share the output (setting names and counts only, no preset files). It ranks what to build next.
- **Lightroom samples:** if you have Lightroom, the measurements in [lightroom-trial-plan.md](lightroom-trial-plan.md) let us fit our models (kit: `tools/lightroom-kit/make-kit.sh`). Share only fitted numbers, never Adobe's renders or files.
- **Film simulation references:** raws plus in-camera re-renders with each film simulation, so the Film recipe looks can be fitted ([design/film-look-measurement.md](design/film-look-measurement.md)).
- **Creative and Adobe Raw profiles** (`Look`, `RGBTable`) need two facts: the 85-character alphabet the DNG SDK uses for `Table_<md5>` text, and the names of the `crs:` properties that hold table fingerprints in an Adobe Raw profile `.xmp`. Facts only; copy no table data.

## Next steps

### Convert raws (2026-10-31)

For a card shot in RAW by mistake (a friend's niece's camera): the library's **Convert raws…** (in the editor: File → Convert Raws to JPEGs…, or right-click the photo, for its folder) and `amb convert` make JPEGs as the camera would have saved them, on Auto, or like one edited photo; photos already edited keep their edit, and a stopped run carries on. Also: right-click a library selection to apply a preset to all of it. Exports, Convert raws and `amb render` develop on the GPU when there is one, in tiles when the photo is too large for it at once (sized from the free memory the driver reports), and Convert raws does three photos at a time, or only the best of each burst ([design/gpu.md](design/gpu.md)). Small wins: History shows each step's values ("0.00 → +0.70", every change in its tooltip); Search (Ctrl+K) sets a setting from its name and a number ("exposure 0.7"); Settings → General can go to the next photo after a rating, flag or label from the keyboard. The library's grid starts with the folders inside the one shown, as tiles with their photo counts (and ↑ for the folder above); a click opens one. Double-click a folder in the Folders tree to open or close it. In the editor, Ctrl- or Shift-click photos in the filmstrip to select them; right-click applies a preset, pastes settings (Ctrl+Shift+C copies the open photo's), exports or converts them all. File → Export Edit Replay… (`amb replay`) makes a short AV1 MP4 of the edit from its History, for Reels and Shorts. AI models (Select object's SAM 2.1, AI Remove's Big-LaMa and MI-GAN, YuNet, search) run on the GPU with our own kernels when there is one (2026-11-06; [design/gpu.md](design/gpu.md)). The hub's CI now builds the app and installs every item with `ambrotype --check-hub HUB_FOLDER`, so a submission the app would refuse fails its pull request.

### From the competition (2026-10-30)

LightCraft, Rembrandt, Redlamp, FlashView and Tenzen, what to take from each, why people pay for Tenzen, and our niche: [research/17](research/17-competitors-2026.md). In work, in this order: ~~before/after split~~ (done), ~~a blank-canvas layout editor~~ (done: Appearance → Start from scratch…), ~~a shareable before/after page~~ (done: File → Share Before and After…, `amb share`), ~~contact sheets~~ (done: the library's Contact sheet…, `amb contact`: film strips with holes, frame numbers and edge print, or a plain grid; 300 dpi pages), ~~Touch~~ (done: T, drag on the photo for the tone region or the colour under it, `ui/touch.js`), ~~smart search~~ (done: CLIP or SigLIP 2, the user's choice in Settings → AI models; the library's Smart, Find similar photos, `amb search`).

### Pinned: teach by showing, not by cards (owner, 2026-10-20)

Card tips get skipped. Learn's lessons stay in Help → Learn, but first-run teaching should show, not tell. Done on 2026-10-22 (`ui/teach.js`):

1. ~~**A sample photo that teaches through its History.**~~ The empty screen's **Try a sample photo** (and Help → Open the sample photo) downloads a stand-in raw (the CC0 D750 still life from raw.pixls.us, 18 MB, checked) with a nine-step edit. Clicking a History step (or Ctrl+Z) lights up the controls it changed and slides them to their value. With History focused (clicked, or Tab) the arrow keys step through it and Home/End go to the ends. Left: swap in the owner's photo below (a new `data/sample/` sidecar, URL and checksum in `commands/sample.rs`).
2. ~~**Hold ⌘/Ctrl to see every shortcut**~~ on the toolbar buttons and tasks (not on sliders: they have no keys).
3. ~~**Empty states that teach:**~~ the library says "drop a folder here", and a folder dropped anywhere opens in it; on an unedited photo Exposure, Highlights, Shadows, Temp and Vibrance glow faintly until the first edit.
4. ~~**Task bar hover previews:**~~ a sketch of the task's columns and panels (open ones highlighted), with its key.
5. ~~**Watch mode:**~~ Help → Watch an edit (or the empty screen's button) opens the sample afresh, goes back to Original, and a ghost cursor replays its nine steps in about 20 seconds, each named beside it. Any key, click or scroll stops it; the rest stays in History.
6. ~~**A "?" by every setting**~~ (`ui/help.js`): hover for a line on what it does, click for a card: how it shows in a photo, when to raise it, when to lower it, its range and start value. The longer text (`coach` in `crates/amb-protocol/data/descriptions.json`) is written for every setting in the panels (101, and the 32 colour-band sliders from templates).
7. ~~**Photography basics**~~ (Help → Photography basics, Shift+F1; `ui/basics.js`, text in `ui/basics.json`): an eight-step guide to how a photo is made and edited, and 26 terms. In a "?" card, terms such as *midtones* or *clipping* are underlined faintly: hover for a line, click to open the guide there. Nothing shows unless asked for, so it stays out of a pro's way. Tone and colour terms show **on your own photo** (`ui/zones.js`): hovering *midtones*, *shadows*, *highlights*, *clipping* or *saturation*, or the "?" of Highlights, Shadows, Whites, Blacks, Saturation or Vibrance, lights up those pixels on the preview, dims the rest and says what share of the photo they are. Drawings (`ui/figures.js`, SVG in the theme's colours) show what a photo can't: aperture sizes, depth of field, shutter blur, ISO noise, stops, the histogram's zones, clipping, contrast, dynamic range, the kelvin scale, saturation, hue, luminance, halos, vignettes and crop shapes; six of the eight chapters have one. Left: more terms, terms in Learn's lessons, translations.
8. ~~**UI tests**~~ (`ambrotype --ui-test SCRIPT.js photo.dng`; [recipes.md](recipes.md)): the editor runs a script that acts like a person (sliders, keys, clicks) and checks what the page shows, then exits with the result. CI runs `crates/amb-gui/tests/ui/` on Linux. The first script opens a photo, edits, undoes and redoes, opens every panel, checks the Color Mixer tabs and a help card, and exports. Left: more scripts (masks, crop, library), and macOS and Windows.
9. ~~**Monitor colour**~~ (Settings → General): on Linux the preview is converted to the monitor's profile, from the desktop (X11, per monitor) or a chosen file; macOS and Windows webviews do it themselves. Left: Wayland (colord), LUT profiles, thumbnails; check by hand on a Mac and on Windows.

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
- [x] ~~Hub history clean-up~~: dropped (owner, 2026-10-23). Older commits in both repos keep their trailers; new ones name no model
- [x] Release notes: `docs/release-notes/v0.1.0.md` (the draft release starts with it). Read it before publishing
- [ ] After publishing: README's "no installers yet" becomes a link to the release
- [ ] Tag `v0.1.0`, check the draft release, publish

After the release: the owner's sample raw (spec above) replaces the stand-in.

~~Hub repo to-do: index `borders/`~~ (done 2026-10-23: White, White with a black line and Polaroid are in the hub).

There are two tracks that can run in parallel: the UI track is what users see and share, and the fidelity track depends on the Lightroom trial data.

### UI and sharing track (agreed 2026-10-07, in this order)

1. ~~Crop tool and size presets~~ (done 2026-10-07). Small follow-ups are in [GAPS.md](GAPS.md).
2. ~~Layout files, built-in looks and themes~~ (done 2026-10-07; [design/customization-and-hub.md](design/customization-and-hub.md), [ai/ui-layouts.md](ai/ui-layouts.md)).
3. ~~Editor mode~~ (done 2026-10-07): **Edit layout** moves panels, settings and toolbar buttons by drag and drop, changes control types, labels, titles and columns, splits and merges panels, adds from a list of what isn't shown, and resizes columns. Colours are edited in the Appearance dialog. Since 2026-10-15 it starts from Appearance → Edit layout… (or Ctrl+Shift+L); the built-in toolbars no longer have their own button for it.
4. ~~Hub~~ (done 2026-10-07, for layouts and themes; needs the hub repo, see Owner to-dos): Omarchy-style. Items are git repos, and one index repo takes submissions as pull requests. The in-app browser covers layouts, themes, presets, collage templates and size profiles.
5. ~~Borders~~ (done 2026-10-08): border presets in the export dialog and `amb render --border`, batch with `amb render A B ... --out-dir`. Was: width, colour, caption; border presets; fit to a size preset; batch.
6. ~~Collage maker~~ (done 2026-10-08): shareable layouts (size, background, gap, outer border, rounded corners, cells you can split and remove), per-photo move/zoom/rotate and "Edit photo" in the editor, batch pages for big folders (optionally only rated photos). Follow-ups (resizing cells by their edges, saving a collage to reopen, per-cell edits) are in [design/collage-and-borders.md](design/collage-and-borders.md).
7. ~~Plugins and AI-made features~~ (done 2026-10-08): WASM-sandboxed "action" plugins with permissions, "Make one with AI…" instructions for the user's own assistant, Share as pull request to the hub. ~~Batch runs~~ (from the library, 2026-10-12). ~~Pipeline effects with their own controls~~ (2026-10-15: effect plugins, colour or pixels, in the Effects panel; Duotone and Light leak built in; Holographic and Hue trip 2026-11-04, for the Trippy task). Next for plugins: free-form plugin panels, ~~the hub listing plugins in the app~~ (2026-10-12; [GAPS.md](GAPS.md)).

The agreed UI track is done, and the library view after it (2026-10-08; [design/library.md](design/library.md)). Next is the parity track below.

### Parity track (agreed 2026-10-08)

Another open-source Lightroom alternative, [LightCraft](https://github.com/storytold/lightcraft) (MIT/Apache, egui, its own catalogue), appeared with much more breadth. We keep our own direction (faithful Lightroom presets and sidecar round trip, customising and sharing, plugins) and build towards what it has, in this order:

1. ~~**Fujifilm raws:** proper X-Trans demosaic, neutral blown highlights~~ (done 2026-10-08).
2. ~~**JPEG, PNG, TIFF and WebP input**, with a raw and its same-name JPEG shown as one photo (as Lightroom does), and Lightroom's relative white balance for non-raw files~~ (done 2026-10-08).
3. ~~**Local Highlights and Shadows** (edge-aware tone mapping), the biggest fidelity gap (fidelity track 3)~~ (done 2026-10-08).
4. ~~**Masks:** linear, radial, brush, luminance and colour range, add/subtract/intersect; read and write Lightroom's mask tags~~ (done 2026-10-08; [design/masks.md](design/masks.md)). ~~Select Subject~~ (2026-10-15: U²-Net small, run locally, downloaded on first use). ~~Sharper subject edges~~ (2026-10-15). ~~Background and People (everyone), with the model chosen per task and the user's own ONNX models~~ (2026-10-17: Settings → AI models, `amb models`, MCP `ai_models`). Next for masks: fitting in the trial month, people and objects. ~~Reordering and duplicating masks~~ (2026-10-12).
5. ~~**Spot removal:** heal and clone~~ (done 2026-10-10; [design/spot-removal.md](design/spot-removal.md)). **To do (owner, 2026-10-19): blemish removal with the brush isn't good enough yet;** see GAPS. Red eye waits for a Lightroom sidecar: the `RedEyeInfo` format isn't documented anywhere we can read, and a guess could break Lightroom's files.
6. ~~**Transform and Upright**~~ (done 2026-10-10: the sliders, and Upright Auto, Level, Vertical and Full from the photo's lines; [design/transform.md](design/transform.md)). ~~Guided Upright~~ (2026-10-12, as Transform sliders). Next for it: fitting in the trial.
7. ~~**Library:** pick/reject flags, loupe, compare and survey views, import with copy and rename, collections, virtual copies~~ (done 2026-10-10; [design/library.md](design/library.md)). ~~Subfolders, smart collections~~ (done 2026-10-12). ~~Metadata panel~~ (title, caption, creator, copyright; 2026-10-12). ~~Stacks, several folders at once, collection sets~~ (2026-10-12). ~~Moving from a Lightroom Classic catalogue~~ (2026-10-13: `amb lrcat import`, From Lightroom…; to try on real catalogues). ~~Opening one in place~~ (2026-11-06: the Folders panel's Lightroom section, `amb lrcat open`). ~~The catalogue~~ (2026-10-15: a SQLite index of what was read from the sidecars and raw headers, so big folders open about 50× faster once seen; All photos; camera, lens, ISO and focal length in search and smart collections; `amb catalog`). Next for it: smart collections and All photos straight from the index without checking every file.
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
2. ~~**Ways to browse:** a timeline (years, months, days, with a scrubber), a calendar, and a map from GPS tags (photos without GPS can be placed from a GPX track or by dragging)~~ (2026-10-23: Timeline and Map views, [design/library.md](design/library.md)). Left: a place search (by name), reverse geocoding (city and country as metadata).
3. ~~**Duplicates and bursts:** exact duplicates by file hash, near-duplicates and bursts by a picture fingerprint, grouped into stacks, with "keep the best, reject the rest"~~ (2026-10-23: Organise → Duplicates and bursts…, [design/library.md](design/library.md)). Left: deleting the extra copies (to the trash) from the dialog.
4. **Culling help:** score sharpness, closed eyes and exposure, so the best of a burst comes first. Local models only.
5. **Search by what's in the photo:** a local image-and-text model ("dog on a beach at sunset") and optional local face grouping, both off by default and never uploaded. Models must have a licence we can use (AI models list).
6. ~~**File care:** move, rename and delete with sidecars and JPEG pairs kept together (to the system trash), batch rename, reorganise into dated folders, a keyword tree manager, metadata templates, and "find missing photos" after files moved outside the app~~ (2026-10-23: library → Organise, [design/library.md](design/library.md)). Left: keyword hierarchies (the list is flat), drag photos onto a folder in the Folders panel, and trying the trash on a Mac and on Windows.
7. **Safety:** checksums kept in the catalogue to catch silent file corruption, a backup check (is every photo also on the backup drive?), and how much space each folder, year and camera takes.
8. **Several computers:** a library on a NAS or synced folder used from more than one machine, each with its own catalogue cache.
9. **Sharing:** export a static web gallery and a slideshow from a collection. (2026-10-23: a slideshow in the app, Ctrl+Enter. Left: exporting one as a video or a web page, and music.)
10. **Videos:** list and play the clips next to the photos, with ratings and keywords, so a card's whole import lives in one place.

### Film lab track (owner, 2026-10-28)

Ideas from two film-scan apps, negaflow (Apache-2.0) and NegPy (GPL-3), rebuilt in our own code from their docs and public sources. None of these needs their authors' permission or their code. The comparison, what we don't take, and the patent check are in [research/16](research/16-film-scan-apps-survey.md). Sizes are S (a day or two), M (a week) and L (more). Roughly in this order:

1. ~~**Quick conversion wins**~~ (done 2026-10-28):
   - ~~a fixed density range when Auto is off~~ (`amb:NegativeRange`)
   - ~~a film base eyedropper~~ (Pick base) and ~~a roll base~~ (pick on one frame, then Paste settings → Film negative); ~~a base that isn't bare light~~ (holes or gaps around the film)
   - ~~a metering region~~: the levels are measured inside the crop, the base on the whole scan. Left: drawing a region by hand
   - ~~an expired-film target~~ (`amb:NegativeExpired`: matches red and blue to green from shadows to highlights)
   - Tried on eight real scans from the BlueNeg dataset (2026-10-28), which fixed Auto off and Expired film ([design/film-negatives.md](design/film-negatives.md))
2. ~~**Cast removal along the greys**~~ (done 2026-10-29: `amb:NegativeCastRemoval`, red and blue matched to the line of greys from shadows to highlights; tried on a synthetic sunset and two real frames, [design/film-negatives.md](design/film-negatives.md)).
3. ~~**Auto Density and Auto Grade**~~ (2026-10-31: switches beside Auto, off by default; try them on real scans before making them the default), metered only on parts with detail, with highlight and shadow holds (M). The patents behind them have expired (the 2002 one on 2024-08-03, checked 2026-10-31).
4. ~~**Roll baseline**~~ (done 2026-10-30: Sync negative roll… asks for scenes grouped by light, odd frames kept, a baseline frame and own brightness; `amb negative sync`; [design/film-negatives.md](design/film-negatives.md)).
5. **Dust and scratches:** automatic, a guided rectangle, a heal brush that searches its stroke, a line tool, infrared from a fourth channel or an `_ir` file, a check that keeps wires and lettering, grain kept, repairs as layers in the sidecar (L). Test bench: FILM-R v2, downloaded, never committed.
   - ~~Automatic specks and hairs, grain kept, a check that keeps wires and lettering, the FILM-R bench~~ (done 2026-10-29: Find dust in the Spot panel and `amb dust`, as heal spots; [design/dust.md](design/dust.md)). ~~The line tool~~ (2026-10-29: Shift-drag in the spot tool, `amb dust --line`). Left: the guided rectangle and a heal brush that searches its stroke. Infrared parked until there is a scan with an infrared channel ([GAPS.md](GAPS.md) §9).
6. **Film look on digital photos:** ~~halation in linear light~~ (Effects → Glow: Halation and Bloom, 2026-10-31), ~~grain that follows density~~ (Grain → Film response, 2026-10-31), ~~a paper-like roll-off~~ (Effects → Film finish → Highlight roll-off, 2026-11-02) (M).
7. **Darkroom aids:** density and zone under the cursor, zone overlay, placing a spot in a zone, step wedge, negative range and scan-clip readouts, test strip, ring-around, peek at the negative (M); Pick Better, choosing between two versions to tune one setting at a time (M).
8. **Scans in:** flat-field from a shot of the bare light, a channel-bleed matrix per film type or from a colour chart, half-frame split, a strip split into frames, stitching a large negative (M each).
9. **A paper curve in density:** toe, straight line and shoulder, ISO-R grades, split grade, preflash, B&W papers fitted from public datasheets (L). Candidate for negative model v2.
10. **Print and output:**
    - a contact sheet as film strips, with a straight proof (M)
    - print layouts and paper sizes (M)
    - printer and paper soft proof, applied once to the composed page (M)
    - a flat 16-bit master (S)
11. **B&W processes and finishing:** lith, cyanotype, Sabattier, chemical toners, edge burn, filed carrier, chroma denoise that keeps edges, a skin chroma ceiling (S–M each; some as effect plugins).
12. **Film metadata:** stock, camera and development per roll, search terms such as `film:… iso:>=400`, and a hot folder (M).

Not taken: NegPy's print tables (made from Fujifilm printer software and profiles), negaflow's lab-scanner profiles named after the machines, and CC BY-SA data NegPy carries (spektrafilm dye curves, DX codes) unless the owner says yes.

### Codebase track

The structure review of 2026-11-09 ([design/structure.md](design/structure.md)) found no need for a rewrite. Its cleanup is in [cleanup.md](cleanup.md), in exact steps for smaller models: guard rails, small merges and splitting the big files are done (2026-11-09); the structural steps wait for the owner's go.


0. **First, measure what helps** (plan: [design/agent-navigation.md](design/agent-navigation.md)): fresh agents on a fixed task set, with and without recipes and on-demand maps. Rounds A1 and B1 are done: with the recipes, Sonnet passed all 12 tasks at the same cost. The steps below follow from it.
1. **A map for agents** (largely done: [recipes.md](recipes.md), checked by a test; [ARCHITECTURE.md](ARCHITECTURE.md); [design/structure.md](design/structure.md)): a detailed map of the code (per crate and per UI file: what it owns, its main types and functions, how data flows from a slider to the render and to the sidecar) with recipes for common changes ("add a setting", "add a panel", "add a toolbar button", "add a mask kind", "add a hub item kind"). Builds on [ARCHITECTURE.md](ARCHITECTURE.md) and AGENTS.md; checked by a test where it can be (like the settings reference).
2. **A cleanup pass to match it** ([cleanup.md](cleanup.md)): ~~split the largest files~~ (2026-10-16: `amb-gui/src/commands/`; `ui/app.js` into preview, crop, presets and export files; the CLI by area; the engine's `pipeline/` and `detail/` as folders; 2026-11-09: the cleanup's phases 1 and 2, 15 more files split and shared helpers merged), one clear home per concern, consistent names, dead code out. Behaviour unchanged; the tests and a by-hand run of the editor confirm it.

## Working notes

- This repo was built with Claude Code. Large text sweeps went to smaller models, and parallel work to worktree agents that were then merged.
- **Parallel agents** need separate `CARGO_TARGET_DIR`s, or they overwrite each other's binaries. A debug `target/` grows to about 20 GB.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. On Linux the GUI needs the WebKitGTK dev packages (see desktop-setup.md).
