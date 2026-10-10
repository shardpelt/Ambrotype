# 13 - What Adobe's own community asks for: Ideas boards and bug reports, and design implications for Ambrotype

Status: design input, 2026-10-10. Feeds [SCOPE.md](../SCOPE.md) §2 (goals and principles), §3 (feature map and competitor table), §7 (preset importer), §8 (phases) and §10 (risks). Complements [12](12-user-feedback-and-design-implications.md), which is built from Reddit. Where this document agrees with 12, that is noted. Where the Adobe boards add something 12 did not have, it is flagged **new**.

---

## 1. Method and caveats

**Method.** The source is Adobe's Community site, four boards:
- Feature requests, Lightroom Classic (board 676), the "Ideas" board for the desktop catalog app.
- Feature requests, Camera Raw and the wider Lightroom family (board 564).
- Bug reports, Lightroom Classic (board 674).
- Bug reports, Camera Raw (board 562).

Pages were saved on 2026-10-10 and split into 18 chunks (`i12ideas-01` to `i12ideas-18`). Extraction facts were written per chunk and then merged here. This document digests the facts files and checked the chunks where a number looked odd. It does not re-read the live site.

**How threads were chosen** (from the source index, items 11 and 12). The new Community site has no sort by votes, and its search could not be read. So threads came from the boards' own **most-viewed** and **most-replied** lists. Only the **first page** of each thread was saved, so late replies, later status changes and long discussions are cut off. List pages were taken to page 3 for most boards. There is no "most voted" list.

**What the numbers mean.**
- On the two feature-request boards, each list row shows a number before the view/reply/age string. It matches the "votes" figure in a second listing for the same idea (for example 94 for the full-raw cloud sync idea in both), so it is read here as the **vote count**. Views and reply counts sit in the same string and could not be separated reliably, so they are not used.
- Vote counts exist only for the Classic board lists that were captured with them (chunks 17 and 18), for a few Camera Raw board rows, and for a few figures quoted inside threads. Most other threads have **no count** ("not collected"). The bug-report listings show **no** vote or reply counts at all.
- Ideas that were merged into others had their votes moved. So a count can sit on the surviving idea, and a merged idea can show a low number.
- "Status" words (Released, In Development, Not Prioritized, Open for Voting, Implemented) are Adobe's tags as shown on the row or quoted by an Adobe employee in the thread. "Released" does not say which version or year unless the thread says so.

**Selection bias.**
- Most-viewed and most-replied lists favour old, famous requests. Some ideas are over ten years old and have collected views for a decade.
- People who post on an Ideas board are Adobe-committed users. Switchers and people who left are visible only through remarks inside threads.
- Bug threads over-represent crashes after an update, since those draw replies. Quiet bugs are under-represented.
- Product versions span roughly Lightroom 3 to Lightroom Classic 15.x. Old bugs are included and are often long fixed. They are used here only as evidence of bug classes, not of current state.
- The facts for bug threads sometimes record one thread twice (a thread can be in both the "views" and "replies" lists). Duplicates were merged.

**Counts are indicative.** "About N threads" below means distinct threads or listing rows in the facts that touch the theme, counted by hand. Treat them as rank order and magnitude.

**Privacy.** No usernames or profile data are used. "An Adobe employee" and "a community expert" are the only roles named.

Sources: Adobe Community boards, as listed in the source sections below; selection method from the source index (items 11 and 12).

---

## 2. Signal summary

### 2.1 Feature-request themes, ranked by signal

"Votes (known)" is the sum of counted votes on ideas in that theme. It leaves out ideas with no count, so it is a floor. Ideas merged into others are not double counted.

| Rank | Theme | Ideas touched | Votes (known) | Adobe status mix |
|---|---|---|---|---|
| 1 | Masking and local adjustments (AI mask upkeep, HSL in masks, more local tools) | about 12 | 584+ | one Released, others open or in development |
| 2 | UI ergonomics and customisation (Done button, shortcuts, fonts, dark mode, scrolling) | about 15 | 536+ | mostly Open for Voting, a few Released |
| 3 | Retouch, creative and preset-workflow tools (heal/clone, layers, grain/halation, crop and relative presets) | about 12 | 190+, plus a related heal/clone idea quoted at about 355 | one In Development, most open |
| 4 | Catalog, DAM and metadata (network catalog, sidecars, notes, stacks, locks, keywords) | about 15 | 285+ | mixed; several Released |
| 5 | Cloud sync and ecosystem | about 7 | 173+, plus a related smart-collections idea quoted at 1.3K | Not Prioritized for smart collections; keyword sync Released |
| 6 | Platform and hardware support (Windows on ARM, Linux, older CPUs, 10-bit) | about 5 | 173+ | ARM In Development, Linux Not Prioritized |
| 7 | AI culling and selection | about 4 | 146+ | culling Released |
| 8 | Denoise/Enhance workflow | about 5 | 142+ | partly addressed by updates |
| 9 | Scopes (RGB parade, vectorscope, histogram) | about 4 | 97+ (parade) and 48 (histogram complaints) | parade/vectorscope In Development |
| 10 | Output: formats, watermark, export (HEIC/HEIF, WebP, metadata watermark, multi-size) | about 12 | 28+ (few counts) | mixed |

Other themes with a real presence but smaller signal: colour profiles, LUTs and lens profiles; camera-format support; video; Book, Print and Slideshow; face recognition.

### 2.2 Bug and stability complaints, ranked by number of threads

No counts are shown on bug lists, so rank is by how many distinct threads or rows fall in each class. Only rank order is meaningful.

| Rank | Bug class | About N threads |
|---|---|---|
| 1 | Crashes, launch failures and freezes, often right after an update | 30 |
| 2 | Catalog, metadata and sort-order faults (capture-time shifts, "needs update" flags, custom order, folder rename, conversion errors) | 25 |
| 3 | GPU- and AI-related faults (denoise, Enhance Details, AI masks resetting or failing) | 25 |
| 4 | Colour, profile and lens-profile faults | 25 |
| 5 | Performance (slow UI, blocked UI, sync lag, memory growth) | 20 |
| 6 | File-format and new-camera support gaps and misreads | 20 |
| 7 | Photoshop and external-editor round trips | 12 |
| 8 | Import, tether and export faults | 15 |
| 9 | Display and UI scaling faults (second monitor, small or high-DPI screens, blank panels) | 12 |

### 2.3 Requests versus bugs

- Feature requests are about **added capability or a fixed annoyance**, and vote counts reach three digits for a few UI and masking items. Bug threads are about **things that used to work**, and no votes are shown.
- The boundary is blurry in practice. Several "ideas" are really regressions: the Done button that left the crop toolbar (190 votes), the Develop histogram redesign (24 and 24 votes), and the Denoise change that blocks the UI (93 votes). People file regressions as ideas when the bug board turns them away.
- The bug board enforces a rule: reports must be about the current release and include steps to reproduce, or they are moved. An Adobe employee on the Camera Raw board said most reports are not bugs. This probably pushes complaints about behaviour into the Ideas boards.

