# 09 - Commercial and freemium alternatives to Lightroom (market survey, 2025-2026)

Status: researched 2026-10-03 via WebSearch/WebFetch. Reddit deliberately excluded (covered elsewhere).

**Source caveat.** A large share of "Lightroom alternative" content is vendor SEO. Imagen AI, Skylum, ON1, Aftershoot, aiarty and many preset shops publish comparison posts that rank their own products first. Those are cited only for facts (price, feature lists) and tagged `[vendor]`. Some fetches (Capture One support FAQ, Adobe help, Phoblographer) returned 403/402, so those claims come from search snippets and are tagged `[unverified]`. Prices are US list prices as reported at the cited date and change often.

**Reading guide.** "Migration" means three separate things, and the market treats them very differently:

1. **Catalog import**: read `.lrcat` (folders, collections, ratings, keywords, edits).
2. **XMP import**: read Lightroom develop settings (`crs:` namespace) from sidecars or embedded XMP, so edits survive without the catalog.
3. **Preset import**: install `.xmp` (modern) or `.lrtemplate` (legacy) presets and profiles and get the same look.

---

## Part 1 - Desktop products

### Capture One (Pro / Mobile; Express discontinued)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Pro about $18/mo annual after a 6% rise effective 2 Jun 2026; All-in-One (adds iPad/iPhone, cloud) about $24.75-25/mo annual; Studio about $48.50/mo; perpetual licence still sold, also hit by the rise. Express discontinued. | Win, macOS; iPad and iPhone (via All-in-One) | Catalogs and Sessions (own database) | Yes, built in (File > Import Catalog > Lightroom Catalog) | Not documented | No. Third-party converter only |

- **Pricing.** Two consecutive 6% increases (Mar 2025, Jun 2026) with "no major accompanying feature launch" is the main reviewer grievance. Newsshooter says the price now exceeds the whole Adobe plan for one tool. https://www.newsshooter.com/2026/06/01/capture-one-raises-prices-again-is-it-still-worth-it-or-time-to-jump-ship/ and https://petapixel.com/2026/05/27/capture-one-to-increase-all-product-prices-by-6/
- **Catalog import: vendor claim.** Per the vendor, the import brings over ratings, colour labels, keywords, IPTC data and a few basic edits such as white balance, exposure and saturation. The guide says nothing about XMP files or presets. https://www.captureone.com/blog/step-by-step-guide-from-lightroom-to-capture-one
- **Catalog import: reviewers.** Shotkit confirms ratings, labels, exposure and metadata survive but not everything carries over, mainly colour adjustments, because HSL and color grading map poorly onto C1's Color Editor. Test with a small collection first. Collections become Albums and Collection Sets become Groups, but Smart Collections are not imported (search-summary of C1 support FAQ, `[unverified]`; the FAQ itself returned 403). https://shotkit.com/switch-from-lightroom-to-capture-one/ and https://support.captureone.com/hc/en-us/articles/360003692438-Switching-from-Lightroom-to-Capture-One-FAQ
- **Preset import.** Lightroom `.xmp`/`.lrtemplate` are not compatible with C1 Styles (`.costyle`). The de facto path is Picture Instruments Preset Converter ($46, Win/macOS). It takes `.xmp`/`.lrtemplate` and emits `.costyle`/`.costylepack`, mapping functions "according to optical standards"; white balance is the main source of discrepancy and manual fine-tuning may be needed. https://picture-instruments.com/products/?lang=en&id=17 ; https://presets.io/a/blog/lightroom-to-capture-one-preset-conversion-guide
- **Standout and praise.** Colour tools, tethering, medium-format support, layers; rated about 4.5/5 in aggregator reviews `[vendor-adjacent]`. https://www.softwarehow.com/capture-one-pro-review/
- **Complaints.** Price and increases; Capterra reviewers cite the lack of a preset ecosystem versus Lightroom, a cluttered Develop UI, and slow import/export. https://www.capterra.com/p/200094/Capture-One-Enterprise/reviews/
- **AI.** Modest: AI masking and selection; no generative tools.

### DxO PhotoLab 9 + PureRAW 5 + FilmPack

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| PhotoLab 9 $239.99 new, $119.99 upgrade (from v7-8), perpetual. PureRAW 5 sold separately (perpetual). FilmPack separate or bundled. | Win, macOS (no iPad) | No database. Reads the folder tree live; "Projects" as albums. Edits in `.dop` sidecars | No | Ratings, colour labels and keywords only; develop settings ignored | No |

