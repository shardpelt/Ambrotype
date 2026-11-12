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

- 2026-11-11: **AI Denoise** on the raw (library → More → AI Denoise…, or search; `amb denoise RAW [--amount]`): RawNIND's networks (GPL-3.0 or CC BY 4.0) take the noise out before the photo is developed, into a new `-Denoised.dng` beside it with its edit, as Lightroom does. Bayer sensors from the sensor's data, X-Trans and others after our demosaic; Amount mixes back the photo as it was. An optional 58 MB download in Settings → AI models (darktable-ai's archive); about 50 s for a 24 MP photo on 4 cores
- 2026-11-11: **Kaleidoscope** and **Heat camera** effects, in the Trippy tab (Kaleidoscope, Mandala, Fractal, Heat camera, Contours) and Trip me: mirrored slices or a fractal fold, false colour by brightness (Ironbow, Rainbow, White hot, Black hot, Arctic, with repeating bands). Effect plugins can now ask for the whole photo at once (`"whole": true`), which a kaleidoscope needs
- 2026-11-11: **the guides for AI agents, built in**: `amb docs` (list, one, or all written out with their links working), MCP resources for each, and Read in Connect an AI app; **Start Claude Code / Codex / Gemini CLI here** opens a terminal in the photo's folder with `amb` ready and starts the agent, told where the guides are (the command for each is data, in `ai-apps.json`)
- 2026-11-11: **Clean up** (library → Organise ▾ → Clean up…): finds the photos you may not want (copies and alike pictures, the weaker frames of bursts, blurry, eyes closed, nearly black or blown out, rejected), each with why, and pages through them all marked to go; K keeps, D lets go, then the trash or Reject them. Picked or rated photos are left out. `amb cleanup DIR [--trash|--reject]`
- 2026-11-11: a **man page** for `amb` (`amb man`, `docs/man/amb.1`), made from its help text, a test keeps it current; every idea in one ordered list, quick wins first, then the big things ([GAPS.md](GAPS.md) §0)
- 2026-11-11: **Pick…** a place on a map or by address or name (OpenStreetMap's Nominatim, asked only on a search): from the library panel's Place, the viewer's info panel and each guess in Guess places
- 2026-11-11: a **Place** for each photo by hand: the library panel (for the selection) and the viewer's info panel take coordinates, degrees or a pasted Google Maps, OpenStreetMap or Apple Maps link; Map shows it, × clears it
- 2026-11-11: **Guess places** (Map view, `amb place --guess`): photos without a place get one from those that have one, taken within 30 minutes (between two, on the line between them; a clock difference can be given) or looking the same (photos Smart search indexed); listed with why, confirmed before anything is written (`amb-library::places`)
- 2026-11-11: **Zoom as in other apps**: the mouse wheel and a trackpad pinch zoom at the pointer (Ctrl+wheel on Linux and Windows, WebKit's gesture events on a Mac), smoothly from the whole photo to 800 %, two fingers move a zoomed photo, Ctrl + − 0; the same in the library's viewer (and a click in its middle, a right-drag box). **F** in Develop: just the photo, full screen (the filmstrip moved to F6). Peek on hover stays the loupe (I) for now
- 2026-11-11: Insights finished: **What it suggests** (a fast lens kept stopped down, a zoom often at its end, dark scenes and shake, midday light, the focal length you live at, where the keepers come from; each with its number), **Try this week** (challenges: the far side of your focal length, aperture and time of day), and **Split by** camera, lens or year (up to three side by side, as shares). Also in `amb insights`
- 2026-11-11: the Trippy task's effect buttons show the photo with the effect while hovered, as presets do (the click gives what the hover showed); History's top edge and the line under Snapshots drag to size them again (double-click: usual size)
- 2026-11-11: Back (‹, Alt+←) steps through the library's views and an Insights bar's filter, not only the tabs; Survey taken out of the library (compare and the grid do its job); Compare splits side by side or above and below, the candidate either way round; Grid comes back from Map and Timeline; presets apply to the original by default
- 2026-11-11: **Glass Light** and **Glass Dark** themes (Appearance): frosted, see-through panels over a soft wash of colour (a theme's new `glass` token, the blur in px); still to check that the blur stays smooth on older Windows machines. Eight new presets (Blue Hour, Cinematic, Crisp Morning, Desert Dust, Matte Portrait, Noir, Sunset Duotone, Vaporwave) and four new Surprise me styles (Cinematic, Dreamy, Vintage, Punchy)
- 2026-11-11: **Insights** (library view, Shift+I): how the photos shown were shot: a chart each for aperture, focal length, shutter speed, ISO, time of day, lens and camera, with the kept ones (picked, or 3 stars and up) as the bar's darker part, and the settings used most together; camera chips narrow it, a click on a bar shows those photos. `amb insights DIR`
- 2026-11-10: Select object's **Copy**: drag a copy of it elsewhere, and it stays where it was too (Move's clone without the fill)
