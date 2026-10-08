# Where we left off (2026-10-08)

A short "start here" for the next work session. Details are in [ARCHITECTURE.md](ARCHITECTURE.md) (what exists) and [GAPS.md](GAPS.md) (everything missing). New to the repo, or an AI agent? Read [AGENTS.md](../AGENTS.md) first.

## State

- **Branch:** `claude/lightroom-alternative-scope-xlnf0p`. Everything is committed and pushed; about 350 tests are green.
- **Name:** **Ambrotype**. The GitHub repo is `shardpelt/Ambrotype` (renamed from flightbroom; old links redirect).
- **Built:**
  - preset/sidecar reader and writer (`crs:` and our `amb:` tags)
  - edit protocol with undo, including grouped commands that become one undo step
  - raw engine: core Develop panels, Lens Corrections, film negatives, **crop and straighten** ([design/crop.md](design/crop.md))
  - export: JPEG/PNG/TIFF/WebP/AVIF/EXR, colour spaces with ICC
  - CLI `amb`
  - desktop editor (`amb-gui`, Tauri 2) with:
    - 1:1 zoom and pan
    - white-balance picker
    - clipping overlay
    - point-curve editor
    - colour-grading wheels
    - saved preset folders
    - feedback button (pre-filled GitHub issue)
    - crop and straighten tool (R), with aspect lock
    - **size presets** (Instagram, Snapchat, TikTok, X, Reddit, YouTube, Pinterest, LinkedIn and print sizes, plus custom ones) for the crop and exact-size export (`amb render --exact WxH` too)
    - **looks:** the UI is drawn from a layout file (toolbar, side columns, panels, and knob/bar/slider/number per setting) and a theme (colour tokens). Built-in looks (Classic, Compact, Big controls, Studio, Left-handed; Dark, Light, High contrast, Neutral grey) can be cloned, renamed, shared and imported in the Looks dialog. AI assistants can write them ([ai/ui-layouts.md](ai/ui-layouts.md), `ambrotype --check-look`)
    - **settings and shortcuts:** the ⚙ button lists every keyboard shortcut on a keyboard map and lets you change them (clashes are caught; only changes are stored)
    - **filmstrip:** the other photos in the folder under the photo; ← → or a click opens them, saving edits first
    - **rating, colour labels and keywords:** stars (keys 1–5), labels (6–9) and a keyword field, saved to the sidecar at once and embedded in exports
    - **library** (G): a folder as a grid; sort, filter and search; select many to rate, label and keyword at once; copy one photo's edit and paste it onto others (by setting group); export the selection; start a collage with it ([design/library.md](design/library.md))
    - **hub:** Looks → Browse hub… installs shared layouts and themes (checked against the index's sha256 and validated); Submit to hub… opens GitHub with your look filled in as a pull request
    - **editor mode:** Edit layout rearranges the screen in place (drag and drop, control types, split and merge, add and remove, column widths) and saves it as your own layout
    - **borders** on export, with presets and colour swatches
    - **plugins** (Ctrl+Shift+P): sandboxed WebAssembly plugins that make edits (one undo step), with permission prompts, install from a folder, Share as pull request, and "Make one with AI…" instructions for the user's own assistant. Built in: Auto tone and Random look. CLI: `amb plugin new/build/check/run` ([ai/plugins.md](ai/plugins.md))
    - **collage maker** (C): layouts with gap, outer border, corners and background; drag photos into cells, then move, zoom, rotate, or edit them in the editor; **arrange freely** (move, resize, turn and overlap frames by hand); save the collage as a `.ambcollage` file to reopen and edit; save and share layouts; batch a whole folder into pages ([design/collage-and-borders.md](design/collage-and-borders.md))

## Owner to-dos

- [ ] Set up a **local** Claude Code session on the Mac so it can reach your drives and Lightroom: [desktop-setup.md § Working with an AI agent locally](desktop-setup.md#working-with-an-ai-agent-locally).
- [ ] Fill the hub repo `Ambrotypehq/ambrotype-hub` (created, empty) with the `hub/` folder of this repo: copy its contents to the repo's root and push; keep it public with Actions on. Shared plugins arrive there as pull requests to `plugins/<id>/`. (Or install the Claude GitHub App on the Ambrotypehq organisation so an agent can push it.) The app's **Browse hub…** reads `https://raw.githubusercontent.com/Ambrotypehq/ambrotype-hub/main/index.json`. Until then it says the hub can't be reached.

- [ ] Try the editor on the Mac: `cargo run --release -p amb-gui -- photo.nef`. Note what feels wrong (or use the Feedback button).
- [ ] Run `amb preset census <your preset folders>` and share the output (setting names and counts only). It ranks what to build next.
- [ ] Lightroom trial month: follow [lightroom-trial-plan.md](lightroom-trial-plan.md) (sweep kit: `amb sweep generate`).

## Next steps

There are two tracks that can run in parallel: the UI track is what users see and share, and the fidelity track depends on the Lightroom trial data.

### UI and sharing track (agreed 2026-10-07, in this order)

1. ~~Crop tool and size presets~~ (done 2026-10-07). Small follow-ups are in [GAPS.md](GAPS.md).
2. ~~Layout files, built-in looks and themes~~ (done 2026-10-07; [design/customization-and-hub.md](design/customization-and-hub.md), [ai/ui-layouts.md](ai/ui-layouts.md)).
3. ~~Editor mode~~ (done 2026-10-07): **Edit layout** moves panels, settings and toolbar buttons by drag and drop, changes control types, labels, titles and columns, splits and merges panels, adds from a list of what isn't shown, and resizes columns. Colours are edited in the Looks dialog.
4. ~~Hub~~ (done 2026-10-07, for layouts and themes; needs the hub repo, see Owner to-dos): Omarchy-style. Items are git repos, and one index repo takes submissions as pull requests. The in-app browser covers layouts, themes, presets, collage templates and size profiles.
5. ~~Borders~~ (done 2026-10-08): border presets in the export dialog and `amb render --border`, batch with `amb render A B ... --out-dir`. Was: width, colour, caption; border presets; fit to a size preset; batch.
6. ~~Collage maker~~ (done 2026-10-08): shareable layouts (size, background, gap, outer border, rounded corners, cells you can split and remove), per-photo move/zoom/rotate and "Edit photo" in the editor, batch pages for big folders (optionally only rated photos). Follow-ups (resizing cells by their edges, saving a collage to reopen, per-cell edits) are in [design/collage-and-borders.md](design/collage-and-borders.md).
7. ~~Plugins and AI-made features~~ (done 2026-10-08): WASM-sandboxed "action" plugins with permissions, "Make one with AI…" instructions for the user's own assistant, Share as pull request to the hub. Next for plugins: pipeline effects and plugin panels, batch runs, the hub listing plugins in the app ([GAPS.md](GAPS.md)).

The agreed UI track is done, and the library view after it (2026-10-08; [design/library.md](design/library.md)). What to pick next is open: candidates are the follow-ups above and the fidelity track below.

### Fidelity and engine track

1. **Lightroom trial month:** capture the sweeps, then fit the model constants ([lightroom-trial-plan.md](lightroom-trial-plan.md)). Check the crop's angle sign and corner semantics there ([design/crop.md](design/crop.md) open questions).
2. **Transform/Upright** (numeric `Perspective*` first), composed into the crop/lens resampling pass.
3. **Local Highlights/Shadows** (edge-aware), then **masks** (linear/radial/brush/range).
4. **Camera profiles** (read the user's DCPs in place, fitted base looks) once trial data exists.
5. **Calibration panel, B&W mixer, Point Color.**
6. **Token-efficient AI layer:** core card, `amb do` compact syntax, `--brief` output, `amb stats`, then the MCP server (also the base for the AI flows in UI step 7).
7. **Auto and Randomize buttons, Tutorial mode** ([design/future-ideas.md](design/future-ideas.md)).
8. **Performance:** measure release-build preview and 1:1 times on the Mac; a tiled 1:1 render instead of a full-size render (a debug build took about 33 s); then the GPU preview path.
9. **Installers** per OS (`cargo tauri build`, in CI; notarised DMG for macOS).

## Working notes

- This repo was built with Claude Code. Large text sweeps went to smaller models, and parallel work to worktree agents that were then merged.
- **Parallel agents** need separate `CARGO_TARGET_DIR`s, or they overwrite each other's binaries. A debug `target/` grows to about 20 GB.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. On Linux the GUI needs the WebKitGTK dev packages (see desktop-setup.md).