Sources (list page URLs): https://community.adobe.com/feature-requests-676?sort=replyCount, https://community.adobe.com/feature-requests-676/index2.html?sort=views, https://community.adobe.com/feature-requests-676/index3.html?sort=views, https://community.adobe.com/feature-requests-564/index3.html?sort=views, https://community.adobe.com/bug-reports-674?sort=replyCount, https://community.adobe.com/bug-reports-562?sort=views, https://community.adobe.com/bug-reports-562/p-how-do-i-write-a-bug-report-666773

---

## 3. Feature requests: the top requests with votes and Adobe status

Adobe status is as tagged on the list or stated by an Adobe employee. "Several users" means the thread shows more than one voice but no count.

### 3.1 Top requests by vote count (known counts only)

| Votes | Request | Board | Status shown |
|---|---|---|---|
| 303 | Recompute AI subject and sky masks automatically on every selected photo after copy, paste or auto-sync | Classic | Released |
| 190 | Put the Done button for crop and masking back in the toolbar instead of under the histogram | Classic | Open for Voting |
| about 143 | HSL / colour mixer inside local adjustments and masks (idea about 14 years old) | Camera Raw | a related Point Color feature was cited by an Adobe employee as a partial answer |
| 103 | Native Windows on ARM build | Classic | In Development |
| 97 | RGB parade and vectorscope | Classic | In Development; an Adobe employee said the vectorscope reached Camera Raw 18.4 |
| 94 | Sync full-resolution raw files to the cloud, not only smart previews | Classic | no status tag shown |
| 93 | Let Enhance/Denoise run in the background again instead of blocking the whole UI (reported for 14.4) | Classic | an Adobe employee said an April update addressed it; users said the UI still blocked |
| 85 | AI culling by chosen criteria | Classic | Released (announced with the MAX release) |
| 83 | User-assignable keyboard shortcuts | Classic | no status tag shown |
| 60 | Zoom while using the crop tool | Classic | Released (MAX release, after more than 13 years of requests) |
| 47 | Save crop (aspect, straighten) inside presets | Camera Raw | no status tag; first posted about 2011 and static at 47 |
| 46 | Keywords added on iOS reaching Classic and web, and the reverse | Classic | Released (Classic 15.4, off until enabled, not retroactive) |
| 46 | AI masking ignores the "use graphics processor" setting; users with old GPUs cannot turn it off | Classic | no status tag shown |
| 46 | Keep the subject mask when sending to Photoshop | Classic | no status tag shown |
| 42 | Linux version | Classic | Not Prioritized |
| 36 | Finer font-size control for menus and panels on 4K and 5K monitors | Classic | Open for Voting |
| 35 | Allow the catalog on a network drive | Classic | Open for Voting |
| 34 | Keyboard shortcut for crop Auto straighten | Classic | no status tag shown |
| 34 | AI composition and crop suggestion, to apply one hand crop across a batch | Classic | no status tag shown |
| 34 | Separate XMP sidecar for DNG and JPEG files | Classic | no status tag shown |
| 33 | Text notes on photos, stacks, folders and collections | Classic | no status tag shown |
| 31 | Glow, bloom, halation and multi-colour film grain | Classic | In Development |
| 28 | Layers; metadata tokens in watermarks; auto-stacking by the gap between shots | Classic | no status tag shown |
| 27 | Custom paper sizes in Book; automatic image tagging | Classic | no status tag shown |
| 26 | Lasso selection tool (magnetic and polygonal); sorting by more metadata fields | Classic | no status tag shown |
| 25 | Simple liquify or warp tool | Classic | no status tag shown |
| 24 | Histogram complaints (outline hides the data; clipping indicator disagrees); panel scroll bar too near the collapse arrow | Classic | histogram tagged Released; panel bar no tag |

Smaller counts worth noting: dark mode for toolbar and dialogs 23; lock photos against edits and deletion 22; video metadata written into files 22; depth-of-field blur slider in masks 20 (noted as in development); mouse wheel on sliders on macOS 20; relative develop presets 19; left-handed mode 18; custom crop ratios beyond five 18; keyword order for stock use 16; copy/sync settings presets 16; GPU driver warning with "do not show again" 16; 10-bit display chain 14; AVX2 requirement 14; brilliance slider 12; rolling-shutter banding fix 12; AltText field 12.

Large related counts quoted inside threads: a Photoshop-style heal/clone/content-aware idea at about 355 votes sitting "under consideration" for almost 10 years; a related smart-collections sync idea at 1.3K upvotes.

Sources: https://community.adobe.com/feature-requests-676/index2.html?sort=views, https://community.adobe.com/feature-requests-676/index3.html?sort=views, https://community.adobe.com/feature-requests-676?sort=replyCount, https://community.adobe.com/feature-requests-676/p-masking-add-auto-update-recompute-to-subject-sky-masking-after-copy-paste-664827, https://community.adobe.com/feature-requests-676/p-move-done-button-from-beneath-histogram-back-to-toolbar-t-665252, https://community.adobe.com/feature-requests-564/p-hsl-adjustments-for-local-adjustment-tools-666554, https://community.adobe.com/feature-requests-564/p-allow-crop-settings-in-presets-666538, https://community.adobe.com/feature-requests-564/p-more-photoshop-like-clone-healing-content-aware-brushes-666552, https://community.adobe.com/feature-requests-676/p-ability-to-sync-smart-collections-with-ecosystem-clients-665344

### 3.2 What Adobe marked delivered, in progress or declined

The facts show these outcomes. Where a version or time gap is given, it comes from the thread.

**Delivered (Released or Implemented, or an Adobe employee said it shipped):**
- AI mask recompute after paste (303 votes). A June update implemented flip for mask groups as well, though later posters said whole groups still cannot be inverted.
- AI culling (85), tied to the MAX release.
- Zoom while cropping (60), after more than 13 years of requests. Users then reported that crop handles are hard to reach at 100% zoom.
- Keyword sync between Classic and the cloud apps (46), Classic 15.4 in June 2026, opt-in and not retroactive.
- Focus-point display, marked released with the July Camera Raw 18.4.1 update.
- JPG input in the DNG Converter, marked released with Camera Raw 18.4 after about 11 years. A community expert had earlier said it would not happen.
- Feather/expand and auto-recompute for AI masks, updated in Camera Raw 18.3.1 (an Adobe community manager).
- Vectorscope in Camera Raw 18.4, quality tweaks in 18.5.
- Older items: batch Photo Merge and folder/collection colour labels (both Classic 8.4, the labels after about 8 years), custom Book pages and lay-flat paper (7.5), hiding built-in presets (7.4), HEVC import on Windows (stated as in work, then tagged Released), preset hover-preview switch, multi-size export, bulk reset of edits and flags, Live View while tethered (beta in 10.2), face region metadata, 3D LUTs, local tone curve, local noise reduction brush, PSB cataloguing, WebP import/render/sync (export not included).

**In development (as tagged):** Windows on ARM; RGB parade and vectorscope for Lightroom; Live Photo import as still plus movie; vector shapes with variable feathering; anamorphic de-squeeze (Camera Raw 18.3 carried an update; users asked whether Classic gets it too); glow/bloom/halation/grain; depth-of-field blur slider; copy rules for different orientations; Lens Profile Creator 64-bit update.

