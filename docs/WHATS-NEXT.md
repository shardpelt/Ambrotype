# What's next

A short running list, kept up to date as work lands. Newest decisions first. Everything planned, in order, is in [ROADMAP.md](ROADMAP.md); what's missing is in [GAPS.md](GAPS.md). Projects we researched, and when we last checked them, are in [research/watchlist.md](research/watchlist.md).

## Waiting on the owner

0. **Sample photos from your Fujifilm** (owner, 2026-11-11): a few raws if you have them, JPEGs are fine too (JPEGs show the library, Insights and the look presets well; raws are needed only for the raw pipeline and Convert raws). For the README screenshots and the sample photo.
1. **v0.1.0:** try the editor on a Mac and once on Windows (the list is in ROADMAP.md's v0.1 checklist), then say go: tag, check the draft release, publish, and point the README at it.
2. **Auto Density and Auto Grade on real negatives:** try both switches (Negative panel, beside Auto) on a roll. If they look right, they become Auto's default.
3. **GPU speed:** export a 24 MP raw on the Mac with and without `AMB_GPU=off` (both are tiled or whole as the GPU's memory allows). And AI on the GPU: time Select object's first click and an AI Remove with and without `AMB_AI_GPU=off` (Settings → AI models says where they run).
4. **Your sample raw** to replace the stand-in sample photo, after the release.

## Up next

The order is [ROADMAP.md](ROADMAP.md)'s. The first open rows:

- **A paper curve in density** (`film-paper-curve`)
- **Film looks tied to stocks** (`film-stock-looks`)
- **Dust: the guided rectangle and a heal brush that searches its stroke** (`film-dust-guided`)
- **Darkroom aids** (`film-darkroom-aids`)
- **Fade Restoration** (`film-fade-restoration`)

## Recently done

- 2026-11-13: **One sideways row of photos** (`photoRow` in `ui/core/photo-row.js`): the Develop filmstrip and the Map's tray of photos without a place share one row, so the tray also scrolls with the wheel and glides with a drag, and a photo dragged up still lands on the map; Clean up's strip, Duplicates' rows and the convert dialog's picks use it too
- 2026-11-13: **Duplicate and Edit… for presets**: right-click a preset: **Duplicate** (any preset, from a pack, Lightroom or a folder you added; a copy named “NAME copy” in the app's own folder, the same group, applying exactly as the original) and, for your own, **Edit…** (rename, move to a group, untick or add settings, or take the values from the open photo); `duplicate_preset`, `preset_contents` and `update_preset` in `src/commands/preset_edit.rs`, the file work in `src/preset_files.rs`, the dialog in `ui/develop/preset-edit.js`
- 2026-11-12: **A code style and the guards that hold it**: `docs/codestyle.md` says how code is written here, held by guard tests (the editor-command handler list, the UI's wire calls, old UI idioms whose counts may only fall, `amb`'s usage against its dispatch, the docs map and links); a docs map (`docs/README.md`), task cards for cleanup (`docs/cleanup.md`), a roadmap by model tier (`docs/ROADMAP.md`) and `docs/delegation.md`.
- 2026-11-12: **Batch export** (library → Batch export…, File menu, or search "batch", "resize", "smaller"; `amb render DIR… --out-dir OUT --recursive`): a folder and its subfolders as smaller copies in another folder, in the same folder structure (or flat), each photo with its own edit; a line says how many photos in how many folders before it starts, then progress with Stop, then a summary with the sizes before and after. Files are written whole or not at all, so a stopped run starts again with Skip files that exist (`amb-ops::export_batch`)
- 2026-11-12: **Back in the library** steps through the folders (and collections) you came from, with the grid's scroll, and Forward (Alt+→) mirrors it; it leaves the library only when none are left. Folder loads now run one at a time, so Back can't land mid-load
- 2026-11-12: **the Map drags and zooms** like the photo (`ui/library/map-touch.js`, also in Pick…): drag to move, wheel or pinch to zoom at the pointer, two-finger swipes move it; a sideways swipe no longer zooms out a level per wheel event
- 2026-11-12: **Hide or delete presets**: right-click a preset (Hide, Delete… for ones saved in Ambrotype, to the trash) or a group heading (Hide this group); “N hidden” at the foot of Presets shows them dimmed or unhides all; hidden ones are left out of every preset list
- 2026-11-11: **AI Denoise** on the raw (library → More → AI Denoise…, or search; `amb denoise RAW [--amount]`): RawNIND's networks (GPL-3.0 or CC BY 4.0) take the noise out before the photo is developed, into a new `-Denoised.dng` beside it with its edit, as Lightroom does. Bayer sensors from the sensor's data, X-Trans and others after our demosaic; Amount mixes back the photo as it was. An optional 58 MB download in Settings → AI models (darktable-ai's archive); about 50 s for a 24 MP photo on 4 cores
- 2026-11-11: **Kaleidoscope** and **Heat camera** effects, in the Trippy tab (Kaleidoscope, Mandala, Fractal, Heat camera, Contours) and Trip me: mirrored slices or a fractal fold, false colour by brightness (Ironbow, Rainbow, White hot, Black hot, Arctic, with repeating bands). Effect plugins can now ask for the whole photo at once (`"whole": true`), which a kaleidoscope needs
- 2026-11-11: **the guides for AI agents, built in**: `amb docs` (list, one, or all written out with their links working), MCP resources for each, and Read in Connect an AI app; **Start Claude Code / Codex / Gemini CLI here** opens a terminal in the photo's folder with `amb` ready and starts the agent, told where the guides are (the command for each is data, in `ai-apps.json`)
- 2026-11-11: **Clean up** (library → Organise ▾ → Clean up…): finds the photos you may not want (copies and alike pictures, the weaker frames of bursts, blurry, eyes closed, nearly black or blown out, rejected), each with why, and pages through them all marked to go; K keeps, D lets go, then the trash or Reject them. Picked or rated photos are left out. `amb cleanup DIR [--trash|--reject]`
- 2026-11-11: a **man page** for `amb` (`amb man`, `docs/man/amb.1`), made from its help text, a test keeps it current; every idea in one ordered list, quick wins first, then the big things ([GAPS.md](GAPS.md) §0)
- 2026-11-11: **Pick…** a place on a map or by address or name (OpenStreetMap's Nominatim, asked only on a search): from the library panel's Place, the viewer's info panel and each guess in Guess places
- 2026-11-11: a **Place** for each photo by hand: the library panel (for the selection) and the viewer's info panel take coordinates, degrees or a pasted Google Maps, OpenStreetMap or Apple Maps link; Map shows it, × clears it
- 2026-11-11: **Guess places** (Map view, `amb place --guess`): photos without a place get one from those that have one, taken within 30 minutes (between two, on the line between them; a clock difference can be given) or looking the same (photos Smart search indexed); listed with why, confirmed before anything is written (`amb-library::places`)