- **Migration reality.** DxO ignores Lightroom adjustments; only star ratings are recognised. Lightroom edits are visible only if you export TIFF/DNG. It can read keywords from XMP, provided Lightroom writes sidecars. https://forum.dxo.com/t/moving-from-lightroom-to-dxo-import-the-xmp-files/16530 ; https://forum.dxo.com/t/importing-lightroom-processe-imges-in-dxo/35164 (forum posts and search summary, `[unverified]` on current v9 behaviour).
- **Library.** Folder-based "live" view praised for zero import friction; criticised for weak keywording (one keyword at a time), no search within Projects, and indexed folders losing moved images. https://blog.luxborealis.com/2025/10/08/dxo-photolab-9-vs-lightroom-part-1-interface-and-library/ ; https://lifeafterphotoshop.com/dxo-photolab-9-review/
- **AI and quality.** DeepPRIME / DeepPRIME XD3 denoise is "still the best by far" (Life after Photoshop). PhotoLab 9 adds AI masking. Lens-correction profiles from DxO labs are a core selling point. https://lifeafterphotoshop.com/dxo-photolab-9-review/
- **Complaints.** Some Windows users report AI masking failures and 5+ minute processing. DxO gives no release-date notice, which annoys recent buyers. PureRAW round-tripped DNGs can lose the original camera profile in Lightroom. https://community.adobe.com/questions-675/dxo-pureraw-5-import-to-lr-missing-profiles-983557
- **Positioning.** Often used as a companion to Lightroom (PureRAW plug-in) rather than a replacement. https://fstoppers.com/reviews/review-dxo-pureraw-5-698924

### ON1 Photo RAW 2026 (and MAX)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Perpetual $99 (Photo RAW) / $199 (MAX); upgrades about $80; subscription from about $7.99/mo | Win, macOS; separate mobile app | Browse folders or catalog (hybrid) | Yes: "Lightroom Migration Assistant" | Not stated | Not stated for LR presets. LUTs supported |

- **Vendor claim.** The Migration Assistant transfers "catalogs, folders, metadata, and edits" (basic adjustments such as exposure). The page is silent on XMP sidecars and Lightroom preset import. https://www.on1.com/lightroom-alternative/ `[vendor]` ; https://www.on1.com/on1-vs-lightroom/ `[vendor]`
- **Reviewer view.** Digital Camera World's 2026 review does not test migration. It praises scope and value (it costs less than half of one year of the Adobe Photography Plan) and criticises muddled browsing versus cataloguing, a simplistic Home screen and confusing masking options. https://www.digitalcameraworld.com/cameras/on1-photo-raw-2026-review-its-hard-not-to-like-this-affordable-do-it-all-photo-editing-program ; TechRadar also reviews it: https://www.techradar.com/pro/software-services/on1-photo-raw-review
- **AI.** AI masking (Super Select), Brilliance AI, Resize AI, NoNoise, auto-keywording, Restore AI (MAX).
- **Reputation.** Broad all-in-one, good value. Historically criticised for slower performance and rough edges `[unverified for 2026]`.

### Luminar Neo (Skylum)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Lifetime about $119 (cross-device $159); Pro subscription about $119/yr; major AI add-ons sometimes sold separately | Win, macOS (also plug-in for Photoshop/LR Classic/Photos) | Light catalog; added "catalogs" and LR migration in 2026 | Yes, new in v1.27.1 (Jun 2026); needs LR Classic 15+ | Not stated | Not stated |

- **Vendor claim.** Migrates "photos, folders, albums, ratings, and basic edits" (exposure, white balance, lights, shadows). Masks, AI selections and geometry/perspective edits are not transferred. https://tuxoche.com/2026/06/25/luminar-neo-1-27-1-import-lightroom-catalogs-now-a-first-practical-test/ (search summary, `[unverified]`)
- **Reviewer caveat.** Fstoppers says migrated photos may look slightly different in Luminar Neo than in Lightroom because the rendering engine differs, which Skylum describes as expected behaviour; unsupported-camera RAWs appear but may not open. The author had not tested a migration. https://fstoppers.com/apps/skylum-adds-lightroom-library-import-luminar-neo-caveat-903070
- **AI.** The headline feature: sky/relight/portrait AI, AI Assistant suggesting presets. Reviewers describe it as shortcut-oriented rather than a darkroom. https://www.culturedkiwi.com/luminar-review/ `[vendor-adjacent]`
- **Complaints.** Trustpilot: "perpetual" licences ending when support ends, removed features, paid upgrades, slow replies. https://www.trustpilot.com/review/skylum.com