**Not Prioritized or declined:** Linux; smart-collections sync; metadata applied to every photo in a collapsed stack; HEIC/HEIF export (merged into another idea, votes moved); Sigma Merrill/SD1 raw support; un-maximised PSD visibility. An Adobe employee said transform blank corners are by design, with a generative expand technical preview as early access. Relative presets were "never released" per an Adobe employee. Card-copy-only import was called by design.

**Left hanging (long open, no clear answer in the facts):** heal/clone (about 10 years), HSL in masks (about 14 years), crop in presets (about 15 years), customisable shortcuts (no status), face recognition (many requests, no official answer in that thread), network catalog. A community expert noted that votes alone do not move Adobe.

Sources: https://community.adobe.com/feature-requests-676/p-ability-to-sync-lightroom-classic-keywords-with-the-lightroom-ecosystem-666425, https://community.adobe.com/feature-requests-564/p-display-focus-points-666543, https://community.adobe.com/feature-requests-564/p-dng-converter-enable-jpg-conversion-666504, https://community.adobe.com/feature-requests-564/p-masking-add-controls-to-modify-mask-to-expand-contract-or-feather-in-ai-selections-664842, https://community.adobe.com/feature-requests-676/p-enable-zoom-with-crop-tool-666057, https://community.adobe.com/feature-requests-676/p-ai-culling-665458, https://community.adobe.com/feature-requests-676/p-masking-invert-flip-mask-group-664825, https://community.adobe.com/feature-requests-676/p-rgb-parade-and-vectorscope-in-lightroom-665033, https://community.adobe.com/feature-requests-564/p-anamorphic-lens-support-de-squeeze-666622, https://community.adobe.com/feature-requests-564/p-fill-in-empty-areas-in-transformed-or-cropped-rotated-image-666485, https://community.adobe.com/feature-requests-676/p-relative-develop-presets-666410, https://community.adobe.com/feature-requests-676/p-provide-support-for-linux-666366

---

## 4. Feature-request themes in detail

### 4.1 Masking and local adjustments (rank 1)

- **AI mask upkeep is the loudest single item.** Auto-sync and paste leave the pasted subject or sky mask stale until the user presses recompute photo by photo. This is the 303-vote idea. Related pain: Update AI Settings acts only on the first selected image in some cases; AI masks need a manual update after a catalog moves between machines or an edited catalog comes back from an editor; presets do not carry AI settings, so users apply a preset and then run Update AI, which adds a second history step. A user who batch-updated 50 masks in about 15 seconds called it a big daily saving.
- **Mask hand-off.** Keep the subject mask when sending to Photoshop (46); export masks as alpha channels or layer masks (several users; a photogrammetry user with 750+ photos). A community expert warned that AI masks are not full-pixel and are looser than Photoshop masks.
- **Mask editing gaps:** feather or expand for AI masks; refine edge; invert or flip a whole group; lasso (26); brush shapes beyond a circle; reshapeable gradient lines; a depth-of-field blur slider (20); a target-based "point" selection tool; masks for everything except subject and sky; Colour Grading that respects the active mask.
- **HSL, tone curve, noise and sharpening as local tools.** HSL in masks is about 143 votes and 14 years old. A thread on making every adjustment local says Adobe adds only one or two local settings per year. Local tone curve and local noise brush are tagged Released.
- **GPU control for AI masks (46).** Users with old or unsupported GPUs want to turn GPU off for masking and said a later release broke the workaround.
- **Mask scale limits.** A user reports masks with many brush strokes hit a hidden limit and reset their edits; another saw AI settings reset on reopening, even in a new catalog.

Sources: https://community.adobe.com/feature-requests-676/p-masking-add-auto-update-recompute-to-subject-sky-masking-after-copy-paste-664827, https://community.adobe.com/feature-requests-676/p-masking-make-the-mask-available-in-photoshop-664878, https://community.adobe.com/feature-requests-564/p-make-all-adjustments-local-666540, https://community.adobe.com/feature-requests-564/p-hsl-adjustments-for-local-adjustment-tools-666554, https://community.adobe.com/bug-reports-674/p-ai-masks-require-updating-and-or-disappear-664524, https://community.adobe.com/feature-requests-676/index3.html?sort=views

### 4.2 UI ergonomics and customisation (rank 2)

- **A tiny regression, a huge signal.** Moving the Done button from the toolbar to under the histogram drew 190 votes. One poster said they moved to Capture One after the change. The button reappeared briefly and went away again.
- **Shortcuts.** User-assignable shortcuts (83): override crop and brush keys, a hotkey for a specific brush preset, one for Open as Layers. Shortcuts follow the app language, so on non-US layouts keys such as the bracket keys can go missing. Users lean on plugins and hardware controllers to work around it.
- **Auto-straighten shortcut (34),** batch-friendly for event and sports shooters. Auto straighten can lock to a field's lines rather than the horizon.
- **Scaling and fonts.** Menu and panel text too small on 4K/5K screens (36) with only Small and Large; OS scaling blurs external monitors. The Camera Raw window larger than the screen on small laptops, with buttons off-screen, is also a bug (see §5.5).
- **Dark mode and layout.** Dialogs and toolbars stay white in a dark UI (23); floating and movable panels; all panels on one side for pen-tablet users; a custom canvas colour around the image.
- **Accidental panel collapse (24).** The thin scroll bar sits beside the collapse arrow on high-DPI screens. Reported for about eleven years.
- **Smaller input fixes:** scroll wheel on sliders on macOS (20); Option+scroll zoom broken with the brush and spot tools; crop zoom handles; hysteresis on tone-curve points; faint grid lines; Camera Raw UI revert after a layout change (a user measured about 30% more time per image).
- **Histogram.** The 13.0 redesign hid where the data sits and the clipping indicator disagreed with real clipping; users asked for the old look or a choice.

Sources: https://community.adobe.com/feature-requests-676/p-move-done-button-from-beneath-histogram-back-to-toolbar-t-665252, https://community.adobe.com/feature-requests-676/p-allow-for-keyboard-shortcut-customization-666358, https://community.adobe.com/feature-requests-676/p-font-sizes-in-ui-are-small-and-unreadable-we-need-to-be-able-to-resize-with-greater-granularity-665023, https://community.adobe.com/feature-requests-676/p-lightroom-classic-panel-scroll-bar-close-annoyance-664748, https://community.adobe.com/feature-requests-676/p-please-add-a-keyboard-shortcut-for-automatically-strighten-the-photo-option-665224, https://community.adobe.com/feature-requests-676/p-brighten-the-histogram-as-lrc-13-0-histogram-is-much-darker-than-lrc-12-5-665659, https://community.adobe.com/feature-requests-564/p-revert-ui-to-older-camera-raw-ui-12-2-1-after-interface-changes-666476

### 4.3 Retouch, creative tools and preset workflow (rank 3)

