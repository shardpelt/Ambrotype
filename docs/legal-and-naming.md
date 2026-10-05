# Legal position and product name

Checked on 2026-10-04. This is an engineer's reading of the rules, not legal advice. Before 1.0, or before taking donations at any real scale, have an IP lawyer review it. A one-hour consult is usually enough.

## 1. Is what we do legal?

| Activity | Position | Why |
|---|---|---|
| Reading Adobe's public help pages | ✅ Fine | Reading is never infringement. Writing down what features do, in our own words, is fine too. Ideas, functionality and behaviour are not protected by copyright. |
| Committing those notes publicly | ⚠️ **Fix before going public** | `docs/adobe-docs/` has about 1,000 verbatim quotes (~12,000 words) from Adobe's help. Short quotes for reference fall under the quotation right (US fair use), but this volume is beyond what's comfortable. Paraphrase the quotes and keep only short ones where the exact wording matters (e.g. menu names). The raw mirrored pages were never committed, which was correct. |
| Reading and writing `.xmp`, `.lrtemplate` and DNG files | ✅ Fine | Data formats and functionality are not protected by copyright (CJEU *SAS Institute v World Programming*, C‑406/10). XMP is an open ISO standard (ISO 16684). Adobe publishes the DNG spec with a royalty-free patent licence for readers and writers. That licence is lost if you sue Adobe over DNG patents, which we won't. |
| Using a Lightroom subscription to compare outputs (render, measure, fit our own constants) | ✅ Fine | This is black-box testing: run the program normally and observe its output. Under the EU Software Directive (2009/24/EC art. 5(3)), a lawful user may "observe, study or test the functioning of the program", and art. 8 makes contract terms that forbid this void. Adobe's terms forbid reverse engineering in the decompiling sense, which we never do. |
| Decompiling or disassembling Lightroom/ACR, or extracting its profiles, LUTs or code | ❌ Never | It breaches Adobe's terms, it isn't needed, and it would taint the code. The project rule stands: no Adobe binaries, profiles (DCP), LCP lens files, presets or code in the repo or the downloads. |
| Publishing calibration renders made with Lightroom | ⚠️ Keep private | You own the copyright in your own photos and their renders, but there's no reason to publish bulk Adobe output. Publish only our fitted numbers (`--calibration` JSON), which are our own measurements. |
| Saying "imports Lightroom® presets" | ✅ Fine | Nominative, descriptive use. Add the notice "Lightroom is a trademark of Adobe Inc.; not affiliated with or endorsed by Adobe." Don't use Adobe logos, icons, UI artwork or colour scheme. |
| Users loading presets they bought | ✅ Their right | Users load their own files, as they would in Lightroom. We never distribute purchased presets (and none are in the repo). |
| Porting darktable/RawTherapee/ART code | ✅ Fine under GPL-3.0-or-later | Keep their copyright headers and credit them. RapidRAW is AGPL: read it for ideas only, don't copy code. |
| Patents | ⚠️ Low, not zero | Software patents mostly bite in the US. darktable, RawTherapee and others have shipped similar features for 15+ years without action. We use published academic methods, not Adobe's. Never read Adobe patents "to see how they do it". |


**Before making the repo public:**
1. Add the `LICENSE` file (GPL-3.0-or-later) and set `license` in every Cargo.toml.
2. Paraphrase the verbatim quotes in `docs/adobe-docs/`.
3. Put the trademark notice in the README and the app's About box.
4. Pick the final name (below) and rename.

## 2. The name

The name "Flightbroom" works, but it plays on "Lightroom" in the same market. Adobe can argue it trades on their mark, even as a joke. A rename now costs one afternoon; after launch it costs users, links and search ranking.

What makes a name safe: it's distinctive for software (an invented word, or a real word unrelated to software), it isn't close to an existing photo-software brand (Lightroom, Luminar, Capture One, Exposure, Darkroom, Halide, Obscura, SilverFast, Ansel, ...), and its domain and handles are free.

### Candidates checked

| Name | Meaning | Conflicts found | Free domains (RDAP) | Verdict |
|---|---|---|---|---|
| **Lightfast** | "Lightfast" means resistant to fading, the archival standard for prints and pigments: light that lasts. "Fast" also means firm/fixed (as in steadfast) and quick. | No live US trademark (only dead marks in cosmetics and internet access). No software or app with this name found. Crate name free. Closest mark: **SilverFast** (LaserSoft, scanner software), a different first word but the same "-fast" in a neighbouring field. | lightfast.photo, lightfast.io (.com/.org/.app are taken) | **Recommended.** Check EU registers (§3), then claim it. |
| Lightstead | light + steadfast/homestead | A family-organiser app called "Lightstead" exists (different goods). | lightstead.org, .app, .photo, .io | Good fallback. |
| Lucerna | Latin for lamp | Two small software agencies use the name. | lucerna.photo, lucernaapp.org | Possible, but a weaker story. |
| Heliora | sun + aura | Two photo sun-tracker apps use it. | | ❌ Too close. |
| Heliograph | Niépce's first photograph | A file-transfer app, a NYC software firm, Héliographe Studio (photo apps). | | ❌ Crowded. |
| Halation | the film glow effect | "Halation" is a Windows image-editing app. | | ❌ Taken. |
| Stillroom | still photography + room | At least 8 apps use it, including photo apps. | | ❌ Crowded. |
| Lightkeeper | the lighthouse keeper | Lightkeeper (fintech platform) has software marks. | | ❌ |
| Camera Lucida | Latin for "light room", and Barthes' classic book on photography | Drawing-aid apps, and Lucida is a registered font brand. | | ❌ Lovely, but risky. |