### Affinity by Canva (Affinity Photo 2 successor)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| **Free** (single app merging Photo, Designer, Publisher, Oct 2025). Needs a free Canva account. AI (Generative Fill/Expand, Remove Background) needs Canva Premium ($120/yr) | Win, macOS; iPad promised 2026 | None | No | No | No (LUTs only) |

- **What changed.** Canva made the whole app "free, forever", with AI features behind Canva Premium. https://www.dpreview.com/news/5229499714/affinity-is-now-one-app-that-s-completely-free-forever-mostly/ ; https://www.canva.com/newsroom/news/all-new-affinity/ ; critical take: https://wilkinson.graphics/blog/2025-11-01-affinity-is-free/
- **Photo role.** RAW develop, layers, retouching. Reviewers call it a Photoshop replacement, not a Lightroom replacement: no catalog/DAM, weak for high-volume workflows. Lightroom presets cannot be used, because the RAW engine differs, though LUTs work (shotkit; summary). https://shotkit.com/affinity-photo-review/
- **Newer.** The Phoblographer published a review of Affinity version 3.3 asking whether it can replace Lightroom (15 Sep 2026); the page returned 402, so contents `[unverified]`. https://www.thephoblographer.com/2026/09/15/affinity-version-3-3-review/
- **Complaints.** Canva account requirement, AI paywall, trust concerns about Canva (wilkinson.graphics).

### DaVinci Resolve 21 "Photo page" (2026 newcomer)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Free; Studio $295 (AI masking etc. reserved for Studio) | Win, macOS, Linux [unverified for the Photo page] | Albums and a LightBox browser | Yes (claimed), details unspecified | Unknown | No. LUTs/OpenFX supported |

- **Claim and reviews.** Out of beta 3 Jun 2026. PetaPixel lists Lightroom catalog import but not what transfers. Engadget finds it viable for casual users, but says it lacks some advanced features, particularly in organisation and exporting (no watermarking, content credentials, export post-processing) and recommends pros stay with Lightroom for now. https://petapixel.com/2026/04/13/davinci-resolve-21-is-now-a-lightroom-alternative-raw-editing-tethering-masking-and-more/ ; https://www.engadget.com/apps/davinci-resolve-21-hands-on-a-viable-lightroom-alternative-for-casual-users-160520123.html
- **Significance.** The most credible free and well-funded entrant; node-based grading is a different mental model from Lightroom sliders.

### Pixieset Photo Editor (2026 newcomer, sponsored coverage)

- Free for culling/editing with 100 export credits; $12/mo (annual) or $18/mo for unlimited exports. Raw editing (curves, masking, healing), AI culling, "Personal AI Styles" learned from your Pixieset galleries; processing is local. No mention of Lightroom catalog or preset import. Source is a sponsored Fstoppers piece, so treat as marketing. https://fstoppers.com/sponsored/pixieset-photo-editor-lightroom-alternative-photographers-903950

### Exposure X7 (Exposure Software, ex-Alien Skin)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Perpetual about $129 (bundle $149); latest build 7.2.0.25, Jan 2026. No X8 found | Win, macOS; plug-in for Photoshop/LR | No catalog: point at folders | Listed as "Lightroom migration" in reviews (X7 era) | Unknown | Own presets only [unverified] |

- Review coverage is mostly 2022-24 (DPReview, Life after Photoshop, Shotkit); little 2025-26 independent coverage found, so the product looks to be in maintenance mode `[unverified]`. https://www.dpreview.com/reviews/exposure-x7-software-review-more-powerful-masking-and-a-ui-that-adapts-to-your-needs ; https://expertphotography.com/exposure-x7-review
- Strength: film-look presets and a plug-in workflow. Weakness: no catalog, thin AI.