- **Retouch.** A Photoshop-style clone/heal/content-aware brush is wanted by people who say spot removal is the main reason they leave for Photoshop (about 355 votes in a related idea). Simple liquify/warp (25). Fill the blank corners left after transform, rotate or crop (Adobe called it by design; generative expand exists as a technical preview in Camera Raw).
- **Layers.** Layers (28) or at least a Blend command with multiply and screen for Orton and halation looks. A reply argued profiles and local adjustments already act as layers.
- **Film looks.** Glow, bloom, halation and multi-colour grain (31, in development). A request for a library of analog film-emulation presets and profiles says licensing might be the obstacle.
- **Presets as workflow objects.**
  - Crop (aspect, straighten, Constrain Crop) cannot be saved in presets (47 votes, about 15 years old).
  - Relative presets that add an offset instead of setting a value (19). An Adobe employee said they were never released; Quick Develop is the only relative path.
  - Saved checkbox sets for Copy/Sync Settings (16). A wedding photographer unticks about six items every time.
  - Preset install, find, move and delete are painful; backup and restore for presets (alongside catalog backup); a user with 650+ presets finds search too crowded and uses named groups.
  - Preset Amount slider: welcomed as a game changer, but at zero some effect remains, and users asked for it in Classic and on existing presets (a third-party plugin fills the gap).
  - Hover preview of a full-size image freezes large panoramas; a switch was added, and a later hover bug showed previews for only the first profiles on Windows.
  - Presets that use the radial filter and range masks stopped working in mobile apps and Camera Raw after one update (a bug report).
- **Other:** a brilliance slider like the iPhone one (12); a Camera Raw linear profile; Colour Temperature below 2000K for infrared work; invert scanned negatives to positives in colour and black and white (several users; third-party tools and ON1 cited as having it); anamorphic de-squeeze.

Sources: https://community.adobe.com/feature-requests-564/p-more-photoshop-like-clone-healing-content-aware-brushes-666552, https://community.adobe.com/feature-requests-676/p-add-layers-to-lightroom-666312, https://community.adobe.com/feature-requests-564/p-allow-crop-settings-in-presets-666538, https://community.adobe.com/feature-requests-676/p-relative-develop-presets-666410, https://community.adobe.com/feature-requests-676/p-presets-for-copy-paste-sync-settings-664778, https://community.adobe.com/feature-requests-564/p-beta-feature-in-camera-raw-14-0-releasing-at-max-2021-preset-amount-slider-feedback-thread-666600, https://community.adobe.com/feature-requests-676/p-add-the-possibility-to-disable-full-image-preset-preview-666403, https://community.adobe.com/feature-requests-564/p-ability-to-invert-negative-scans-to-positives-color-and-black-and-white-666587

### 4.4 Catalog, DAM and metadata (rank 4)

- **Network-stored catalog (35).** Wanted for NAS use, multi-computer work and machines whose local data is wiped. A community expert said constant writes get corrupted over a network because the database is single-user. Some users report years of use over iSCSI or links without trouble. A multi-computer lock, as in video editors, was suggested.
- **Sidecars.** XMP omits flags, collections, stacks and virtual-copy settings (several users). Users mark rejects with a keyword or label so the decision survives. Wanted: separate XMP for DNG and JPEG (34), and an option never to write into JPEG, TIFF or DNG originals. Sidecar names omit the original extension, so a JPEG and a raw with the same base name share one sidecar.
- **Notes (33)** on photos, stacks, folders and collections, searchable, printable; users now abuse caption fields or dummy images.
- **Locks (22)** against edits, sync and deletion; a pro with 340,000+ images wants a flag that blocks deletion. Hiding sensitive photos behind a flag or password.
- **Stacks and pairs.** Auto-stack by the gap between shots (28), RAW plus JPEG as a pair (18), metadata on every photo in a stack (11), stacks across folders. A "RAW plus JPEG" thread also asks for embedded-JPEG view and a toggle between the two (several users).
- **Keywords.** Keep custom order for stock agencies (16); bulk delete, merge, duplicates (9); only nine keyword-set slots; batch export-flag changes; flat sync. Face recognition and re-indexing (several users); read camera-recorded face data.
- **Sort and find.** Sort by Title, Caption and multiple keys at once (26). AltText field (12).
- **Formats and assets.** Catalogue EPS, AI, PDF, GIF, PNG as DAM assets (19); PSB over size limits (released); video metadata written into files (22).
- **Import safety.** Delete card originals after a verified import (6, "by design" per Adobe); skip the JPEG when a raw exists; "New Photos" filter breaks when switching catalogs; Windows drive letters A: and B: hidden.

Sources: https://community.adobe.com/feature-requests-676/p-allow-catalog-to-be-stored-on-a-networked-drive-666362, https://community.adobe.com/feature-requests-676/p-include-additional-metadata-in-xmp-flags-collections-vc-s-etc-664991, https://community.adobe.com/feature-requests-564/p-add-a-camera-raw-option-to-prevent-writing-back-into-input-files-666532, https://community.adobe.com/feature-requests-676/p-add-text-notes-to-photos-stacks-folders-directories-collections-666360, https://community.adobe.com/feature-requests-676/p-ability-to-lock-photos-to-prevent-further-editing-666399, https://community.adobe.com/feature-requests-676/p-better-keyword-management-666408, https://community.adobe.com/feature-requests-564/p-capability-to-display-embedded-preview-or-sidecar-jpeg-666530, https://community.adobe.com/feature-requests-676/p-delete-images-on-card-after-import-666337

### 4.5 Cloud sync and the Lightroom ecosystem (rank 5)

- Full-resolution raw sync (94): users want to edit from an iPad or another machine without smart previews only. Upload and download are slow, so people route photos through a phone. One user would move to Capture One without a fix; another with about 3 TB rejects cloud-only storage.
- Smart-collections sync (21, with a related 1.3K idea): Not Prioritized. A community expert said Adobe stated Classic would get no further sync improvements once the cloud app launched. Users call sync in Classic buggy.
- Keyword sync (46): Released in 15.4. A flat-sync request remains because the cloud app has no hierarchy.
- Local-network or cable sync (12) to avoid the cloud and roaming data.
- Live Photos arrive as still or movie but not a linked pair (In Development).
- Client commenting without an Adobe account (open for about 11 years). A blog owner noted AI-driven spam, which is the reason some sites demand login.

Sources: https://community.adobe.com/feature-requests-676/p-should-be-able-to-sync-full-raw-files-to-the-cloud-not-just-smart-previews-666424, https://community.adobe.com/feature-requests-676/p-ability-to-sync-smart-collections-with-ecosystem-clients-665344, https://community.adobe.com/feature-requests-676/p-i-would-like-to-synchronize-over-lan-cable-instead-of-internet-665276, https://community.adobe.com/feature-requests-676/p-support-for-importing-live-photos-ios-9-iphone-6s-664746, https://community.adobe.com/feature-requests-676/p-client-commenting-rating-and-liking-collections-without-adobe-id-664947

### 4.6 Platform and hardware (rank 6)

- Windows on ARM native (103, In Development). Users report the emulated build is already faster and lasts longer on battery than on an Intel laptop. Adobe's known-issues list for ARM includes slideshow, video import and Edit in Photoshop.
- Linux (42, Not Prioritized). Posters would subscribe. Dual-boot and VM workarounds exist; GPU in a VM is too slow. Some replies say a port would be a poor use of an Adobe backlog.
- AVX2 requirement from 14.1 locks out older CPUs (14). The Creative Cloud app listed an incompatible version as compatible, and an update then broke the slideshow.
- 10-bit display end to end (14). Retina and Touch Bar are old Released items.
- Hardware churn: users say new camera support pushes them to buy new computers.

