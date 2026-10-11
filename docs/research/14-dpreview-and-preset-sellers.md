# 14 - DPReview forum threads and preset sellers' install pages: design implications for Ambrotype

Status: design input, 2026-10-10. Feeds [SCOPE.md](../SCOPE.md) §2 (vision and goals), §7 (preset compatibility), §8 (roadmap) and §10 (risks). It extends [12](12-user-feedback-and-design-implications.md) (Reddit) and [09](09-market-commercial-alternatives.md). Where this document repeats a point from 12, that is noted. Where it adds something 12 lacked, it is flagged **new**.

---

## 1. Method and caveats

**Method.** Two small source sets were collected on 2026-10-10 and digested by an extraction pass (one facts file per chunk, then this synthesis). This document did not re-read the raw pages in full.

- **Part A, DPReview.** DPReview blocks its own search for scripted requests, so threads were picked from the Retouching forum's most-replied list instead. That gave 14 threads: page 1 of each, and page 2 of three of them (17 pages, 50 posts per page). Chunks `i13dpreview-01` to `-15`. The topics are leaving Lightroom, Capture One vs Lightroom, darktable and DxO.
- **Part B, preset sellers.** Install, FAQ and troubleshooting pages from nine named sellers: Mastin Labs, DVLOP, Pretty Presets, Luxe Lens, Cole's Classroom, Totally Rad, Creative Market, ON1 and PHLEARN (12 pages, one chunk `i14sellers-01`). Four other sellers' guessed page addresses no longer exist and were skipped. Seller names are used because they are businesses. No forum usernames are used anywhere.

**Caveats.**
- **Old and small.** The 14 threads span 2017 to 2026: six from 2017, three from 2018-2019, one from 2020, and one each from 2023 (Lightroom Classic 12.3 AI Denoise), 2024, 2025 and 2026 (forced lens corrections). Early threads date from Lightroom's move to subscription, before AI tools. Product details (prices, camera support, what DxO or Capture One lack) are dated and many have since changed. Use the threads for motives and wording, not for a current feature comparison.
- **Selection bias.** "Most replied" favours arguments. Subscription anger, "I am leaving" threads and brand fights are over-represented. DPReview skews towards enthusiasts, stills shooters and long-time Lightroom owners. Preset buyers and wedding or event pros are rare here.
- **Counts.** "Several users" is the extraction's own signal wording for a point several posters made. It is not a tally. Nothing here is ranked by exact count. Claims inside posts (prices, product behaviour) were not verified, and some are disputed within the thread. Where a thread disputes a claim, that is said.
- **Sellers are not buyers.** Part B is what sellers tell buyers, plus the problems the sellers chose to list. It says nothing about how often each problem occurs. The pages are marketing-adjacent help pages, and each one covers a different set of Lightroom versions. Some sellers' pages are written for Lightroom 5 or 6 and some mention Lightroom Classic 15, so the version coverage is uneven.
- **Item 15 (app-store reviews) is a separate document.**

Sources: DPReview forum threads (Retouching forum), and the install, FAQ and troubleshooting pages of the nine sellers named above. Individual URLs are given per section below.

---

## 2. Part A: DPReview, why people leave or stay

### 2.1 Why people leave (or say they will)

1. **Subscription as renting** (the dominant theme, raised in most of the 14 threads). The wording is consistent over eight years.
   - Subscribers feel like renters: terms can change at any time, there is no way to stop paying and keep the version you have, and perpetual-licence users could skip versions to save money. [adobe-time-to-say-good-bye p1-2; cancelling-adobe-cc; why-all-the-hating-on-adobe]
   - Fear of the price rising once users are locked in appears in 2017 and again in 2025. In the 2025 thread, people compare the new monthly price with the old one and one says they will not return to Adobe over it. [adobe-time-to-say-good-bye p2; sometimes-it-feels-only-adobe-understands]
   - A request seen more than once: a subscription that becomes a perpetual licence after 12 months, with no further updates. [bye-bye-and-thank-you-adobe]
   - Defenders say it is good value for the Lightroom plus Photoshop bundle (about $120 a year in 2017). This is a split opinion, not a one-sided complaint.
   - Practical friction: no monthly Lightroom-only plan, cancellation fees, licence-check errors on annual plans, and the cheapest plan being hard to find on the pricing page. [cancelling-adobe-cc; why-all-the-hating-on-adobe]
   - Workarounds buyers describe: annual plan at Black Friday or from resellers, or keeping an older standalone version.