### Names that don't start with "Light" (checked 2026-10-05)

| Name | Meaning | Conflicts found | Free domains (RDAP) | Verdict |
|---|---|---|---|---|
| **Ambrotype** | A positive photograph on glass (1850s). From Greek *ambrotos* "immortal" + *typos* "impression": an image that lasts forever. | No software or app found; one photography studio ("Ambrotype Visuals"). Crate name free. | ambrotype.org, ambrotype.photo | **Top pick.** It means what we want, and it's a real photographic word nobody uses for software. |
| **Argentype** | Silver + impression; the silver halide that photography captures light in. A variant of Herschel's 1842 "argentotype". | Only historical and art-term uses. Crate free. | argentype.org, .app, .photo | Strong. Solid, unusual and ownable. |
| Albumen | The albumen print, the dominant 19th-century print process. | None found in software. | albumen.org, albumen.photo | Possible, but it also means egg white. |
| Cyanotype / Collodion | Historic processes | Used generically as effect names in many filters and plug-ins. | .app, .photo | Weak as marks (descriptive in photo software). |
| Safelight | The darkroom lamp | **A new free GPL RAW editor named Safelight launched in Sept 2026.** | | ❌ Taken by a direct competitor. |
| Emulsion | The light-sensitive layer | "Emulsion" is a Mac photo app. | | ❌ |
| Phos, Argent, Lodestar, Vesper, Heliotrope, Daguerre, Platina, Aurum | | Crate names taken, and domains/apps crowded. | | ❌ |
| Sunprint | Cyanotype "sun prints" | Printer app plus a managed-print company. | | ❌ |

### Names about the job itself (capturing a signal, assigning colour), checked 2026-10-05

| Name | Meaning | Conflicts found | Free domains (RDAP) | Verdict |
|---|---|---|---|---|
| **Photochrom** | The 1890s Swiss process that turned black-and-white negatives into colour pictures: colour assigned to captured light, which is what a raw developer does. | Only an unpublished rooted-phone camera app and an unrelated NFT project ("PhotoChromic"). Crate free. | photochrom.org, .app, .photo | **Strong.** Exact meaning, historic, ownable. |
| Chromara | Coined: chroma + -ara | No exact match, but the "Chroma" prefix is crowded (Razer Chroma, Chroma DB). | .org, .app, .photo | Possible. |
| Rawsmith, Huewright | Coined: "the one who forges raw files / colours" | None found. | .org, .app, .photo | Free, but they sound like tools, not a lasting brand. |
| Photosite | The light-sensitive element of a sensor | None found in software. | .photo | Reads like "photo site" (a website). |
| Chromogen | Colour-forming development (C-41) | **A film-emulation RAW editor named Chromogen exists** (chromogen.app). | | ❌ Direct competitor. |
| Autochrome | The Lumière colour process; its dyed starch grains were the ancestor of today's Bayer colour mosaic | An iOS filter app and an online editor use it. | | ❌ |
| Trichrome | Three-channel colour photography | Several apps (Trichromatic, Trichromy). | | ❌ |
| Opsin | The eye's light-to-signal pigments | SIONYX OPSIN, a digital night-vision camera with an app (class 9). | | ❌ |
| Colorfast | Colour that doesn't fade | NewBlue ColorFast, a colour-grading plug-in. | | ❌ |
| Tincture, Ochre, Iridis, Tessera, Vermilion, Sinopia | | Crate names taken, or crowded. | | ❌ |

Trademark databases (TMview, WIPO) weren't reachable from this session; run step 3.1 below for the final pick.

## 3. Do you need to register it?



1. **Search (free, ~15 minutes):**

   - [WIPO Global Brand Database](https://branddb.wipo.int/).
   - [USPTO search](https://tmsearch.uspto.gov/).
   - Look for identical and similar marks in **class 9** (downloadable software) and **class 42** (software services).
2. **Claim the handles (today, free or cheap):** the GitHub organisation, the domain(s), the crates.io name (publish a placeholder crate) and the social handles.
3. **Register:**

   - **EU (EUIPO):** €850 for one class, +€50 for a second. Covers all 27 member states, but only needed if it takes off.
   - A US filing only becomes relevant much later.
   - Classes 9 and 42 cover it. Registration lasts 10 years and can be renewed indefinitely.
4. **Optional:** a short lawyer check of the search results (~€150–300) before paying the filing fee.