Sources: https://community.adobe.com/feature-requests-676/p-native-support-for-wos-copilot-arm-devices-665423, https://community.adobe.com/feature-requests-676/p-provide-support-for-linux-666366, https://community.adobe.com/feature-requests-676/p-allow-cpus-without-avx2-to-install-adobe-products-666075

### 4.7 AI culling and selection (rank 7)

- AI culling by user-chosen criteria (85) was tagged Released. Requests describe what pros want: local processing (no cloud) for thousands of burst frames, choice of strictness, results as flags, ratings or labels, out-of-focus detection (one airshow user has about 50,000 images per trip), grouping bursts, closed-eye or right-eye-sharp checks, a staged technical pass then a subjective pass, genre presets. The first release was seen as tuned to portraits.
- A third-party cull app is used because previews load instantly and faces zoom.
- Related: AI composition and crop for batches (34), automatic tagging (27), focus-point display (delivered in Camera Raw), focus points before import when culling.
- Dissent: one user wanted bug fixes before niche features and tied culling to the price rise.

Sources: https://community.adobe.com/feature-requests-676/p-ai-culling-665458, https://community.adobe.com/feature-requests-564/p-display-focus-points-666543, https://community.adobe.com/feature-requests-676/index2.html?sort=views

### 4.8 Denoise and Enhance workflow (rank 8)

- Enhance/Denoise started blocking the whole UI in 14.4 where earlier versions made DNG files in the background (93). Users stayed on 14.3, and a community expert says AI tools have blocked the UI since AI masks arrived in 2021 and suggests a slower background mode. The 15.3 help page still describes the old dialog.
- Filename suffix "Enhanced-NR" breaks round trips (30); later builds work inside the raw file and no derivative DNG is made.
- Denoise output as DNG is about four times the raw size (TIFF wanted, 2). Export file-number naming gives sequence numbers after denoise (17).
- Super Resolution and Denoise combining is disputed in the thread. Super Resolution on older Sony raws makes artifacts in hair; denoise first helped.
- Wanted: a choice of denoise method.

Sources: https://community.adobe.com/feature-requests-676/p-bring-back-the-ability-of-enhance-to-run-in-the-background-lrc-666179, https://community.adobe.com/feature-requests-676/p-a-way-to-alter-the-naming-for-enhanced-filename-suffixes-665461

### 4.9 Scopes (rank 9)

- RGB parade and vectorscope (97, In Development). Workarounds are paid overlay apps, region scopes and opening the file in Photoshop as a Camera Raw smart object. The vectorscope exists in Camera Raw 18.4 (right-click the histogram).
- Histogram complaints (24 and 24) and a request for histogram gridlines (an Adobe employee said a December update implemented them, but users said they were still missing).
- Colour-blind aids in the HSL panel (colour being picked, saturation warning); an interactive colour wheel in HSL.

Sources: https://community.adobe.com/feature-requests-676/p-rgb-parade-and-vectorscope-in-lightroom-665033, https://community.adobe.com/feature-requests-676/p-brighten-the-histogram-as-lrc-13-0-histogram-is-much-darker-than-lrc-12-5-665659

### 4.10 Output: formats, watermark, export (rank 10)

- HEIC/HEIF export (several users; Not Prioritized, votes merged). Users say Apple's format is the norm and a lossless detour loses quality; some would look at other apps. HIF from Canon R5 and HEVC video import also asked. Preferred container debates (HIF vs AVIF, JPEG XL unusable on one iOS beta).
- WebP export (import, render and sync shipped; export not); PNG export added; Camera Raw opens every WebP file with no toggle.
- Metadata-driven watermark (28) with date, lens, camera, folder; third-party plugins are Windows-only or abandoned; one long-time user said they may leave over it. Watermark darker in export than in the editor; watermark failure on export when none is used.
- Multi-size export in one pass (4; Released). Parallel export and CPU use: older threads on export using about half the CPU.
- Export raw with edits applied (a Camera Raw request), and export mask data.
- Camera Raw watermark option, batch without Lightroom.

Sources: https://community.adobe.com/feature-requests-564/p-ability-to-export-files-in-the-heic-heif-formats-666659, https://community.adobe.com/feature-requests-564/p-camera-raw-lightroom-classic-lightroom-ecosystem-support-for-webp-666566, https://community.adobe.com/feature-requests-676/p-smart-watermark-to-include-data-location-name-date-lens-camera-exif-metadata-etc-666200, https://community.adobe.com/feature-requests-676/p-export-images-at-multiple-resolutions-in-one-go-666306, https://community.adobe.com/feature-requests-564/p-add-watermark-on-camera-raw-666575

### 4.11 Colour, profile, lens and camera-support requests (below the top 10)

- **Disable the built-in lens profile.** Some cameras (Micro Four Thirds, compacts, some Nikon Z and Panasonic) ship profiles that cannot be turned off. Capture One has a strength slider. One user says this is the deal-breaker for returning. A community expert said the off switch exists only for cameras added after a certain point. A blank "zero" profile was suggested.
- **Profiles and LUTs.** Camera Matching for Canon CR3 (marked Released), linear camera calibration profile, Rec 709 in the LUT colour-space list (with gamma 2.2 and 2.4 variants), documentation of how a LUT is stored in an enhanced profile, and moving the Camera Calibrator chart into Camera Raw. A reply in the Rec 709 thread said Rec 709 has no simple gamma; another clarified that the internal working space is linear and a 2.2-curve space only drives displayed values. The thread did not describe the layout of the Look/RGBTable/LookTable data.
- **Lens profiles** wanted for Olympus Zuiko Digital (Four Thirds), several Canon models and tilt-shift lenses. Self-made profiles were requested for Adobe's downloader, which an Adobe employee's reply and the Leica thread say is no longer supported.
- **Camera support:** Sigma Merrill/SD1 (Not Prioritized, years of requests), Fuji EXR, Phase One IQ4 150 MP not opening months after launch (the user moved to Capture One), Hasselblad crop-mode in MakerNote, OM-1 II high-res files.
- **Display of data:** more EXIF, and the focus point recorded by the camera.

Sources: https://community.adobe.com/feature-requests-564/p-disable-built-in-lens-profile-666496, https://community.adobe.com/feature-requests-564/p-support-for-canon-cr3-camera-matching-profiles-666479, https://community.adobe.com/feature-requests-564/p-add-rec-709-to-lut-color-spaces-in-new-enhanced-profiles-666492, https://community.adobe.com/feature-requests-564/p-support-lens-profiles-for-olympus-zuiko-digital-lenses-e-system-666521, https://community.adobe.com/feature-requests-564/p-sigma-sd1-when-will-the-merrill-sensors-be-supported-please-666507

---

## 5. Bug and stability complaints, by class

No vote or reply counts are shown on the bug boards, so these are ordered by number of threads. Versions are as stated in each thread.