### ACDSee Photo Studio Ultimate 2026

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| $89/yr or $8.90/mo subscription; perpetual about $150 (often discounted) | Win, macOS (Win is the primary platform) | Folder-based browser plus optional database | Not found in any 2026 review | Reads XMP metadata `[unverified]` | Not found |

- 2026 headline is "AI Develop Presets" (adjustments plus AI masking) and claimed up to 100% faster RAW decoding. Reviewers call it a worthy Lightroom competitor with new features inside a familiar workflow. https://www.techradar.com/reviews/acdsee-photo-studio-ultimate-review ; https://www.digitalcameraworld.com/tech/software/acdsee-photo-studio-ultimate-2026-review ; https://shotkit.com/acdsee-photo-studio-review/
- Complaints (recurring): dated, dense UI; Windows-first. Treat as `[unverified]` for 2026.

### Zoner Studio 2026

- Subscription $5.99/mo or $59/yr (family $98/yr); Windows only. Organiser plus RAW plus layer editor. TechRadar and Creative Bloq: "heavy on image editing, light on AI". Recognises keywords assigned in Lightroom; no Lightroom catalog importer was found. https://www.creativebloq.com/photography/photo-editing-software/zoner-studio-2026-review-all-in-one-image-editing-with-a-splash-of-video-too ; https://www.techradar.com/pro/software-services/zoner-studio-photo-editor-review

### Pixelmator Pro and Photomator (Apple)

| Pricing | Platforms | Catalog/DAM | LR catalog import | LR XMP import | LR preset import |
|---|---|---|---|---|---|
| Pixelmator Pro in **Apple Creator Studio** ($12.99/mo or $129/yr, launched 28 Jan 2026, 10 apps). Photomator is **excluded**; standalone $7.99/mo or $29.99/yr | macOS, iPadOS (Pixelmator Pro iPad); Photomator Mac/iOS/iPad | Photomator sits on the Apple Photos library | No | No | Not natively; LUTs work |

- **Status.** Apple closed the acquisition in Feb 2025 and discontinued Pixelmator iOS. Photomator is described as having "no updates" beyond bug fixes, with a Creator Studio version rumoured but unconfirmed. The 29 Sep 2026 Creator Studio update covered Pixelmator Pro (iPad Select and Mask, templates) with no Photomator mention. https://9to5mac.com/2025/02/11/apple-officially-owns-pixelmator-and-photomator/ ; https://sixcolors.com/post/2026/01/whats-the-real-value-in-creator-studio/ ; https://www.macrumors.com/2026/09/29/apple-creator-studio-new-features/ ; https://daringfireball.net/2026/01/thoughts_and_observations_regarding_apple_creator_studio
- **Presets.** Lightroom's formats are not imported; Photomator takes LUTs (`.cube`) and Pixelmator's own "Color Adjustments" presets. https://bodega.supply/blog/news/how-to-import-your-presets-packs-into-pixelmator-and-photomator-aka-color-adjustments `[vendor: preset shop]`
- **Praise.** Fast, native, good ML masking and denoise, cheap, Apple-silicon performance. Complaint: Apple-only and an uncertain roadmap.

### Apple Photos

- Free, bundled with Apple platforms. RAW support, basic and curves/selective colour tools, iCloud sync. Not a catalogue replacement for pros (no deep metadata, no preset ecosystem, locked to Apple). No Lightroom import (the Apple Photos library can receive exported files only). https://www.makeuseof.com/apple-photos-vs-adobe-lightroom/ `[low-quality source]`

### Excire Foto / Excire Search (Lightroom plug-in)

- Excire Foto 2026 is a local AI DAM for search and keywording; Excire Search 2026 is the Lightroom Classic plug-in version. It builds its own catalog and does not edit files. This is a **companion to Lightroom**, not a replacement, and it shows that AI search is a gap in Lightroom Classic. https://expertphotography.com/excire-foto-review ; https://www.shutterbug.com/content/excire-foto-launches-search-2026-lrc ; https://www.ephotozine.com/article/excire-search-2026-review-37524

### Mylio Photos+

