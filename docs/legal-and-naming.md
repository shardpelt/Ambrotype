# Legal position and product name

Checked on 2026-10-04. This is an engineer's reading of the rules, not legal advice. Before 1.0, or before taking donations at any real scale, have an IP lawyer review it. A one-hour consult is usually enough.

## Status (2026-10-05): renamed to Ambrotype (for now)

- **Decision:** the project is renamed from "Flightbroom" to **Ambrotype**: "immortal impression", the 1850s glass photograph. Done in code and docs:
  - crates `amb-*`, CLI `amb`, desktop app `ambrotype`
  - XMP namespace `amb:` = `http://ns.ambrotype.org/develop/1.0/`
  - environment variables `AMB_*`
- **Still to do:**
  1. Search TMview and WIPO (classes 9 and 42) for "Ambrotype".
  2. Register ambrotype.org (also the domain in our XMP namespace URI) and/or ambrotype.photo.
  3. ~~Claim the GitHub organisation~~ Done: `Ambrotypehq` (`ambrotype` is a user account from about 2016, and `ambrotype-app` was taken too). Claim the crates.io names.

  5. ~~Rename the GitHub repository~~ Done: `shardpelt/Ambrotype` (old links redirect).
- **Reserving the name, in order** (checked 2026-10-07 by RDAP lookups: ambrotype.org, .photo, .dev and .io unregistered; .com taken since 2017; .app registered 2026-07-15; crates.io `ambrotype` free; the GitHub name `ambrotype` is an existing user account, so the organisation is `Ambrotypehq`):
  1. **Search the trademark registers** (TMview, WIPO, USPTO; classes 9 and 42). If someone already has "Ambrotype" for software, stop and pick an alternative before spending money.
  2. **Domains (same day, about €10–40 a year each):** ambrotype.org first (our XMP namespace and the app id `org.ambrotype.editor` depend on it; Flathub checks you own the domain behind an app id), then ambrotype.photo and ambrotype.dev. Turn on auto-renew and registrar lock.
  3. **Handles:**
     - ~~the GitHub organisation~~ done: `Ambrotypehq`, kept as a reserved name; the code stays at `shardpelt/Ambrotype` for now (decided 2026-10-07). Once the trademark is registered, GitHub's trademark policy is the way to ask for `ambrotype`
     - crates.io `ambrotype`: publish a real, minimal crate, not an empty placeholder (crates.io removes name-squatting)
     - Mastodon/Bluesky/X/Instagram/YouTube handles
     - an email address on the domain

  5. **Use it:** "Ambrotype™" until registered, "Ambrotype®" only after. Keep proof of first use (dated releases, the website).
  6. **Renewals:** domains yearly; a trademark lasts 10 years and is renewable.
- **Alternatives kept in case a check fails:** Photochrom, Argentype (§2).
- **Licence (decided 2026-10-08):** GPL-3.0-or-later, copyright "The Ambrotype contributors"; the plugin SDK and the `amb plugin new` template are MIT OR Apache-2.0. Done:
  - `LICENSE` (the GPL text from gnu.org), `NOTICE` (copyright, exceptions, ported code, library notes, trademarks), SDK `LICENSE-MIT` and `LICENSE-APACHE`
  - `license` in every Cargo.toml, and an SPDX line at the top of every source file
  - About tab in the app (copyright, no warranty, the licence and notices readable offline, a link to the source); the CLI's help says the same
  - every dependency checked: all GPL-compatible (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, LGPL-2.1 for rawler); CI now checks this on every push (`cargo deny check licenses sources`, allowed list in `deny.toml`)
- **Before going public:**
  1. ~~Paraphrase the verbatim Adobe quotes in `docs/adobe-docs/`.~~ Done 2026-10-11 (see §1).
  2. Make the repository public (the GPL promises the source to everyone who gets the app).
- **Releases** (when installers are built):
  1. Ship `LICENSE`, `NOTICE` and the licence texts of all bundled libraries with every installer (generate them with `cargo about`; MIT, BSD and Apache require the copyright notices to travel with binaries).
  2. Each release points to the exact source it was built from (the Git tag).
  3. Add `cargo deny` to CI so a new dependency with an incompatible licence fails the build.

## 1. Is what we do legal?

| Activity | Position | Why |
|---|---|---|
| Reading Adobe's public help pages | ✅ Fine | Reading is never infringement. Writing down what features do, in our own words, is fine too. Ideas, functionality and behaviour are not protected by copyright. |
| Committing those notes publicly | ✅ Fine (fixed 2026-10-11) | The digests in `docs/adobe-docs/` and the research notes are in our own words. A scan on 2026-10-11 compared every text file in the repo with the 230 archived help pages, the specs, the SDK sources and the 907 pages collected on 2026-10-10: no line shares a run of 12 or more words with a source. The only quotes left are short UI labels and menu names, document titles, Adobe's required DNG licence notices and our own text. Ideas, functionality and behaviour are not protected; the raw page text was never committed. Re-run the check before each public release. |
| Reading and writing `.xmp`, `.lrtemplate` and DNG files | ✅ Fine | Data formats and functionality are not protected by copyright (CJEU *SAS Institute v World Programming*, C‑406/10). XMP is an open ISO standard (ISO 16684). Adobe publishes the DNG spec with a royalty-free patent licence for readers and writers. That licence is lost if you sue Adobe over DNG patents, which we won't. |
| Using a Lightroom subscription to compare outputs (render, measure, fit our own constants) | ✅ Fine | This is black-box testing: run the program normally and observe its output. Under the EU Software Directive (2009/24/EC art. 5(3)), a lawful user may "observe, study or test the functioning of the program", and art. 8 makes contract terms that forbid this void. Adobe's terms forbid reverse engineering in the decompiling sense, which we never do. |
| Decompiling or disassembling Lightroom/ACR, or extracting its profiles, LUTs or code | ❌ Never | It breaches Adobe's terms, it isn't needed, and it would taint the code. The project rule stands: no Adobe binaries, profiles (DCP), LCP lens files, presets or code in the repo or the downloads. |
| Publishing calibration renders made with Lightroom | ⚠️ Keep private | You own the copyright in your own photos and their renders, but there's no reason to publish bulk Adobe output. Publish only our fitted numbers (`--calibration` JSON), which are our own measurements. |
| Saying "imports Lightroom® presets" | ✅ Fine | Nominative, descriptive use. Add the notice "Lightroom is a trademark of Adobe Inc.; not affiliated with or endorsed by Adobe." Don't use Adobe logos, icons, UI artwork or colour scheme. |
| Users loading presets they bought | ✅ Their right | Users load their own files, as they would in Lightroom. We never distribute purchased presets (and none are in the repo). |
| Porting darktable/RawTherapee/ART code | ✅ Fine under GPL-3.0-or-later | Keep their copyright headers and credit them. RapidRAW is AGPL: read it for ideas only, don't copy code. |
| Patents | ⚠️ Low, not zero | Software patents mostly bite in the US. darktable, RawTherapee and others have shipped similar features for 15+ years without action. We use published academic methods, not Adobe's. Never read Adobe patents "to see how they do it". |


**Before making the repo public:**
1. ~~Add the `LICENSE` file (GPL-3.0-or-later) and set `license` in every Cargo.toml.~~ Done 2026-10-08.
2. ~~Paraphrase the verbatim quotes in `docs/adobe-docs/`.~~ Done 2026-10-11.
3. ~~Put the trademark notice in the README and the app's About box.~~ Done 2026-10-08.
4. ~~Pick the final name (below) and rename.~~ Done: Ambrotype.

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