### 5.1 Crashes, launch failures and freezes (rank 1)
- After updates, the app will not start or quits at launch: 11.3 (a crash module on Windows), 12.2.1 (starts as a background process with high RAM and CPU but no window), 13.0.1 on macOS, and the latest update on Macs generally. Reinstalling, cleaner tools and preference resets often failed. Rolling back to the previous release was the common workaround. A moderator said fixes ship in the next update and may take up to 24 hours to appear.
- A leftover lock file beside a catalog after a crash blocks reopening; deleting only the lock file is the advice.
- A crash during a large pre-export edit left one Windows user without catalog access on a paid job.
- Version conversion traps: after a conversion, rolling back is blocked by a "too new" error; a catalog kept an old name with a suffix because the upgrade ran twice.
- M1 Max crashes while the same version runs on an M1 Pro.

### 5.2 Catalog, metadata and ordering (rank 2)
- Capture time moves by hours after a rating, edit or sync in 14.1 (several users); a staff member said 14.1.1 fixes time-zone handling. An older bug truncates fractional seconds, which scrambles bursts. Sorting by file name was the workaround.
- Photos flagged as needing a metadata update or republish with no change, for years (reported over four years before and acknowledged nearly three years before); a user said it made all backups look out of date.
- Custom sort order breaks after an external-editor round trip, an accidental thumbnail move, or about 52 reorderings.
- Renaming a folder in the app breaks the links to its photos.
- Collection sync stalls with no error, thousands of items queued; users asked for a way to reset the sync database. Adding to a target collection takes minutes with sync on.
- Catalog conversion fails for 112,000 photos; "unexpected error" on open; folder panel stops at about 1,600 folders in a 450,000-photo catalog; 400,000-photo catalog crashes when auto XMP write is turned on.
- Import from a device appends "-2" duplicates even with the duplicate check on.

### 5.3 GPU and AI faults (rank 3)
- Denoise does nothing or breaks on certain files and GPUs: Pentax DNG for about two years (working again after a February update), Ricoh DNG with in-camera noise reduction baked in (an Adobe employee said the maker would have to change this), Intel Arc GPUs (dark or sepia output, 2 to 55 minutes per image), AMD (squares in the output), Fuji X-T2/X-H1 and Sony and Canon files with purple lines on top and right edges.
- Enhance Details: all-black output on Windows with CR2; coloured squares on macOS Mojave until an OS update; AMD GPU not used on macOS.
- AI masks: Select People "entire person" selects the frame; masks reset with a red dot on reopen; old subject masks show empty after conversion; "some masking models are downloading" error blocks preset paste, and one user needed a full uninstall and registry clean.
- Generative Remove changes pixels outside the painted area (14.5). Smart-object heal spots converted to Generative Remove in an early-access build.
- Camera Raw 18.3 froze and saves took about twenty minutes until GPU was turned off.

### 5.4 Colour, profiles and lens profiles (rank 4)
- Canon R5 raws rendered 1.5 to 2 stops dark in one version and correct in the next, which then over-brightened hand-edited images. Panasonic S1R II 12-bit files about two stops dark.
- Sony files taking a green-yellow cast after an update (whole catalog); smart previews turning green when the drive is disconnected; Sony A1 II colour errors confirmed by Adobe.
- Nikon Z6/Z7 magenta cast on white balance changes and wrong lens identification; lens profiles not applied to adapted lenses even though the panel says so.
- Calibrated displays: shadows from about 20 to 0 clipped to black on macOS while the histogram shows none; colour shifts when zooming in Library versus Develop and fit view; Print and Book full-screen colours differ from output.
- HEIC panoramas from iPhone and Samsung shift colour right after import, inconsistently.
- Lens profile v3/v2 problems for Canon 24-240, RF 16 mm, R50; Sigma over/under-correction; phones whose EXIF focal length is a hair off the profile's.
- Pixel and DJI DNGs lose Adobe profiles after a phone or drone firmware change.

### 5.5 Performance (rank 5)
- UI slows with editing time, especially clone/brush after several minutes on Xeon-class CPUs; grid scrolling slow with a custom display profile while editing stays fast.
- RAM grows to about 17 GB when importing 100 images on Windows (13.5); cloud-sync process grows until the system runs out of memory on M1 Macs; a 540,000-photo catalog slows and destabilises.
- Library preview not updating after a Develop edit until restart.
- Grain looks stronger in Library and in exports than in Develop; smart previews look grainier in 10.0 than 9.4 for the same preset.
- Camera Raw: the window too large on small screens, buttons off-screen; a dialog cannot be closed at 150% Windows scaling (Escape discards edits); a known bug according to an Adobe employee.

### 5.6 Format and new-camera support (rank 6)
- Fuji X-E2 DNGs unreadable after one update (fixed in the next, Bridge and Camera Raw first); lossy-compressed Panasonic DNGs unreadable; Leica DNGs edited by an older app reported damaged though other tools read them; Hasselblad FFF unsupported; Samsung Galaxy S7 DNG strip; Pixel Expert Raw import failure; some Fuji HEIF files intermittently unreadable.
- Video: HEVC on Windows, clips not playing or importing after migration, errors in 15.5 on Windows for files VLC plays.

### 5.7 Photoshop and external editors (rank 7)
- Open as Layers puts only one or two of five selected images in the file and leaves a blank layer.
- Edit In opens a Photoshop beta; Edit/Image/Layer menus greyed after the trip from Lightroom; the colour profile is dropped on a raw round trip; Photoshop jumps to the front each time.
- Photoshop single-letter accelerator keys removed in one release (unintended, a bug logged).

### 5.8 Import, tether and export (rank 8)
- Tether loses the camera on power-off or cable pull and may need a restart; Sony A7 IV, Nikon Z8 firmware 2.0, Canon after a macOS update. A photographer said the maker's own tether tool recovered images that the app lost.
- Export: "watermarking failed" when no watermark is used; skips a fifth of files; network-share "folder not writable"; exports with a brush mask overlapping red-eye fail with GPU export on; resize-to-short-edge ignored for DNG with Don't enlarge ticked.

### 5.9 Display and layout (rank 9)
- Second-monitor Compare view black or wrongly zoomed (13.0/13.0.1); panels and filmstrip black after switching to Develop; full-screen Camera Raw mostly blank until a tool key is pressed; Mask panel does not open in full screen on an M-series MacBook; Mask panel text clipped in French, German and Spanish.

Sources (list page URLs): https://community.adobe.com/bug-reports-674?sort=replyCount, https://community.adobe.com/bug-reports-674/index2.html?sort=replyCount, https://community.adobe.com/bug-reports-674/index3.html?sort=views, https://community.adobe.com/bug-reports-562?sort=replyCount, https://community.adobe.com/bug-reports-562/index2.html?sort=views, https://community.adobe.com/bug-reports-562/p-denoise-on-dng-from-ricoh-gr-iii-x-is-missing-luminance-denoise-666876, https://community.adobe.com/bug-reports-562/p-canon-r5-raw-images-are-underexposed-666743, https://community.adobe.com/bug-reports-674/p-edit-sync-photo-changes-the-capture-time-664376, https://community.adobe.com/bug-reports-674/p-photos-are-marked-for-republish-even-though-there-is-no-change-663802, https://community.adobe.com/bug-reports-674/p-can-t-launch-the-most-recent-update-of-lightroom-classic-664027

