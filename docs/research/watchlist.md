# Watchlist: projects we researched

Projects we surveyed, with where they stood when we last looked, so we can check them again and see what changed. Recheck about once a month, or when someone points at one.

**How to recheck.** Read the README, the changelog or release notes, and the website. Don't read their code (see [legal.md](../legal.md)). Then:
1. Update "Last checked" and "Where it was".
2. Add anything new worth taking to that project's survey, and to [WHATS-NEXT.md](../WHATS-NEXT.md) if it's worth doing soon.
3. Add a new project here when a survey brings one in.

Reddit can't be read by our tools (blocked for automated readers since 2026-11); ideas from there come by hand (paste a thread) or via pixls.us, Hacker News and articles that quote it.

## Open source

| Project | Licence | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [LightCraft](https://github.com/storytold/lightcraft) | MIT or Apache-2.0 | [17](17-competitors-2026.md) | 2026-11-11 | Alpha, ~15.9k stars, still no releases; HDR and panorama merge, WebAssembly build, MCP server | SAM 3 Object/Describe masks, AI Denoise (Bayer, in progress), MCP server, print layouts, contact sheet PDF |
| [CalibRaw](https://github.com/Duecki1/CalibRaw) | GPL-3.0-or-later | [17](17-competitors-2026.md) | 2026-11-11 | v1.4.1 (9 Nov): colour calibration, AI denoise, carousel export, Linux .deb, Google Play | Guided perspective, People mask, depth and object masks, RawNIND denoise. Taken so far (our own builds): tiled GPU exports from free memory, Edit Replay, auto defringe, highlight roll-off, AI Remove |
| [Rembrandt](https://github.com/thesnarkitecht/rembrandt) | GPL-3 | [17](17-competitors-2026.md) | 2026-11-11 | ~206 stars, undated README; no visible change | Super Resolution, Generative Remove and Expand, Refocus, edits asked in words, Edit like me, Lightroom catalogue import |
| [Redlamp](https://github.com/pdcgomes/redlamp) | MPL-2.0 | [17](17-competitors-2026.md) | 2026-11-11 | 0.3.5, ~384 stars; JPEG XL export (0.3.1), a Library (0.3.0); macOS 26 on Apple Silicon only | 36 film simulations, focus stacking, SAM 2.1 object masks (SAM 3 under Meta's own licence), Depth Anything sky and depth, Generative Remove, dust removal |
| [RapidRAW](https://github.com/CyberTimon/RapidRAW) | AGPL-3.0 | [07](07-rapidraw-survey.md), [04](04-oss-ecosystem.md) | 2026-11-11 | v1.6.9 (7 Nov): Black & White mode (film responses, colour filters, toning), Motion Blur, Speed Edit (hold W and drag on the photo), rebuilt Clone/Heal/Liquify, Match Brush mask, a second-window viewer | AI Text mask, face-part masks, GPU AI, 2.5D relight, Auto Straighten, Immich, paid cloud generative cleanup, AI-free mode |
| [negaflow](https://github.com/habinsong/negaflow) | Apache-2.0 | [16](16-film-scan-apps-survey.md) | 2026-11-11 | 1.2.6 (3 Nov, film target fixes); macOS, Windows | Negative conversion, GrainMend dust and scratches |
| [NegPy](https://github.com/marcinz606/NegPy) | GPL-3.0 | [16](16-film-scan-apps-survey.md) | 2026-11-11 | 0.68.0 (8 Nov): Fade Restoration, B&W slide toggle, manual Dmin/Dmax picking, roll average, frame-number sort | Paper and H&D modelling, Auto Density/Grade, narrowband scanning, toners (lith, cyanotype) |
| [darktable](https://www.darktable.org), [Ansel](https://ansel.photos) | GPL-3 | [03](03-darktable-survey.md), [04](04-oss-ecosystem.md) | 2026-11-11 | darktable 5.6.2 (4 Oct, fixes); nothing newer; Ansel not rechecked | Major releases, new modules; Ansel's lens deblurring |
| [RawTherapee](https://rawtherapee.com), [ART](https://art.pixls.us) | GPL-3 | [04](04-oss-ecosystem.md) | 2026-11-04 | RawTherapee 5.13 (26 Jul: Rec2020 working space, Capture Deconvolution); ART's site gave 404 | Demosaic and highlight recovery |
| [digiKam](https://www.digikam.org) | GPL-2+ | [04](04-oss-ecosystem.md) | 2026-11-04 | 9.1.0 (7 Jun); natural-language search (GSoC) | Library and face tagging |
| [Safelight](https://discuss.pixls.us/t/introducing-safelight-a-free-open-source-raw-editor-built-like-an-ide/58615) | Open source (check) | — | 2026-11-11 | v1.0.x since June 2026, Electron; its pixls.us thread is busy (280 replies) | A raw editor "built like an IDE" |
| [LightZone](https://github.com/ktgw0316/LightZone) | BSD-3-Clause (check) | — | 2026-11-11 | 5.0.3 (22 Oct, fixes) | Revived old editor; zone-based tools |
| [Retoucan](https://alternativeto.net/software/retoucan/about/) | MIT | — | 2026-11-04 | New Lightroom Classic plug-in, ~0 stars | AI per-photo exposure and white balance "as you'd set them" |
| [LrGeniusAI](https://github.com/LrGenius/LrGeniusAI) | Open source | — | 2026-11-04 | Lightroom Classic plug-in | AI tagging, semantic search, AI edit (beta) |
| [Lightroom AI Bridge](https://github.com/aslanoov/lightroom-ai-bridge) | Open source | — | 2026-11-04 | macOS; Claude edits in a running Lightroom Classic | AI agents driving an editor (as our MCP) |
| [RAWmakase](https://github.com/pch/rawmakase) | MIT | [17](17-competitors-2026.md) | 2026-11-11 | New (Sep 2026), pre-1.0, Rust; macOS, Linux, Windows (ARM too); .lrcat import, Lightroom XMP presets, DCP/XMP profiles, .cube LUTs, GPU preview, CLI, MCP server, MIDI | Closest to us in shape; AI object removal and culling help "coming soon"; MIDI control and scripting |
| [Imagin Raw](https://news.ycombinator.com/) | Open source (check) | — | 2026-11-11 | Show HN, Jul 2026; 9 MB SwiftUI raw browser and culler for Mac, XMP for Lightroom and Bridge; slow on big libraries (PhotoWorkout) | Speed on large folders; an iOS build |
| [PhotoCraft](https://letsdatascience.com/news/artcraft-updates-photocraft-its-open-source-rust-image-edito-d80bf804) (ArtCraft) | Apache-2.0 and MIT | — | 2026-11-11 | v0.8.2 (12 Oct), a clean-room Rust image editor (layers, masks, type); not ready for pros yet | A Photoshop-side companion, not a raw developer |
| [Happy Photon](https://discuss.pixls.us/tag/open-source) | Open source (check) | — | 2026-11-11 | pixls.us topic, active Oct 2026: "a simple photo workflow for RAW and JPEG" | Read its README next round |

## Closed and commercial

| Product | Price | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [Photon](https://getphoton.com) (Aftershoot) | Free, closed; paid for by Aftershoot | [17](17-competitors-2026.md) | 2026-11-11 | Page updated 10 Nov 2026; same feature list (Basic, HSL, noise, crop, presets, offline) and the AI Coach | The AI Coach: it explains why each adjustment helps; style suggestions |
| [FlashView](https://flashview.net) | One-off after a 60-day trial | [17](17-competitors-2026.md) | 2026-11-04 | Windows only; editing mode, PSD and Affinity round-trip | Fast culling |
| [Photon Studio](https://tenzen.studio) (Tenzen) | Free, donations | — | 2026-11-11 | Oct 2026: free Photoshop alternative (PSD, layers, Photoshop shortcuts), local AI selection and search; Windows, macOS, Linux | Not a raw developer; how a free closed suite funds itself |
| [Aphera](https://petapixel.com/2026/08/21/aphera-is-a-raw-editor-for-photographers-who-chase-feeling-over-formula/), [RevelRaw](https://www.indiehackers.com/product/revelraw) | Paid, macOS | — | 2026-11-11 | New small Mac raw editors (Aug 2026); pages refused us | Simple, "feeling over formula" editing; presets matched to the photo |
| [Tenzen Light Studio](https://tenzen.studio) | $59 once, a free plan | [17](17-competitors-2026.md) | 2026-11-11 | Now one of six "Creators Suite" apps (Photon Studio, a free Photoshop alternative, is theirs, not Aftershoot's) | Why people pay for it |
| Capture One, DxO PhotoLab, ON1, Luminar Neo, Affinity, Exposure, ACDSee, Zoner, Pixelmator Pro/Photomator, DaVinci Resolve Photo, Pixieset | Paid or freemium | [09](09-market-commercial-alternatives.md) | 2026-11-11 (search only) | No autumn 2026 releases found in search; last seen: PhotoLab 9 (AI masks), ON1 2026 (AI masks, super resolution), Luminar Neo interface and speed update | New AI tools, Lightroom migration, price changes |
| Lightroom / Lightroom Classic | Subscription | [01](01-lightroom-feature-inventory.md), [13](13-adobe-community-ideas.md) | 2026-11-11 (Adobe's pages give 403; from roundups) | Classic 15.5 (Aug 2026): Render to DNG, big metadata split into XMP and ACR sidecars, Assisted Culling; Generative Expand on Lightroom desktop and mobile; no autumn release found yet | New tools and XMP tags to read |