2. **What happens to my photos if I stop paying.** A repeated fear. Replies in two threads agree that Library, export and print keep working while Develop and Map stop. One thread notes a poster's claim that images become fully inaccessible was not confirmed by others. [bye-bye-and-thank-you-adobe; cancelling-adobe-cc; why-all-the-hating-on-adobe]
3. **Camera support tied to the app version.** In 2017, owners of older Elements and standalone Lightroom 6 could not open raw files from new cameras. This was read by posters as a push to upgrade. The usual workaround was Adobe's free DNG Converter, which others rejected: one-way, drops proprietary maker-note metadata, and, in their view, only swaps one proprietary raw format for another. [adobe-time-to-say-good-bye p1-2; beginner-question-alternatives p2] Fast support is also a reason to stay: Lightroom added the Canon R5/R6 about three weeks after release, which posters contrasted with DxO. [beginner-question-alternatives p1]
4. **Forced behaviour they cannot switch off.** The built-in lens profile that cannot be disabled led one user to leave for another program. In the 2026 lens-corrections thread, users say Lightroom follows the camera maker's "mandatory correction" hint and the in-camera crop, and cannot show the full frame; DxO can show the uncropped frame and Capture One can switch corrections off. [any-way-around-forced-lens-corrections; capture-one-11-compared-to-lightroom]
5. **The cloud direction.** Fear (2017-2019) that Classic would be dropped and imports forced into cloud storage. Replies say Classic 8.0 keeps originals on local drives. A 2024 thread mentions the outcry over AI scanning of cloud uploads, with one poster calling it overblown. [reality-check-on-impressions; capture-one-lightroom-dam; why-all-the-hating-on-adobe]
6. **Mandatory catalog.** Several users dislike importing into a catalog before editing and want "open a file, edit it, export" like a raw converter. A user with a perpetual Lightroom 6.14 asks for File Open and File Save with a sidecar. They prefer a folder tree. [beginner-question-alternatives p1-2; reality-check; sometimes-it-feels-only-adobe-understands]
7. **Performance.** Slow import and UI with 1000+ raw files after an event (2017); Library module slow; Windows slowdown after a few edited images that needs a restart (2025). Capture One and darktable are praised for speed. [capture-one-11-compared; adobe-time-to-say-good-bye p1; sometimes-it-feels-only-adobe-understands]
8. **Weak spots in the rendering.** Adobe Standard starts flat and skin tones look poor (several users, with fixes: set develop defaults once, use the camera's own profile, or build a custom profile with a ColorChecker). Fuji X-Trans detail lags other converters. Noise reduction was weaker than DxO's before AI Denoise. [capture-one-11-compared; no-alternative-to-lr-found p1-2; dxo-photolab-vs-lightroom-vs-captureone]

### 2.2 Why people stay

1. **The catalog and DAM, mentioned as the anchor in most threads.** Lightroom's DAM is called the strongest by users of Capture One, DxO and darktable. Cited specifics: collections and virtual copies, smart collections, keywording, offline-file handling, print and soft-proofing, folder search with about 180,000 photos. Bridge is described by one poster as a browser, not a true DAM, because it has no database. [beginner-question-alternatives; cancelling-adobe-cc; sometimes-it-feels-only-adobe-understands; capture-one-lightroom-dam]
2. **Lock-in is the edits, not the learning curve.** Users say replacing a raw editor is a short learning curve. The real trap is that Lightroom's develop edits do not carry over. One user with about 50,000 images says switching would mean re-editing most of them. [bye-bye-and-thank-you-adobe; no-alternative-to-lr-found p1]
3. **Camera profiles and the starting point.** One thread concludes that ColorChecker-based calibration and camera-matching profiles are the main reason posters stay, since alternatives without them give a worse starting point (a Canon file looked very red elsewhere). [reality-check-on-impressions]
4. **Bundle value and breadth.** "One package" for catalog, develop, retouching, presets and printing. The Photographer plan is called a Swiss Army knife. Mobile integration, a web presence, a DAM and printing are listed as must-haves that DxO plus Affinity lack. [no-alternative-to-lr-found p1; sometimes-it-feels-only-adobe-understands; why-all-the-hating-on-adobe]
5. **AI tools (2023 onward).** AI Denoise, masks, Generative Fill and Adaptive Color are said by several users to be ahead of Capture One. A 2024 poster says AI noise reduction removed the need for Topaz or DxO, trailing DxO only at very high ISO. [sometimes-it-feels-only-adobe-understands; why-all-the-hating-on-adobe]
6. **Camera support within weeks** (see §2.1 item 3).
7. **A lot of people keep Lightroom plus one other tool.** Common pairs: Lightroom with Capture One for editing, DxO or Topaz for denoise, Affinity or Photoshop for pixels, Qimage for printing, Bridge plus a converter. A user who tried many alternatives ended up subscribing and still uses Topaz Denoise AI beside it. [no-alternative-to-lr-found p2; beginner-question-alternatives p2; capture-one-11-compared]

### 2.3 What they compare it with

| Product | Praised for | Complained about | Notes |
|---|---|---|---|
| **Capture One** | Out-of-the-box colour (camera profile and noise reduction applied on import), speed, layers and colour-based masks, tethering, focus mask, customisable workspaces, perpetual licence feel | Weak catalog and DAM (strongest complaint, several threads): slow to open an 85,000-image catalog (52 s vs under 7 s in Lightroom, one poster), catalog corruption, no keyword synonyms, labels cannot be renamed. Proprietary terms (variant, recipe, output vs export). About $300 with no camera profiling. Missing lens profiles. Little edit data in XMP so edits are hard to move on | A catalog import from Lightroom keeps the look close, per one poster; another says the edits were not picked up. Often paired with Lightroom as DAM |
| **darktable** | Free; masking and blending in nearly every module; multiple instances; Filmic, Tone Equalizer, Color Balance; responsive UI; SQLite catalog with a darkroom view like Lightroom's; XMP sidecars shared with other programs | Steep learning curve with hidden shortcuts; modules hard to find ("more modules" group); export runs silently; weak DAM (no geotagging, face recognition, smart collections); only ICC camera profiles, not DNG/DCP; no print module on Windows; unwanted sidecar files in archive folders (can be turned off) | The "free choice for asset management plus raw conversion". A claim that it outperforms Lightroom's raw rendering is disputed, and other posters ask for data |
| **DxO PhotoLab / PureRAW** | Denoise (PRIME / DeepPRIME), lens corrections incl. JPEG, sidecar files that travel with the raw, fast | No Fuji X-Trans support (as posters said in 2017-2020), slow camera modules, no real DAM, no smart albums, no stacking, features locked behind add-ons (FilmPack, ViewPoint), price, default sharpening too early | Mostly used beside Lightroom as a denoise step. 12.3 thread: Lightroom Denoise now competitive |
| **RawTherapee / ART** | Detail rendering, DCP and LCP profile support, strong fine detail | Local adjustments limited; clunky UI | A stepping stone |
| **ON1, Exposure X, ACDSee, Luminar, Photo Ninja, Affinity** | Exposure X resembles Lightroom's layout (one switch took about two days); ON1 and Affinity as a bargain pair; ACDSee is file-explorer based and fast | A blogger's side-by-side found too many artifacts (Luminar, Exposure X5, ACDSee, Photo Ninja, Silkypix). ACDSee has no camera profiling or history; Luminar crashed and was JPEG-only in beta; Affinity's raw development judged weak | None replaced Lightroom for a user who tried them all |
| **Camera-maker software** | Accurate colour | Brand-locked | Rarely named |

Sources for §2.1 to §2.3: the 14 DPReview thread pages listed in the INDEX, Item 13 (for example https://www.dpreview.com/forums/threads/adobe-time-to-say-good-bye.4143771/ , https://www.dpreview.com/forums/threads/capture-one-lightroom-dam.4341487/ , https://www.dpreview.com/forums/threads/no-alternative-to-lr-found.4363776/ , https://www.dpreview.com/forums/threads/sometimes-it-feels-only-adobe-understands.4808889/).

### 2.4 Recurring complaints and praised features, with signal

**Recurring complaints (by number of threads that raise them)**

1. **Subscription terms** (most threads; several users in many). Same as 12 §2.2 item 2. **New:** the complaint is about control as much as price: the right to keep a version, to skip a version, and to know what stays usable after cancelling. The cancelled-subscription state (Develop and Map stop; Library and export keep working) is a concrete expectation people repeat.
2. **Catalog friction** (about 7 threads, several users). Same direction as 12 §2.2 item 3, but the angle differs: here the complaint is the mandatory import, not corruption or scale. **New:** how users hide the catalog today. Delete the catalog after exporting, run darktable with an in-memory library, or use an auto-import folder.
3. **Edits and sidecars are app-specific** (5+ threads). XMP edit data is plain text but only Adobe products can read the develop instructions. EXIF, IPTC keywords and ratings do travel. Exit routes people name: 16-bit TIFF, re-edit, or Capture One's catalog import. **New** as a first-hand statement of what a leaver actually gets.
4. **Lens and camera support** (6 threads): the forced built-in lens profile, new cameras needing a new app version, DNG Converter as a rejected fix.
5. **Performance** (4 threads): same as 12 §2.2 item 1, older and thinner here.
6. **Missing features people want:** local hue and saturation (HSL) in local adjustments; reusable and shareable masks; smart albums and stacking; a single-key reject; history panel; before/after. Capture One and DxO users miss the same Lightroom features (smart collections, stacking).
7. **Terminology and layout churn** in the tools people move to (Capture One variants and recipes; darktable's module groups). **New:** the vocabulary mismatch is itself a switching cost.

**Praised features (by number of threads)**

1. **DAM, collections, keywords, print** (about 7 threads).
2. **Value of the Lightroom plus Photoshop bundle** (about 5 threads).
3. **Camera profiles and colour starting point** (3-4 threads, as the main reason to stay).
4. **Denoise**: DxO PRIME as the benchmark through 2022; Lightroom's AI Denoise from 2023. 12.3 thread: about 2 minutes per image on one Windows PC, 6 to 45 seconds on others; Apple Silicon much faster; DNG intermediate files of 3 to 4 times the raw size (43.5 MB RAF to 178 MB DNG).
5. **Masking and local edits** as the Capture One and darktable strength; reportedly behind Lightroom's AI masks for sky and subject (Capture One has no sky AI, per a 2025 poster).
6. **Non-destructive denoise with sliders and no intermediate file** is praised as simpler than a separate package, a 2025 view. The 2023 thread complained about the DNG intermediate. Both are on the record.

---

## 3. Part A: what is new relative to doc 12

Items marked **new** are not in 12 (which is Reddit-only), and also add to 09 where noted.

1. **New: the cancelled-subscription state is a trust anchor.** Posters repeatedly want to know what still works after cancelling (Library, export, print keep working; Develop and Map stop). Ambrotype's "no subscription, edits stay yours" message has a concrete test: a leaver wants their edits readable and exportable at any time. This sharpens 12 §6 item 11.
2. **New: the catalog complaint is mandatory import, not only corruption.** People want "open any file, edit, export". 12 §6 item 3 already proposes folder-live first; the DPReview threads add that some users will not touch an app that forces catalog import at all. They also show users already faking a catalogless mode.
3. **New: Lightroom's lock-in is the develop edits.** Users report the learning curve is short and the edits are the trap. Same as 12 §3 ("hardly anyone reports a successful catalog-plus-preset migration"), but here with named workarounds (16-bit TIFF, DNG with embedded profiles, Capture One catalog import) and the key fact that **only metadata travels**.
4. **New: camera-support lag as a retention and a leaving reason.** In 2017 the lag pushed Elements and Lightroom 6 users out. In 2020, three weeks for the R5/R6 won praise against DxO. 12 §2.2 lists camera profile delays (item 10) for the Sony A7V but not the "older version gets no support" angle. Relevant to a perpetual or free tool: users expect new cameras to be supported without paying for the next version.
5. **New: forced lens correction.** Lightroom applies the maker's mandatory correction and crop, and the user cannot see the full frame. DxO and Capture One let the user override. This is a specific, cheap differentiator.
6. **New: the DNG Converter debate.** Users reject converting originals to DNG: one-way, loses maker-note metadata, and swaps one proprietary format for another. This is relevant to Ambrotype's own DNG-handling choices (SCOPE §9): never require conversion, keep originals untouched.
7. **New: users get a different "starting point" from each app.** Capture One applies its profile and noise reduction on import and looks better out of the box; Adobe Standard is called flat. This ties directly to the preset topic, where 12 §4.2 notes profile dependence (item 2).
8. **New: two-tool setups are normal.** Lightroom plus DxO/Topaz/Capture One/Affinity. Interop with a denoise or pixel editor is a feature, not a side issue (compare 12 §6 item 4).
9. **New (weak): vocabulary is a switching cost.** Capture One's own terms (variant, recipe) and darktable's module groups slowed learners. 12 §6 item 5 recommends Lightroom vocabulary; this is more evidence for it.

---

## 4. Part B: what preset sellers tell buyers

### 4.1 Formats and which app takes what

The following is what the nine sellers say. Version numbers are the sellers' own.

| Format | Where it works, per the sellers | Notes |
|---|---|---|
| **`.xmp` preset** | Lightroom Classic 7.3 and newer; Lightroom desktop (cloud app); mobile (iPhone, iPad, Android) imports `.xmp` directly. ON1 calls `.xmp` the standard for Classic CC 7.3 and newer. PHLEARN says it is what current versions import | The standard format. Lightroom Classic and Adobe Camera Raw share one presets folder on disk (per PHLEARN: under CameraRaw/Settings in the Adobe application-support/roaming folder), so a Classic install also shows in ACR |
| **`.lrtemplate`** | Legacy. ON1 says Classic accepts it. Creative Market says older Lightroom CC desktop copies `.lrtemplate` files to a User Presets folder and `.xmp` to a CameraRaw Settings folder. PHLEARN says mobile import also accepts `.lrtemplate` | Reaches mobile only by syncing from desktop with a subscription (Creative Market) |
| **DNG "mobile presets"** | The DNG is a sample photo with the preset applied. Phone user adds it to an album, opens it, and chooses Create Preset. Luxe Lens: works in the free mobile app, no Creative Cloud account or desktop needed. PHLEARN: DNGs are kept for old mobile versions | Not synced back to desktop (ON1). For Classic before 7.3, import the DNGs as photos and create a preset from each with every box ticked (PHLEARN) |
| **Profiles** | Mastin Labs ships mobile profiles that need an active Creative Cloud subscription and a desktop import; they then appear in the phone's Profile menu. Mastin Labs: Profile menu for JPEG, Preset menu for RAW | Presets and profiles are different import paths and different menus |
| **Brushes (local presets)** | Pretty Presets: brushes work in Lightroom 5, 6 and Classic, with separate install steps. Totally Rad: local adjustment presets are not in the Develop presets panel; they sit in the Effect setting of the Local Adjustment Brush | A separate category users need to know exists |
| **Zip packs** | Classic can import a pack straight from a `.zip`; Lightroom desktop and mobile need the files unzipped first (PHLEARN). Luxe Lens suggests iOS Files or ZipApp Lite to unzip | |

Versions named: PHLEARN and ON1 (Classic 7.3 and newer for `.xmp`); Mastin Labs (minimums Lightroom Classic 12 and Lightroom 6.0 for its presets and mobile profiles); Pretty Presets (everything works in Classic, CC, mobile and ACR, except its Evolution Series, which does not run in Lightroom 5 or 6; its Millennium Collections ship both DNG and XMP); and older Lightroom 4 or 5 packs may not import cleanly into current versions (ON1). Presets work on RAW and JPEG, but RAW gives more room (ON1, Pretty Presets).

### 4.2 What sellers say about installing and using

- **Classic import path.** Develop module, Presets panel, plus menu, Import Presets, select all the files (Mastin Labs, PHLEARN, ON1). A folder can be dragged onto the Presets panel (ON1). Mastin Labs also lists File > Import Profiles & Presets.
- **Use the seller's own installer.** DVLOP says its installer (a zip from the account page) installs the packs and importing directly through Lightroom is not recommended, with a Custom Install option that targets a catalog's settings folder.
- **Keep "Store presets with this catalog" unchecked.** Four sellers (Cole's Classroom, Pretty Presets, PHLEARN, DVLOP) say so, because with it checked the presets exist only inside one catalog.
- **Applying.** Apply to a neutral photo first, since presets stacked on old edits are harder to predict. Use the Amount slider to reduce strength. Copy settings across photos with Sync or Copy/Paste Settings (PHLEARN).
- **Sync to phone.** Desktop presets reach the phone only with sync on, the same Adobe ID, and a subscription. ON1 says enable it from File > Start Syncing and wait a few minutes for large libraries. Pretty Presets says syncing is from Lightroom CC (not Classic) in its framing. Creative Market recommends checking each listing for mobile support because DNG is the only format that works without a computer.
- **Backups are the buyer's job.** Pretty Presets says buyers must back up downloaded presets and links cannot be reissued after a loss, though it will try to help.

### 4.3 Problems sellers troubleshoot

All of these are in the sellers' own help text.

1. **Presets do not appear after import** (the most common entry, on DVLOP, PHLEARN, ON1, Cole's Classroom). Causes listed:
   - The panel is filtering: "Show Partially Compatible" must be on, or "Hide Partially Compatible Presets" off, then restart.
   - The panel is blank because no image is imported and selected, or the panel is collapsed or hidden, or the user is not in Develop.
   - A preset group arrives unchecked in Manage Presets and must be ticked, "especially after the Lightroom Classic 15 update" (PHLEARN).
2. **Presets vanish when another catalog is opened** (Store-with-catalog on) (Pretty Presets).
3. **Presets vanish after an update.** PHLEARN says Classic migrates them, so look for a collapsed group, then re-import from the original download.
4. **Greyed out or warning on a preset.** PHLEARN says this happens when a preset uses a raw camera profile on a JPEG.
5. **Old packs** for Lightroom 4 or 5 may not import cleanly (ON1).
6. **Installer problems.** DVLOP says an older installer reported success on a recent macOS but installed nothing, fixed by reinstalling with the newer installer.
7. **Unrecognised files.** Unzipped preset files appear as unrecognised media when Lightroom is not installed yet (Cole's Classroom).
8. **Mobile.** Presets do not reach the phone when the Adobe ID differs or sync is off; the photo-import screen dims `.xmp` files; a zipped pack cannot be read; DNG presets do not sync back to desktop (PHLEARN, ON1, Luxe Lens).
9. **Subscription cost framed as a limit.** Syncing `.xmp` or `.lrtemplate` to a phone needs an active Creative Cloud subscription. Luxe Lens sells DNG as the way around it.
10. **Version split.** Lightroom 5 and 6 vs Classic vs CC, and the "partially compatible" concept, are visible in many pages.

Sources: https://phlearn.com/install-lightroom-presets/ , https://www.on1.com/blog/how-to-install-lightroom-presets/ , https://support.mastinlabs.com/faq , https://support.mastinlabs.com/desktop-presets-lightroom/how-to-install-presets-lightroom , https://support.mastinlabs.com/mobile-profiles/how-to-import-mobile-profiles , https://support.dvlop.com/article/44-presets-are-not-appearing-in-lightroom , https://www.lightroompresets.com/pages/faq , https://luxelens.com/pages/how-to-install-lightroom-mobile-dng-presets , https://coles-classroom.helpscoutdocs.com/article/9-preset-installation-troubleshooting , https://support.gettotallyrad.com/legacy_products/lightroom_presets/troubleshooting.html , https://support.creativemarket.com/hc/en-us/articles/115000850454-Installing-Lightroom-Presets

### 4.4 What is new relative to doc 12 §4

Doc 12 §4 is about how buyers use presets and their quality complaints (adapts badly to camera and light, profile dependence, portability, loss). Part B is about delivery and installation. Almost all of it is **new**.

- **New: the import friction is large and visible.** About ten kinds of "my presets do not show up" cases on sellers' pages. The user's pain starts before the look is applied.
- **New: three formats, two eras.** `.xmp`, legacy `.lrtemplate` and DNG mobile presets are all live in sellers' packs. Many packs ship more than one. 12 §6 item 6 lists `.xmp`, `.lrtemplate` and zip packs; DNG-as-preset, brushes and profiles as a separate path are not in 12.
- **New: "partially compatible".** Lightroom's own filter hides presets that do not fully fit the user's version. Sellers' fixes rely on telling buyers to turn the filter off. This is the same issue as Ambrotype's fidelity report, seen from Adobe's side.
- **New: a preset can be profile-dependent in a second way.** Sellers say a preset using a raw camera profile is greyed out on a JPEG. 12 covers camera dependence only; this adds file-type dependence (RAW vs JPEG).
- **New: store-with-catalog.** Four sellers warn about it. It is a concrete way presets get "lost" (12 §4.2 item 6 only attributes loss to corruption and update incompatibility).
- **New: seller-supplied installers.** DVLOP ships its own installer because direct import is not reliable. Ambrotype's importer should assume the user already tried and failed once.
- **New: buyers back up their own files.** At least one seller says links cannot be reissued. A presets library with a source copy matters (12 §4.3 wants "keep presets safe in sidecars and backups").
- **New: mobile presets as a subscription workaround.** Luxe Lens markets DNG presets as a way to avoid Creative Cloud. This links the preset format to the same subscription anger as Part A.

---

## 5. Design implications for Ambrotype

Targets are proposals, not facts. Roadmap references are to SCOPE §8. Section numbers for 12 are its §6 items.

### 5.1 From Part A

1. **Edits must be readable and exportable at any time, and the message must say so.** Evidence: §2.1 item 2, the cancelled-subscription anxiety. Decision: the user's edits live in plain sidecars they can read and copy. Export never depends on a licence or an account. Ties to SCOPE §2 (goals) and 12 §6 item 11. Roadmap: already the default of the recipe model; the marketing line is **new**.
2. **A catalogless open-edit-export mode is a first-class path.** Evidence: §2.1 item 6, §2.4 item 2. Decision: "File > Open" any image, edit, export, with a sidecar next to the file and no import step. 12 §6 item 3 already says folder-live first, and this confirms it with first-hand demand. The optional catalog sits behind it. Roadmap: Phase 1 CLI and Phase 2 app (**new** emphasis: do not ship an import-only first run).
3. **Leaving Lightroom: metadata is the part that travels; say so, and do better.** Evidence: §2.2 item 2, §2.4 item 3. Decision: the migration wizard in 12 §6 item 12 tells the user up front what carries over (ratings, keywords, labels, collections) and what is approximated (develop edits), and offers the 16-bit TIFF snapshot users already rely on as a safety net for edits that cannot be mapped. **New:** a "render a TIFF or full-size JPEG of every edited image alongside the import" option as an explicit safety copy. Roadmap: Phase 3 with the `.lrcat` import.
4. **Camera and lens support: expect new cameras without a paid upgrade.** Evidence: §2.1 items 3 and 4, §3 items 4 and 5 (the 2026 lens thread). Decision: camera-support updates should not depend on buying a new version (a licensing question for SCOPE §6); lens correction is optional and overridable, with the full uncropped frame available. Do not force a manufacturer's "mandatory" hint (state a default and let the user turn it off). Roadmap: Phase 2. **New.**
5. **Never require DNG conversion.** Evidence: §3 item 6. Decision: originals stay as they are; DNG is read and written only when the user chooses. SCOPE §9. **New.**
6. **Start from a good default render.** Evidence: §3 item 7. Decision: a per-camera default profile and noise reduction applied on first open (the Capture One behaviour users praise), editable in the sidecar. Tie to 12 §6 item 7 (colour rendering). **New.**
7. **Interop with the tools people already pair with Lightroom.** Evidence: §3 item 8. Decision: hand-off to and from an external denoise or pixel editor, with the result stored as a version of the same image. Phase 3 or later. (Weak evidence; keep as a risk-reducer.)
8. **Match Lightroom words in the UI.** Evidence: §3 item 9, plus 12 §6 item 5. Decision: use "virtual copy", "collection", "smart collection", not new product-specific words.
9. **Denoise: no hidden intermediate file.** Evidence: the 12.3 thread (§2.4 praise item 6). Decision: if Ambrotype ships an AI denoise, it should be a non-destructive recipe step, not a sidecar DNG that is 3 to 4 times the raw. Publish seconds per image per platform as in 12 §6 item 1. Roadmap: Phase 3 or later.

### 5.2 From Part B: preset import (`crates/amb-presets`)

The crate currently has modules `xmp`, `lrtemplate`, `amount`, `masks`, `registry`, `report`, `retouch`, `value` and `xmp_writer`. `Preset::parse` takes text, and `find_embedded_xmp` reads settings out of an embedded XMP packet (that is how DNG presets would be read). SCOPE §7 says all three inputs are implemented. The points below are about the layer around the parser (file handling, UI and report).

1. **Make "my presets do not show up" impossible, or explained.**
   - Accept `.xmp`, `.lrtemplate`, DNG mobile presets, and a `.zip` or whole folder in one drop (12 §6 item 6 already has zip and folders). **New:** list every file and say what happened to it: imported, skipped, or "not a preset" (for example an unzipped `.xmp` that has no `crs:` settings).
   - Show an import summary, not a silent success. Include the preset count per group. This is the opposite of the seller story where the importer "reported success but installed nothing".
2. **No hidden "partially compatible" state.** Every preset appears. Where fidelity is partial, the existing fidelity report (SCOPE §7) says why. Do not hide anything by default. Offer a filter the user turns on, with the opposite polarity to Lightroom. **New.**
3. **Profile dependence by file type.** If a preset uses a raw camera profile and the target is a JPEG or a non-raw file, say so in the report, and apply the rest. **New.**
4. **Separate presets, profiles and brushes.** Keep three import paths and three menus. Recognise local-adjustment presets (brush presets) as their own kind. Mastin Labs' split (Profile menu for JPEG, Preset menu for RAW) shows buyers already think this way. Decide whether Ambrotype imports local-adjustment presets at Phase 2 or later. **New**, open question for SCOPE §12.
5. **Presets are stored once, outside any catalog.** The catalog never owns a preset, so "Store presets with this catalog" cannot happen. Store the original file and the parsed form, and keep a copy of the source download. **New.**
6. **Find Lightroom's preset folders automatically.** Sellers' pages show the folder is shared between Classic and ACR (CameraRaw/Settings). Offer to import from there with one click, and from DVLOP-style installer targets. Ties to 12 §6 item 12(d) (auto-find). Roadmap: Phase 2.
7. **Read old-era packs.** Packs for Lightroom 4 or 5, `.lrtemplate`, and Process Version differences are still being sold and installed (ON1 says old packs may not import cleanly). Keep Process Version dispatch (SCOPE §7) and report the version in the fidelity report. Test with at least one old `.lrtemplate` pack and one DNG-preset pack.
8. **State the platform limits honestly.** Ambrotype has no Adobe sync, so DNG mobile presets are the only way a seller pack can reach a phone without a subscription. Out of scope for the app (SCOPE §2 non-goals; 12 §6 item 10), but do not delete the DNG form on import, so users can still sync the files themselves. Decision for the owner: whether to export a preset back to `.xmp` and DNG (the crate has an XMP writer). **New.**
9. **Amount and "apply to a neutral photo first" are the sellers' own advice.** Both match 12 §6 item 6. Add a one-line hint when the photo already has edits ("this preset will add to your existing edits").
10. **Test corpus.** Use the sellers' named packs only as shapes to test (formats and menus), not as test data. Buyers' packs are copyrighted. Build the test set from presets we write ourselves in each format.

### 5.3 Risks to add to SCOPE §10 (proposal)

- Preset support is judged first by whether import "just works" on the user's own pack. A weak import report makes the whole preset claim look broken.
- Marketing risk: positioning on "no subscription" meets users who also want the cancelled-state behaviour documented.