- Photos+ $9.99/mo or $99/yr (a June 2026 aggregator claims about $20/mo for new individual plans; conflicting `[unverified]`). Win, macOS, iOS, Android. Local-first multi-device sync with no cloud storage fee; local face recognition and Smart Tags. Links to existing folders. No Lightroom catalog import found. Editing is basic. Digital Camera World and DPS praise browse speed and privacy. https://www.digitalcameraworld.com/reviews/mylio-photos-review ; https://digital-photography-school.com/mylio-photos-review/ ; https://fstoppers.com/post-production/mylio-photos-relaunching-free-app-631791

---

## Part 2 - AI tools in the Lightroom ecosystem

| Product | Pricing | Platforms | Lightroom relationship | Notes |
|---|---|---|---|---|
| **Imagen AI** | Pay-per-photo about $0.22-0.32; free first 250 photos; newer flat "Limitless" plan (vendor blog) | Win, macOS (desktop app) plus cloud processing | Reads and writes Lightroom Classic; outputs XMP-based edits into the LR catalog | "Personal AI Profile" trained on your past edited catalog; culling, crop, straighten, subject mask |
| **Aftershoot** | Subscription from about $20/mo for retouching; flat-rate | Win, macOS | Works with Lightroom Classic and Capture One via XMP | Culling plus editing "profiles" plus retouch; wedding/school volume |
| **Evoto** | Credit-based: edit free, one credit per export; about $6.99-9.99/mo entry | Win, macOS, mobile | Standalone; not an LR plug-in | Portrait retouching |

- **Why it matters for Ambrotype.** Imagen and Aftershoot are **parasitic on Lightroom's catalog and XMP**: they write develop settings into LR's format. They prove that the LR XMP/`crs:` schema is the de facto interchange format for professional AI culling and editing. A competitor that reads and writes it becomes a drop-in host for these tools. Sources are mostly vendor blogs `[vendor]`: https://aftershoot.com/blog/ai-editing-software-for-photographers/ ; https://phaito.com/blog/comparing-ai-tools-phaito-imagen-aftershoot-luminar-evoto ; https://www.capterra.com/p/10015499/Evoto-AI/ . Independent review depth is thin; platform and "reads XMP" details `[unverified]`.
- **Complaints (generic to AI tools).** Per-image pricing anxiety (Imagen), cloud-upload privacy concerns (Pixieset pointedly advertises local processing), and AI culling that "complements, not replaces" human judgement.

---

## Part 3 - Mobile

| App | Pricing | Platforms | Lightroom-preset story | Notes |
|---|---|---|---|---|
| **Lightroom mobile** | Free tier with many tools; paid plan for cloud sync, masking, healing, RAW, batch (older vs current gating differs by source) | iOS, Android | **Native** XMP/DNG preset import. Blog claims the free tier now imports XMP/DNG presets `[unverified]; Adobe help page 403` | The reference. Presets and profiles are the monetised ecosystem |
| **Snapseed** | Free (Google) | iOS, Android | None (own "Looks"); supports RAW/DNG | Fast, free; effectively unmaintained feel `[unverified]` |
| **VSCO** | Plus $39.99/yr (App Store, Aug 2026); Pro $60/yr; **VSCO One $499.99/yr** (Jun 2026) | iOS, Android; Studio Pro on iOS only, macOS due later 2026 | VSCO Film Lightroom presets now exclusive to VSCO One. Studio Pro lacks RAW | PetaPixel notes it currently has no RAW support and lacks the robust editing tools professionals expect. https://petapixel.com/2026/06/23/why-vsco-believes-its-new-500-subscription-is-worth-it-for-pros/ ; App Store 4.7 (about 272k ratings) vs a verified-reviews aggregate of 3.8 `[low-quality]` |
| **Darkroom (iOS)** | Darkroom+ $9.99/mo, $39.99/yr, or $99.99 lifetime | iOS, iPadOS, macOS | Own presets and LUT import; no LR preset import `[unverified]` | Built on Apple Photos library; AI smart presets. https://darkroomsoftware.com/pricing/ |
| **Halide / Process Zero** | Halide subscription or one-time (iOS) `[unverified]` | iOS | n/a | A **camera** app (focus assist, RAW, Process Zero no-AI processing), not an editor |
| **Photomator iOS** | $7.99/mo or $29.99/yr | iOS/iPadOS | LUTs only | See above |
| **RNI Films** | Lightroom preset packs about $164 (bundle); RNI Films app separate | LR desktop/mobile packs; own iOS app | **The reference third-party Lightroom preset vendor.** Ships profiles (not sliders), so presets leave the sliders alone | Fstoppers, Phoblographer and others have reviewed RNI packs; RNI updated packs in Jan 2025 for more cameras. https://www.thephoblographer.com/2025/01/17/rni-films-lightroom-update/ ; https://fstoppers.com/reviews/analog-look-lightroom-review-rnis-all-films-pack-554798 |