### 5.10 Switching signals inside threads
A recurring "I am leaving" remark appears in about 20 threads, almost always naming Capture One, sometimes DxO, ON1, ACDSee or Affinity. The triggers named: lens profile that cannot be disabled, per-image waits of minutes in Develop, no full-raw cloud sync, no HEVC, the Done-button change, the Phase One IQ4 and Sigma support gaps, subscription fees after a break, and an unfixed publish-flag bug. This matches 12's finding that people leave for performance and price, and adds that many leave over one small, specific gap.

Sources: https://community.adobe.com/feature-requests-564/p-disable-built-in-lens-profile-666496, https://community.adobe.com/bug-reports-674/p-unable-to-advance-to-the-next-photo-664118, https://community.adobe.com/feature-requests-676/p-should-be-able-to-sync-full-raw-files-to-the-cloud-not-just-smart-previews-666424, https://community.adobe.com/feature-requests-676/p-move-done-button-from-beneath-histogram-back-to-toolbar-t-665252

---

## 6. How this agrees with, and adds to, doc 12

| Topic | Doc 12 (Reddit) | Adobe boards |
|---|---|---|
| Performance and stability are the top bug class | Top complaint, about 160 mentions; Windows worst | Agrees: crash/launch, freezes and slowdown dominate the bug lists. **New:** denoise and other AI jobs block the UI (93 votes); RAM growth on import; GPU-specific faults; a hard cap on mask strokes; large catalogs (450k to 540k) degrade |
| Catalog fragility, sidecars as truth | Corruption about 18 mentions; XMP sidecars should be source of truth | Agrees. **New:** XMP omits flags, collections, stacks and virtual copies (rejects survive only via keyword or label tricks); sidecar name collisions; originals rewritten; capture-time drift on rating; "needs republish" with no change; custom order breaks |
| Linux | Linux and FOSS section | Agrees: Linux idea has 42 votes, Not Prioritized |
| Presets: Amount slider | Valued | Agrees. **New:** at zero some effect remains; Amount missing from Classic and existing presets in the beta; relative presets (19); copy/sync checkbox sets (16); crop in presets (47); preset backup/restore; AI settings not carried by presets |
| Masking | Top love; wants lasso, reorder, feather | Agrees. **New:** recompute after paste/sync is the single largest vote (303); mask hand-off and export as alpha (46); flip group; HSL/tone curve local (about 143); GPU opt-out for AI masks (46) |
| AI denoise | The feature that sold Lightroom | Agrees. **New:** failures on specific cameras (Pentax, Ricoh, Fuji) and GPUs (Intel Arc, AMD) are major bug threads; file-size and naming side effects |
| Culling | Fast culling is an on-ramp | Agrees. **New:** AI culling with local-only processing and staged technical/subjective passes (85); focus-point overlay while culling |
| Mobile and sync | Loved by few, distrusted | Partly agrees. **New:** full-raw cloud sync (94) and LAN/cable sync (12) are real demand; many want sync without the cloud |
| Camera support delays | Profile delays brick presets | Agrees. **New:** phone and drone firmware changes break profiles; lens profile cannot be disabled; Phase One, Sigma and Pixel gaps |
| Subscription and price | About 99 mentions | Thin here (a few remarks). Adobe boards are product-focused |
| UI churn | About 13 mentions | Agrees and **new**: a single moved button drew 190 votes; layout choices in Camera Raw cannot be undone; histogram redesign |
| Keyboard shortcuts | Not covered | **New**: user-assignable shortcuts (83) and language-dependent keys |
| HiDPI, scaling, dark mode | Not covered | **New**: fonts too small on 4K/5K (36), white dialogs in dark mode (23), Camera Raw window larger than the screen |
| Windows on ARM | Windows treated as one platform | **New**: ARM native build (103) |
| Older CPUs | Not covered | **New**: AVX2 requirement locks out 2013-era CPUs (14) |
| Scopes | Not covered | **New**: parade and vectorscope (97) |
| Network/NAS catalog | Not covered | **New**: 35 votes; expert warns of corruption on a network |
| Notes, locks, hidden photos | Not covered | **New**: notes (33), locks (22), hide-flag |
| Lens profile off-switch | Not covered | **New**: built-in profiles that cannot be turned off |
| Film negative conversion | Mentioned via competitors | Adds a signal: invert scanned negatives (several users, old request); SCOPE §3 already adopts negative conversion |
| Layers | Not covered | Layers (28); SCOPE says not planned |

Sources: [12](12-user-feedback-and-design-implications.md) for the left column; right column from the Adobe Community pages cited in §3 to §5.

---

## 7. Design implications for Ambrotype (prioritised)

Targets are proposals, not measured facts. "SCOPE" refers to [SCOPE.md](../SCOPE.md) section numbers.

### 1. Mask state must never go stale (rank 1 signal)
- **Evidence:** the 303-vote recompute idea; Update AI after paste or catalog move; AI settings missing from presets; masks that reset or show empty after conversion.
- **Decision:** AI masks are stored with the image data they were computed from and recompute lazily and automatically whenever pasted, synced or moved to another machine. Presets can carry AI mask references. A mask group has one-click flip, feather and refine from the first release. Mask caps are published, with no hidden limit.
- **Roadmap:** SCOPE §8 Phase 3 masks. Auto-recompute on paste is **new**; adds to 12 §8 (polygon lasso, reorder).

### 2. Background by default for every heavy job (rank 8 and bug rank 5)
- **Evidence:** the 93-vote Denoise complaint; AI tools blocking the UI since 2021 per a community expert; RAM growth on import.
- **Decision:** denoise, previews, export and sync-like work run in a bounded queue and never block editing of other photos. Output is non-destructive (no derivative DNG by default); if a file is made, the name is a user template. Memory is bounded and leak-tested on import.
- **Roadmap:** extends 12 §6 item 1 (budgets). Adds an "other photos stay editable during denoise" test to the Phase 2 and 3 gates (**new**).

### 3. Do not move controls without a reason; make layout a user setting (rank 2)
- **Evidence:** 190 votes for the Done button; shortcuts (83); fonts (36); dark mode (23).
- **Decision:** every shortcut is remappable and stored as a plain file; menus and panels have independent text size; dark mode covers dialogs; panels can be locked against collapse; layouts can be left- or right-handed. Never rename or relocate a control in a release without a switch to keep the old place.
- **Roadmap:** SCOPE §8 Phase 2 "customisation and sharing" (layout files, themes) already covers layout. Remappable shortcuts, language-independent key bindings and per-element text size are **new** as explicit requirements. Fits 12 §7 anti-goal 6.

### 4. Sidecar completeness (rank 4)
- **Evidence:** XMP omits flags, collections, stacks and virtual copies; a sidecar shared by two files with the same base name; originals rewritten; capture-time drift on rating; 34 votes for DNG/JPEG sidecars.
- **Decision:** one sidecar per file, named so it cannot collide, with an Ambrotype namespace that stores flags, labels, stacks, virtual-copy recipes and collection membership. Never write into originals; never change capture time on a rating or edit. Rating and edit never alter the sort key.
- **Roadmap:** SCOPE §8 Phase 0 data-safety ADR (already planned). Stacks, flags and copies in the sidecar are **new**; fixes the "needs republish" class.

