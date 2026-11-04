# Watchlist: projects we researched

Projects we surveyed, with where they stood when we last looked, so we can check them again and see what changed. Recheck about once a month, or when someone points at one.

**How to recheck.** Read the README, the changelog or release notes, and the website. Don't read their code (see [legal.md](../legal.md)). Then:
1. Update "Last checked" and "Where it was".
2. Add anything new worth taking to that project's survey, and to [WHATS-NEXT.md](../WHATS-NEXT.md) if it's worth doing soon.
3. Add a new project here when a survey brings one in.

## Open source

| Project | Licence | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [LightCraft](https://github.com/storytold/lightcraft) | MIT or Apache-2.0 | [17](17-competitors-2026.md) | 2026-11-04 | Alpha, ~15.5k stars, no releases; also a WebAssembly build | SAM 3 Object/Describe masks, AI Denoise (Bayer, in progress), MCP server, print layouts, contact sheet PDF |
| [CalibRaw](https://github.com/Duecki1/CalibRaw) | GPL-3.0-or-later | [17](17-competitors-2026.md) | 2026-11-04 | v1.4.0 (30 Oct), ~290 stars; Linux, Windows, macOS, Android (F-Droid) | Guided perspective, People mask, depth and object masks, RawNIND denoise. Taken so far (our own builds): tiled GPU exports from free memory, Edit Replay, auto defringe, highlight roll-off, AI Remove |
| [Rembrandt](https://github.com/thesnarkitecht/rembrandt) | GPL-3 | [17](17-competitors-2026.md) | 2026-11-04 | ~200 stars; Tauri + WebGL2/WebGPU; unsigned builds | Super Resolution, Generative Remove and Expand, Refocus, edits asked in words, Edit like me, Lightroom catalogue import |
| [Redlamp](https://github.com/pdcgomes/redlamp) | MPL-2.0 | [17](17-competitors-2026.md) | 2026-11-04 | ~370 stars, README updated 2 Nov; macOS 26 on Apple Silicon only | 36 film simulations, focus stacking, SAM 2.1 object masks (SAM 3 under Meta's own licence), Depth Anything sky and depth, Generative Remove, dust removal |
| [RapidRAW](https://github.com/CyberTimon/RapidRAW) | AGPL-3.0 | [07](07-rapidraw-survey.md), [04](04-oss-ecosystem.md) | 2026-11-04 | v1.6.7 (25 Oct), ~10.9k stars; Windows, macOS, Linux, Android | New since Oct: AI Text mask ("person", "car"), face-part masks, GPU AI (DirectML, CoreML), 2.5D relight with shadows and fog, Auto Straighten, Immich, paid cloud ($14/month) for generative cleanup, AI-free mode |
| [negaflow](https://github.com/habinsong/negaflow) | Apache-2.0 | [16](16-film-scan-apps-survey.md) | 2026-11-04 | 1.2.6; macOS, Windows | Negative conversion, GrainMend dust and scratches |
| [NegPy](https://github.com/marcinz606/NegPy) | GPL-3.0 | [16](16-film-scan-apps-survey.md) | 2026-11-04 | ~1.1k stars | Paper and H&D modelling, Auto Density/Grade, narrowband scanning, toners (lith, cyanotype) |
| [darktable](https://www.darktable.org), [Ansel](https://ansel.photos) | GPL-3 | [03](03-darktable-survey.md), [04](04-oss-ecosystem.md) | 2026-11-04 | darktable 5.6.2 (4 Oct, fixes and cameras); Ansel no dated release | Major releases, new modules; Ansel's lens deblurring |
| [RawTherapee](https://rawtherapee.com), [ART](https://art.pixls.us) | GPL-3 | [04](04-oss-ecosystem.md) | 2026-11-04 | RawTherapee 5.13 (26 Jul: Rec2020 working space, Capture Deconvolution); ART's site gave 404 | Demosaic and highlight recovery |
| [digiKam](https://www.digikam.org) | GPL-2+ | [04](04-oss-ecosystem.md) | 2026-11-04 | 9.1.0 (7 Jun); natural-language search (GSoC) | Library and face tagging |
| [Safelight](https://discuss.pixls.us/tag/open-source) | Open source (check) | — | 2026-11-04 | Announced on pixls.us 2026-08-05, "built like an IDE" | New; read its README next round |
| [LightZone](https://github.com/ktgw0316/LightZone) | BSD-3-Clause (check) | — | 2026-11-04 | 5.0.0 (23 May 2026) | Revived old editor; zone-based tools |
| [Retoucan](https://alternativeto.net/software/retoucan/about/) | MIT | — | 2026-11-04 | New Lightroom Classic plug-in, ~0 stars | AI per-photo exposure and white balance "as you'd set them" |
| [LrGeniusAI](https://github.com/LrGenius/LrGeniusAI) | Open source | — | 2026-11-04 | Lightroom Classic plug-in | AI tagging, semantic search, AI edit (beta) |
| [Lightroom AI Bridge](https://github.com/aslanoov/lightroom-ai-bridge) | Open source | — | 2026-11-04 | macOS; Claude edits in a running Lightroom Classic | AI agents driving an editor (as our MCP) |

## Closed and commercial

| Product | Price | Survey | Last checked | Where it was | Watch for |
|---|---|---|---|---|---|
| [Photon](https://getphoton.com) (Aftershoot) | Free, closed; paid for by Aftershoot | [17](17-competitors-2026.md) | 2026-11-04 | Page updated 31 Oct 2026; launched September 2026 (MODE, Minneapolis); macOS, Windows; offline; some tools "will be included" | The AI Coach: it explains why each adjustment helps; style suggestions; reviewers: only three options, slow lessons |
| [FlashView](https://flashview.net) | One-off after a 60-day trial | [17](17-competitors-2026.md) | 2026-11-04 | Windows only; editing mode, PSD and Affinity round-trip | Fast culling |
| [Tenzen Light Studio](https://tenzen.studio) | $59 once, a free plan | [17](17-competitors-2026.md) | 2026-11-04 | Windows, macOS, Linux; in-place .lrcat import, local AI masks | Why people pay for it |
| Capture One, DxO PhotoLab, ON1, Luminar Neo, Affinity, Exposure, ACDSee, Zoner, Pixelmator Pro/Photomator, DaVinci Resolve Photo, Pixieset | Paid or freemium | [09](09-market-commercial-alternatives.md) | 2026-10-03 | See the survey | New AI tools, Lightroom migration, price changes |
| Lightroom / Lightroom Classic | Subscription | [01](01-lightroom-feature-inventory.md), [13](13-adobe-community-ideas.md) | 2026-10-03 (what's new page gave 403 on 2026-11-04) | See the inventory | New tools and XMP tags to read |