---

## Part 4 - Synthesis

### (a) Comparison against Lightroom

Legend: Y = supported and documented; P = partial or basic edits only; N = none found; ? = not found or unverified. "LR" = Lightroom Classic.

| Product | Price model | Win/mac/Linux | Mobile | Own catalog | LR catalog import | LR XMP develop import | LR preset import | AI |
|---|---|---|---|---|---|---|---|---|
| **Lightroom (ref.)** | Subscription; Photography Plan about $19.99/mo for new subs, LR 1TB $14.99/mo from 20 Mar 2026 (Fstoppers; Imagen blog `[vendor]`; Newsshooter cites $10.99, conflicting) | Win, mac | iOS, Android | Y | n/a | n/a | n/a (native) | Strong (generative remove, masks, denoise) |
| Capture One | Sub about $18/mo or perpetual | Win, mac | iPad/iPhone | Y | **P** (metadata, WB, exposure, saturation) | ? | N (third-party $46 converter) | Modest |
| DxO PhotoLab 9 | Perpetual $240 | Win, mac | N | N (folders) | N | N (ratings/keywords only) | N | Strong (denoise, masks) |
| ON1 Photo RAW 2026 | Perpetual $99-199 or sub | Win, mac | App | Hybrid | **P** (claimed edits) | ? | ? (LUTs) | Strong |
| Luminar Neo | Lifetime about $119 or sub | Win, mac | N | Light | **P** (new Jun 2026; look differs) | ? | ? | Strongest, AI-first |
| Affinity (Canva) | **Free**, AI paywalled | Win, mac | iPad soon | N | N | N | N (LUT) | Paid via Canva |
| DaVinci Resolve 21 Photo | Free / $295 | Win, mac, Linux `[unverified]` | N | Albums | **P** (claimed) | ? | N (LUT/OpenFX) | Studio-only |
| ACDSee Ultimate | Sub $89/yr or perpetual | Win, mac | N | Folders plus DB | ? | ? | ? | Growing |
| Exposure X7 | Perpetual $129 | Win, mac | N | N | P? (X7 era) | ? | ? | Thin |
| Zoner Studio | Sub $59/yr | Win | N | Folders | ? | ? | ? | Light |
| Photomator / Pixelmator Pro | $29.99/yr / in $129/yr bundle | mac, iPadOS | iOS | Photos library | N | N | N (LUT) | ML masks |
| Apple Photos | Free | Apple only | iOS | Y | N | N | N | Some |
| Mylio / Excire | $99/yr / paid | Win, mac (Mylio mobile too) | Mylio Y | Y | ? | ? | N/A | Local AI search |
| Imagen / Aftershoot / Evoto | per-photo / sub / credits | Win, mac | Evoto | N (use LR/C1) | n/a | **Write LR XMP** | n/a | Core product |
| VSCO | $40-500/yr | iOS, Android (mac soon) | Y | N | N | N | LR presets for One subs | Some |
| Snapseed / Darkroom | Free / $40/yr | mobile | Y | N | N | N | N | Light |

**Read-across.** Every desktop contender that offers any migration offers it at the catalog level, and every one of them transfers only the "basic" sliders. **No product in this survey claims to import Lightroom `.xmp` presets and render them faithfully.** The only credible path is the third-party Picture Instruments converter for Capture One, which itself concedes white-balance drift.

### (b) Why users leave Lightroom, and why they come back