### 5. Optional network and multi-machine catalogs (rank 4)
- **Evidence:** 35 votes; experts warn single-user SQLite on a network corrupts; workarounds use block storage.
- **Decision:** the catalog is a rebuildable index on a local disk by default; sidecars are the shared truth, so NAS and sync tools work. An optional lock file prevents two machines opening one catalog.
- **Roadmap:** SCOPE §2 non-goals (interoperate, not sync) plus 12 §6 item 10. A "second machine opens and rebuilds from sidecars" recipe is **new** (Phase 2 docs).

### 6. Presets as workflow objects, beyond the importer (rank 3)
- **Evidence:** crop in presets (47); relative presets (19); copy/sync checkbox sets (16); Amount at zero still applying; preset backup/restore; hover preview freezing large files.
- **Decision:** presets can include crop and straighten as an opt-in group. Relative (offset) presets are a supported type. Amount at zero means no effect, tested. Apply-time groups can be saved as named sets. Presets and profiles live as plain files, so backup is a folder copy. Hover preview is cheap and cancellable.
- **Roadmap:** SCOPE §7 and §8 Phase 2 presets panel; extends 12 §6 item 6. Relative presets, crop-in-preset and "Amount 0 equals no change" are **new**.

### 7. Lens-profile and camera-profile control (bug rank 4, request below top 10)
- **Evidence:** built-in lens profiles that cannot be disabled; adapted lenses wrongly identified; phone and drone firmware breaking matches; wrong colour after a rendering change.
- **Decision:** lens correction strength 0 to 100 percent for every lens and body, with a clear off switch. Profile matching falls back by lens and body ID, not by an exact model string. A visible banner when a profile is missing. A rendering never changes silently between versions (recipes versioned).
- **Roadmap:** SCOPE §8 Phase 1 camera-support process and Phase 3 lens corrections. The strength slider and fuzzy match are **new**; reinforces 12 §6 item 7.

### 8. Culling: local, staged, with focus overlay (rank 7)
- **Evidence:** AI culling (85); local-processing demand; burst grouping; focus points before import; third-party cull tools win on instant previews.
- **Decision:** culling mode (SCOPE §8 Phase 2) shows the embedded preview, a focus-point overlay and zoom-to-focus, groups bursts, and optionally runs a local technical pass (blur, closed eyes, clipping) that only suggests flags. No cloud. A later subjective pass is a Phase 4 option.
- **Roadmap:** SCOPE §8 Phase 2 culling mode; focus-point overlay and suggest-only technical pass are **new**; extends 12 §6 item 4.

### 9. Platform commitments (rank 6)
- **Evidence:** Windows on ARM (103); Linux (42); AVX2 lock-out (14); 10-bit display (14).
- **Decision:** Windows ARM64 native is already a SCOPE §2 goal and this evidence supports it. Publish a minimum CPU instruction level (ideally with a scalar fallback so older machines still work), and a 10-bit display path where the OS allows.
- **Roadmap:** SCOPE §2 goals and §8 Phase 2 packaging. A CPU-baseline statement and a scalar fallback are **new**.

### 10. Scopes and display tools (rank 9)
- **Evidence:** parade and vectorscope (97); histogram redesign complaints (24 and 24); colour-blind aids.
- **Decision:** the histogram has a choice of views (filled, line, gridlines, per-channel), a clipping overlay that matches what the user sees, and RGB parade plus vectorscope. Colour-picker feedback in HSL.
- **Roadmap:** SCOPE §8 Phase 2 histogram and clipping; parade and vectorscope are **new** (Phase 2 or 3).

### 11. Metadata-aware watermark and output (rank 10)
- **Evidence:** metadata tokens in watermarks (28); multi-size export; HEIC/HEIF, WebP and JPEG XL output; export raw with edits.
- **Decision:** export presets have a token system (date, lens, camera, folder, file name) for naming and watermark text, and one pass can write several sizes into separate subfolders. HEIF/AVIF/JPEG XL/WebP are output options. What you see in the watermark editor is what you get.
- **Roadmap:** SCOPE §8 Phase 2 export (AVIF and JXL already listed). Tokens in watermarks, multi-size runs and HEIF/WebP are **new**.

### 12. Cataloging details users miss
- **Evidence:** notes (33); locks (22); hide flag; auto-stack by gap (28) and raw-plus-JPEG pairing (18); keyword tools; sort by more fields (26).
- **Decision:** include these as first-version catalog features rather than plugins: free-text notes on photos and collections; a lock flag that blocks edit and delete; stacks that use the gap between shots and pair a raw with its JPEG; keyword merge/bulk edit; multi-key sort.
- **Roadmap:** SCOPE §8 Phase 3 catalog (keywords, collections, stacks, virtual copies). Notes, locks, pairing and bulk keyword tools are **new**.

### Out of scope or low priority from this evidence
- Layers (28): SCOPE §3 says not planned; mask groups and the Blend idea (multiply/screen) cover most reasons.
- Book, Print, Slideshow, map and face recognition: SCOPE §2 and §8 Phase 4 already place them later or out. Volume here is low.
- Video (HEVC import, metadata): requested often, but not a core stills-development task. Treat as later; do not promise.
- Client commenting, cloud sharing: out (SCOPE §2 non-goals).
- Anamorphic de-squeeze and film-negative invert: niche, but invert is already adopted in SCOPE §3; add de-squeeze as a cheap crop-and-scale option if requested.

### Anti-goals added to 12 §7
- No hidden limits (mask strokes, folder list, keyword sets).
- No layout change without an escape hatch.
- No feature that works only after a manual "update" button on data the app already has.
- No firmware- or model-string-dependent profile matching that silently fails.

Sources: derived from §3 to §5 above; SCOPE.md §2, §3, §7, §8, §10; doc 12 §6 and §7.

---

## 8. Proposed changes to SCOPE.md (proposals only; not applied)

- **§2 principles:** add a sixth: "No stale or hidden state" (masks recompute, no hidden caps, no silent rendering change).
- **§7 importer / presets:** add crop-in-preset (opt-in group), relative presets, saved apply-time groups, and an Amount-0 equals no-change test.
- **§8 Phase 0:** add the sidecar-completeness ADR (flags, stacks, copies, collection membership, collision-free names) and a CPU-baseline statement.
- **§8 Phase 1:** add lens-correction strength and fuzzy lens/body matching to the camera-support process.
- **§8 Phase 2:** add remappable shortcuts, per-element text size, dark-mode dialogs, a lock against panel collapse, RGB parade and vectorscope, focus-point overlay, a metadata-token system for export names and watermarks, and multi-size export.
- **§8 Phase 3:** add AI mask auto-recompute, mask flip/feather/refine, notes, locks, gap-based stacking and raw-plus-JPEG pairing, bulk keyword tools, and multi-key sort.
- **§10 Risks:** add "Users leave over one small gap" (Medium; mitigation: a visible public gap list and a quick-fix lane for small items such as profiles or shortcuts).
- **§13 Document index:** add this document.
- **Personas:** add a "wedding/event shooter with batch paste and sync" check on the mask-recompute flow.

Sources: derived from §7 above.
