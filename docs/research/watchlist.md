# Watchlist: projects we researched

Projects we surveyed, with where they stood when we last looked, so we can check them again and see what changed. Recheck about once a month, or when someone points at one.

**How to recheck.** Read the README, the changelog or release notes, and the website. Don't read their code (see [legal.md](../legal.md)). Then:
1. Update "Last checked" and "Where it was".
2. Add anything new worth taking to that project's survey, and to [WHATS-NEXT.md](../WHATS-NEXT.md) if it's worth doing soon.
3. Add a new project here when a survey brings one in.

Each recheck also looks for projects we don't track yet (new open-source raw editors and Lightroom alternatives named in the places we read) and adds them to the list.

Reddit can't be read by our tools (blocked for automated readers since 2026-11); ideas from there come by hand (paste a thread) or via pixls.us, Hacker News and articles that quote it.

## Open source

| Project | Licence | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [LightCraft](https://github.com/storytold/lightcraft) | MIT or Apache-2.0 | [17](17-competitors-2026.md) | 2026-11-19 | v0.21.1 (12 Nov), ~16.2k stars, tagged releases since v0.x: Lightroom catalogue backup (.zip) import, Lens Blur with AI subject focus and bokeh per mask | SAM 3 Object/Describe masks, AI Denoise (Bayer, in progress), MCP server, print layouts, contact sheet PDF |
| [CalibRaw](https://github.com/Duecki1/CalibRaw) | GPL-3.0-or-later | [17](17-competitors-2026.md) | 2026-11-19 | v1.4.1 (9 Nov), no newer; ~344 stars; already has Glow (Bloom and Halation modes) and a Halation card (radius, threshold, warmth) | Guided perspective, People mask, depth and object masks, RawNIND denoise. Taken so far (our own builds): tiled GPU exports from free memory, Edit Replay, auto defringe, highlight roll-off, AI Remove |
| [Rembrandt](https://github.com/thesnarkitecht/rembrandt) | GPL-3 | [17](17-competitors-2026.md) | 2026-11-19 | 0.3.36 (11 Nov), dependency bumps only; ~211 stars | Super Resolution, Generative Remove and Expand, Refocus, edits asked in words, Edit like me, Lightroom catalogue import |
| [Redlamp](https://github.com/pdcgomes/redlamp) | MPL-2.0 | [17](17-competitors-2026.md) | 2026-11-19 | 0.3.5-alpha (2 Nov), no newer; ~405 stars; a film stock brings its own grain, halation and bloom; Halation and Bloom sliders in masks too | 36 film simulations, focus stacking, SAM 2.1 object masks (SAM 3 under Meta's own licence), Depth Anything sky and depth, Generative Remove, dust removal |
| [RapidRAW](https://github.com/CyberTimon/RapidRAW) | AGPL-3.0 | [07](07-rapidraw-survey.md), [04](04-oss-ecosystem.md) | 2026-11-19 | v1.7.0 (15 Nov): new engine (GPU RCD and Markesteijn demosaic, rebuilt Highlights to Structure, OKLab colour wheels; old edits render differently), RapidRes 2× AI super resolution, per-camera default adjustments, docked filmstrip | AI Text mask, face-part masks, GPU AI, 2.5D relight, Auto Straighten, Immich, paid cloud generative cleanup, AI-free mode |
| [negaflow](https://github.com/habinsong/negaflow) | Apache-2.0 | [16](16-film-scan-apps-survey.md) | 2026-11-19 | 1.2.7 (17 Nov): better auto adjustment, Transform and Horizontal tools | Negative conversion, GrainMend dust and scratches |
| [NegPy](https://github.com/marcinz606/NegPy) | GPL-3.0 | [16](16-film-scan-apps-survey.md) | 2026-11-19 | 0.69.0 (14 Nov): command-line launch and windowless export, Analysis Overlay, analog gain per channel for Nikon scanners, saturation in OKLab | Paper and H&D modelling, Auto Density/Grade, narrowband scanning, toners (lith, cyanotype) |
| [darktable](https://www.darktable.org), [Ansel](https://ansel.photos) | GPL-3 | [03](03-darktable-survey.md), [04](04-oss-ecosystem.md) | 2026-11-19 | 5.6.2 still the latest stable (nightlies only); Ansel unchanged since April | Major releases, new modules; Ansel's lens deblurring |
| [RawTherapee](https://rawtherapee.com), [ART](https://art.pixls.us) | GPL-3 | [04](04-oss-ecosystem.md) | 2026-11-19 | 5.13 still; ART's site still 404 | Demosaic and highlight recovery |
| [digiKam](https://www.digikam.org) | GPL-2+ | [04](04-oss-ecosystem.md) | 2026-11-19 | 9.1.0 still | Library and face tagging |
| [Safelight](https://discuss.pixls.us/t/introducing-safelight-a-free-open-source-raw-editor-built-like-an-ide/58615) | Open source (check) | — | 2026-11-19 | GPL-3.0; v1.0.x; its repo link wasn't in the thread | A raw editor "built like an IDE" |
| [LightZone](https://github.com/ktgw0316/LightZone) | BSD-3-Clause (check) | — | 2026-11-19 | 5.1.0 (13 Nov): needs SSE4.2; no new features | Revived old editor; zone-based tools |
| [Retoucan](https://alternativeto.net/software/retoucan/about/) | MIT | — | 2026-11-04 | New Lightroom Classic plug-in, ~0 stars | AI per-photo exposure and white balance "as you'd set them" |
| [LrGeniusAI](https://github.com/LrGenius/LrGeniusAI) | Open source | — | 2026-11-04 | Lightroom Classic plug-in | AI tagging, semantic search, AI edit (beta) |
| [Lightroom AI Bridge](https://github.com/aslanoov/lightroom-ai-bridge) | Open source | — | 2026-11-04 | macOS; Claude edits in a running Lightroom Classic | AI agents driving an editor (as our MCP) |
| [RAWmakase](https://github.com/pch/rawmakase) | MIT | [17](17-competitors-2026.md) | 2026-11-19 | 0.6.1 (17 Nov), ~1.7k stars: extensions (MCP, MIDI, AI masks as bundled extensions), HDR editing and HDR exports (JPEG XL, gain-map JPEG), AI Retouch extension (hosted models or LaMa on the CPU) | Closest to us in shape; AI object removal and culling help "coming soon"; MIDI control and scripting |
| [Imagin Raw](https://news.ycombinator.com/) | Open source (check) | — | 2026-11-11 | Show HN, Jul 2026; 9 MB SwiftUI raw browser and culler for Mac, XMP for Lightroom and Bridge; slow on big libraries (PhotoWorkout) | Speed on large folders; an iOS build |
| [PhotoCraft](https://letsdatascience.com/news/artcraft-updates-photocraft-its-open-source-rust-image-edito-d80bf804) (ArtCraft) | Apache-2.0 and MIT | — | 2026-11-11 | v0.8.2 (12 Oct), a clean-room Rust image editor (layers, masks, type); not ready for pros yet | A Photoshop-side companion, not a raw developer |
| [Happy Photon](https://discuss.pixls.us/tag/open-source) | Open source (check) | — | 2026-11-11 | pixls.us topic, active Oct 2026: "a simple photo workflow for RAW and JPEG" | Read its README next round |
| Greycard | Open source (check) | — | 2026-11-19 | pixls.us topic "an open source RAW photo editor", busy (132 replies) | Read its README next round |
| FerrumPix | Open source (check) | — | 2026-11-19 | pixls.us: a raw editor, viewer and gallery app, pitched to ART and RawTherapee users | Read its README next round |

## Closed and commercial

| Product | Price | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [Photon](https://getphoton.com) (Aftershoot) | Free, closed; paid for by Aftershoot | [17](17-competitors-2026.md) | 2026-11-19 | Page republished 17 Nov; same free feature list | The AI Coach: it explains why each adjustment helps; style suggestions |
| [FlashView](https://flashview.net) | One-off after a 60-day trial | [17](17-competitors-2026.md) | 2026-11-19 | No dated change | Fast culling |
| [Photon Studio](https://tenzen.studio) (Tenzen) | Free, donations | — | 2026-11-11 | Oct 2026: free Photoshop alternative (PSD, layers, Photoshop shortcuts), local AI selection and search; Windows, macOS, Linux | Not a raw developer; how a free closed suite funds itself |
| [Aphera](https://petapixel.com/2026/08/21/aphera-is-a-raw-editor-for-photographers-who-chase-feeling-over-formula/), [RevelRaw](https://www.indiehackers.com/product/revelraw) | Paid, macOS | — | 2026-11-11 | New small Mac raw editors (Aug 2026); pages refused us | Simple, "feeling over formula" editing; presets matched to the photo |
| [Tenzen Light Studio](https://tenzen.studio) | $59 once, a free plan | [17](17-competitors-2026.md) | 2026-11-19 | Same prices ($59, suite $89) | Why people pay for it |
| Capture One, DxO PhotoLab, ON1, Luminar Neo, Affinity, Exposure, ACDSee, Zoner, Pixelmator Pro/Photomator, DaVinci Resolve Photo, Pixieset | Paid or freemium | [09](09-market-commercial-alternatives.md) | 2026-11-19 (search) | Capture One 16.9.1 (Hasselblad X2D II tethering); PhotoLab 10.3 (Affinity files, per-area DeepPRIME choice); ON1 Photo RAW 2027 (2 Nov: new raw engine, NoNoise built in); Luminar Neo no release | New AI tools, Lightroom migration, price changes |
| Lightroom / Lightroom Classic | Subscription | [01](01-lightroom-feature-inventory.md), [13](13-adobe-community-ideas.md) | 2026-11-19 (from DPReview and reviews; Adobe's pages give 403) | Classic 16 (10 Nov): Glow (Diffusion, Bloom, Halation, Motion, Streak), vectorscope, colour grading per mask, Depth Range on any photo (AI depth), AI blemish removal, focus points, Projection, white balance down to 1,500 K; ACR 19 adds 3 LUTs and Generative Remove across photos | New tools and XMP tags to read |
| VSCO Studio Pro (desktop) | Free download; editing with VSCO Pro, from $5 a month | — | 2026-11-19 | New Mac raw editor (17 Nov): 200+ film looks (Portra, Kodachrome, Velvia, Tri-X), film-modelled grain, batch editing; culling in January, AI Remove in early 2027 | Film looks people pay for; no halation yet |