**Why they leave** (Fstoppers "Why Photographers Are Leaving Adobe in 2026": https://fstoppers.com/software/real-reason-photographers-are-leaving-adobe-901527):

- Price: the Photography Plan went from $9.99 to $19.99/mo for new subscribers in 2025, and Lightroom's cloud plan rose again in March 2026. Adobe's legacy 20 GB plans are closed to new buyers (Imagen, `[vendor]`; secondary).
- Performance: Lightroom Classic is reported to lag in folder navigation, delay in the Develop module and export slowly. The article rates performance as the same or worse.
- Unwanted AI: generative features dominate the roadmap while "stability" requests go unanswered.
- Lock-in: catalog lock-in is described as the biggest barrier, though the pain of switching is finite while the Adobe subscription is forever.
- Subscription fatigue is the explicit draw of perpetual products (ON1, DxO, ACDSee, Luminar).

**Why they come back or never fully leave** (partly inference; the survey found few direct "I returned" accounts, and Fstoppers itself says "few photographers return"):

- The catalog and preset ecosystem is not portable, so a partial switch leaves an LR catalog still in use. Capterra reviewers of Capture One cite the missing preset ecosystem as a lock-in. https://www.capterra.com/p/200094/Capture-One-Enterprise/reviews/
- Migrated images "appear slightly different" (Luminar); colour edits do not translate (Capture One). Visual drift on years of edited work is a trust-breaking event.
- Library depth: DxO's folder-based library lacks keywording and search tools; ON1's "muddled browsing vs cataloguing"; Affinity and Exposure have no catalog at all.
- Mobile and sync: Lightroom is the only desktop-plus-mobile-plus-cloud pipeline most competitors lack (Capture One's mobile needs the All-in-One tier; VSCO has no RAW).
- Ecosystem plug-ins (Imagen, Aftershoot, Excire, PureRAW) are built around Lightroom Classic.
- Competitors are also raising prices (Capture One two 6% rises; VSCO One at $499.99/yr; Skylum "perpetual" complaints), which weakens the cost argument.

### (c) The best-regarded preset and Lightroom migration paths, and how they work

| Path | How it works | What users report |
|---|---|---|
| **Lightroom to Capture One catalog import** | Built-in importer reads the `.lrcat`. Pulls ratings, labels, keywords, IPTC, plus basic edits (WB, exposure, saturation, crop). Collections become Albums | Metadata is trusted. Colour and HSL edits do not transfer. "Test with a small collection first" (Shotkit). Smart Collections dropped. https://shotkit.com/switch-from-lightroom-to-capture-one/ |
| **Preset Converter (Picture Instruments, $46) to Capture One Styles** | Parses `.xmp`/`.lrtemplate`, maps each `crs:` setting to the nearest C1 tool, writes `.costyle`/`.costylepack`; batch | Photographers call it "works, even with elaborate VSCO presets" (vendor-curated quote); WB is the main discrepancy and manual tuning is needed. This is the most visible **commercial** preset-compat product. https://picture-instruments.com/products/?lang=en&id=17 |
| **ON1 and Luminar migration assistants** | Importer reads the `.lrcat`, rebuilds basic develop settings on the app's own engine | Marketing says "edits transfer"; the one independent caveat says looks differ because of engine differences. Neither is reported to import LR presets |
| **LUT export/import** | Bake a look into a 3D LUT (`.cube`) and load it in Affinity, Photomator, ON1, Resolve | Universal and portable, but loses slider-based editability and cannot represent non-colour settings (grain, vignette, tone curve per channel are only partly representable) |
| **Profiles, not presets (RNI, Adobe)** | Profiles apply a base look and leave the sliders at zero | RNI promotes this explicitly; users value that the sliders keep their full range. https://www.thephoblographer.com/2025/01/17/rni-films-lightroom-update/ |
| **XMP sidecars as interchange** | `crs:` settings written by Lightroom and read by Adobe tools, plus AI vendors (Imagen, Aftershoot) | Used heavily in AI pipelines. DxO ignores it beyond ratings/keywords/labels (forum threads cited above) |
| **Lightroom mobile preset import** | Import `.xmp`/`.dng` preset on-device | Reference UX for preset buyers; a blog claims the free tier now supports it `[unverified]` |

**What users expect:** (1) drop an `.xmp` pack in and see the same look, including per-camera profile behaviour; (2) keep star ratings, colour labels, keywords, collections, and fold-in of edits; (3) not have previously edited images shift colour; (4) run the whole thing from a single "Import from Lightroom" wizard that reports what could not be mapped.

### (d) The market gap Ambrotype can own

1. **Faithful preset compatibility is unsolved.** Nobody, free or paid, imports Lightroom presets natively and renders them faithfully. The best answer is a $46 third-party converter for one product with known WB drift. This is the stated differentiator and the market confirms it is open.
2. **Faithfulness is the missing metric.** Competitors say "basic edits transfer". Ambrotype could publish a conformance suite (golden renders per setting) and a per-setting mapping report, turning faithfulness into something measurable.
3. **Free, cross-platform, and a real catalog.** The free options either lack a catalog (Affinity, Exposure, DxO's folder-only), are feature-limited (Resolve's Photo page: weak organisation and export), or are mobile-only. The only free tool with a serious library and migration story is not yet there.
4. **Linux.** No commercial desktop contender besides Resolve (unverified for Photo) targets Linux. Mobile-plus-desktop parity is also thin: Capture One gates mobile, VSCO has no RAW, Photomator is stalling.
5. **Trust and ownership.** Commercial users are being squeezed by repeated price rises (Capture One, Adobe, VSCO One), AI paywalls (Affinity/Canva) and "perpetual" licences that lapse (Skylum). An open-source tool with open formats speaks to this directly.
6. **XMP/DAM interchange for AI tools.** Imagen/Aftershoot use LR XMP. Reading and writing `crs:` XMP makes Ambrotype a possible host for them.

### (e) Design lessons

**Copy**

- A single **"Import from Lightroom" wizard** (Capture One, ON1, Luminar all converge on this). Add a **post-import fidelity report**: exactly which settings, masks and presets mapped, approximated or dropped.
- **Import metadata first, always**: ratings, labels, keywords, collections, flags. That part is universally trusted. Preserve `.xmp` sidecars so users can roll back.
- **Preset formats**: accept `.xmp` (and legacy `.lrtemplate`), profiles, and `.cube` LUTs. Treat profiles as base looks and presets as slider deltas, as Lightroom and RNI do.
- **Folder-live browsing** (DxO, Exposure): zero-import start and no lock-in. Combine it with an optional database.
- **Local-first AI** (Mylio, Excire, Pixieset's marketing): processing on the device, with a privacy statement.
- **Sidecar interoperability** with AI culling vendors: that is a feature, not a risk.
- Capture One's "Styles vs Presets" distinction and a before/after view are well liked.

**Avoid**

- Claiming that edits transfer when only exposure and WB do. The colour drift Skylum itself calls expected behaviour and Capture One's colour-edit loss are the lived experience. Be explicit about drift and where it comes from.
- **Silent colour shifts** on previously edited images. Provide a reference-render comparison and let users pick "match Lightroom" or "native" rendering.
- Folder-only libraries with weak keywording and search (DxO) or "muddled browsing vs cataloguing" (ON1). Keep catalog and browse modes clearly separated.
- **Price and licence gimmicks**: "lifetime" licences that expire, feature removal, paywalled AI that was free. State the licence terms plainly (open-source licence, no account required; contrast Affinity's mandatory Canva account).
- Unstable, slow import of very large catalogs. Shotkit explicitly advises against importing 500k+ images first; design for incremental, resumable, background import.
- Leaving mobile as an afterthought, or building it without RAW (VSCO). Presets are bought on phones, so mobile compatibility matters.
- A node-based or novel UI as the only mental model (Resolve). Keep the Lightroom sliders and module layout familiar for switchers.

---

## Appendix - Open items and unverified claims

- Capture One FAQ, Adobe mobile FAQ, Skylum support page and Phoblographer Affinity 3.3 review could not be fetched (403/402). Claims drawn from them are `[unverified]`.
- No source states which Lightroom settings Resolve 21's catalog import carries. Needs a hands-on test.
- ON1's current behaviour with `.xmp` Lightroom presets, and ACDSee's and Zoner's Lightroom catalog import, were not found in any 2025-26 review. Check vendor docs.
- Exposure: no 2026 independent reviews found; roadmap unclear.
- Photomator's roadmap past Creator Studio is unannounced.
- Lightroom pricing figures conflict between sources (Fstoppers $19.99 Photography Plan vs Newsshooter's $10.99 for three apps; Imagen's $14.99 LR 1TB). Confirm on adobe.com before quoting.
- App Store / Play Store rating checks were limited to VSCO (4.7 from search; low-quality aggregator gave 3.8). Other mobile ratings were not retrieved.
- Trustpilot/Capterra evidence was limited to Skylum and Capture One snippets; G2 not opened.


