# 12 - What Reddit says: user feedback and design implications for Ambrotype

Status: design input, 2026-10-03. Feeds [SCOPE.md](../SCOPE.md) §2, §7, §8, §10. Complements the commercial survey in [09](09-market-commercial-alternatives.md), especially its sections (b) why users leave and return, (c) migration paths, (d) the market gap and (e) design lessons. Where this document and 09 agree, that is noted. Where Reddit adds something 09 lacked, it is flagged **new**.

---

## 1. Method and caveats

**Method.** Threads were pulled from the Arctic Shift Reddit archive (2023-2026; r/photography, r/postprocessing, r/Lightroom, r/AskPhotography, r/captureone, r/DarkTable, r/linux and a few others). The 660 top threads (by score and comment count, about 22,000 comments) were split into 15 chunks. A Haiku-class model wrote one extraction report per chunk: `lightroom_0..5` (loves and complaints), `alternatives_0..4` (competitors), `presets_0..3` (preset use). This document synthesises those 15 reports. It did not re-read the raw threads.

**Counts are indicative, not statistical.**
- Each report counted "posts", "posts/comments" or "mentions" in its own way. Counts in §2-§4 are sums of those figures across reports, rounded, and the units are mixed. Read them as rank order and rough magnitude.
- Each report says it read 10-45 threads, so deep coverage is well below 660.
- Selection bias:
  - Top-by-score favours dramatic threads (price hikes, corruption, "I quit Adobe").
  - Unhappy users over-post. So do FOSS and Linux users in the alternatives threads.
  - Professionals and preset buyers are under-represented. r/photography is culturally anti-preset: one preset report found about 15 anti-preset vs 5 conditional-pro opinions.
  - The window includes Adobe's 2025 price rise and Affinity going free (Oct 2025), which inflate both topics.

**Extraction-quality flags.**
- `lightroom_3` was contaminated by off-topic phone-camera threads. Its counts are discounted, and only its preset and alternatives lists are used.
- `alternatives_1` and part of `presets_0` kept only chunk-local comment indices, not post ids. Those are cited as `[alt1 c#NNN]`.
- Some reports attach one post id to many claims, and some quotes look paraphrased. Quotes below are as reported, at most 25 words, and attributed by post id. They were not re-verified against Reddit.
- `presets_2` marks several wishes as "implied". Those are treated as weak evidence and labelled.

---

## 2. Lightroom: what users love and what they complain about

Mention totals are summed across the six `lightroom_*` reports (with `alternatives_*` where noted).

### 2.1 Loved (ranked by mentions)

1. **Masking, especially AI masks** (about 92 mentions, 6/6 reports). Subject, sky and background masks are the most-praised feature. Several users call masking the reason they have not left.
   - "AI background masking has been a godsend, 99% of images get an AI-generated background mask applied" [1rly2pc]
   - One user says they cannot switch because "Adobe's masking is the best" (`lightroom_2`).
2. **AI tools: denoise first, then remove and generative** (about 78 mentions combined; denoise alone about 46). Denoise is the one AI feature people say sold them on Lightroom. The wedding use case recurs.
   - "AI denoise is magic" [1mteppe]
   - "I can shoot indoor/night sports at ISO 20000, denoise it, and it looks like broad daylight" [19f7t8m]
   - "AI selection [which] fixes all portraits from a 700-photo wedding with one click" [1gd742z]
3. **Colour grading, HSL, curves and tone tools** (about 53). They are praised as a combined toolkit, not as single sliders.
4. **Mobile editing and cloud sync** (about 49). This is loved by a vocal minority. `lightroom_2` concludes "mobile is secondary" for most desktop users, and sync has almost as many complaints (§2.2).
   - "20 years of photos instantly on my phone" [alt1 c#2270]
5. **Workflow ease and speed** (about 47, noisy). Lightroom's learning curve is described as the gentlest in the industry. Culling speed and "arrow through photos, keep/reject, move on" also count.
   - "Lightroom can be used with no context, no tutorial. It just works" [1vz98el]
6. **Presets and the preset ecosystem** (about 33). Presets are loved as a fast base edit that users then tune.
   - "I literally couldn't do my job if I didn't have presets" [1ow831y]
7. **Catalog and DAM: collections, keywords, non-destructive editing** (about 28 mentions). This ranks low by volume but first as a retention reason, because it is why users cannot leave (see §3).
   - "No other editing software has DAM like LR" [1j5zii5]
   - "Nothing beats Lightroom's library organization... culling, picking, color labels work seamlessly" [1gd742z]

### 2.2 Recurring complaints (ranked)

1. **Performance, lag, crashes** (about 160 mentions across all six reports; the clear number one). Sub-themes:
   - Windows is far worse than Mac (about 50 mentions). Users buy Macs to cope.
   - Slowdown at catalog scale.
   - Lag with more than about 4 masks.
   - AI denoise taking minutes.
   - Export not using the GPU.
   - "Cold molasses on latest hardware" [1wt9nsw]
   - "After 4 masks...it completely starts to bug out and becomes outright unusable" [1kbguc3]
   - "copy/paste settings takes 5+ min per photo" [1rhh2k7]
   - "I have 250k photos and struggle at 60k; it's crippled" [1rn6wme]
2. **Subscription cost and price increases** (about 99). The 2025 rise from $9.99 to $14.99 a month is cited constantly. Cancellation friction and fees are cited too.
   - "50% jump overnight" [1mteppe]
   - "Adobe subscription jumped from $9.99 to $14.99 per month...can't justify higher cost" [1j5zii5]
3. **Catalog fragility, corruption and limits** (about 49; corruption and data loss specifically about 18).
   - "Corrupted beyond repair—lost 10+ years of presets and edits" [1jkngmu]
   - "Catalog system is a relic from 2007" [1rn6wme]
4. **Cloud and sync unreliability, privacy** (about 37). Mobile and desktop disagree, there are restart requirements, and users do not trust the cloud.
   - "Another windows user here - please fix the sync issue with Classic" [1oiileq]
5. **Forced AI, generative credits, "double dipping"** (about 29).
   - "Nobody asked for this...they're shoving AI into raw file processing software" [1wt9nsw]
   - Users also say AI work competes with performance fixes for developer time.
6. **Perpetual-licence betrayal and lock-in** (about 8 plus echoes in alternatives). The Lightroom 6 activation shutdown is a trust event.
   - "they bricked Lightroom 6...lost years of work and metadata" [1pfcqlx]
7. **Classic vs Cloud vs Desktop confusion and feature gaps** (about 17). `lightroom_2` has users saying "every time I try non-Classic I'm wondering why basic Classic features aren't there".
8. **UI churn and workflow-breaking updates** (about 13).
9. **Mask tool gaps** (about 5, but specific). Users want a polygon lasso and drag-to-reorder masks.
10. **Camera profile delays** (about 9). Sony A7V support "long overdue" [1smrjth], and "no ETA...extremely irritating" [1woo33c].
11. **Mobile and export limits** (about 12, plus about 16 on a 2026 Android preview bug [1qvqaiq]). Examples are iPad exports at about 2 MP [1uxs003] and no HDR merge on iPad.

---

## 3. Per-competitor summary

| Product | Loved for | Hated for | Why people leave it / return to Lightroom |
|---|---|---|---|
| **darktable** | Free; masking ("beyond what anything else offers" [alt1 c#945]); Filmic/Sigmoid/AgX tone mappers; results. Praise count about 100 across reports | UX: "probably in the worst 3 apps I have used of all time" [1vz98el]; module sprawl and deprecated modules; weak DAM; batch edit needs many more clicks than LR's 3-4 [1omx2jh]; calibration needs CLI/XML [1uucnxa]; old edits broke after updates [1axg02l] | Many quit within an hour to two weeks; those who persist (2-12 weeks, YouTube tutorials) rarely return. Returners cite speed and muscle memory: "I hate library management and editing with a passion...I just pay the adobe tax" [1v847s4] |
| **RawTherapee / ART** | "The mad scientist's raw editor" [1v847s4]; more Lightroom-like than darktable; ART adds CTL scripts and simpler UX | "Very confusing" UI; small community; weak DAM; beginners give up | Seen as a stepping stone; returners cite polish |
| **RapidRAW** | "Clean, polished, and so simple to use, yet has every feature I could dream of" [1n5j9hk]; masking "comes a lot closer to adobe than others" [1qgs72y]; GPU speed; open source | Cannot import existing LR presets (converter promised) [1n5j9hk]; CPU/RAM use despite "GPU accelerated"; no catalog; solo maintainer | Too new to show a pattern. Interest is high among Linux and Adobe escapees |
| **Capture One** | Colour (Fuji, Nikon), tethering, fast masking and luminosity masks, one multi-select view with no Library/Develop switch [1gd742z]; perpetual licence | "Perpetual" is now locked to a minor version [1kthg48]; price; laggy in adjust mode even on M2 [1gd742z]; weak AI masks; no history panel; iPad lacks curves and grading [1053ru9]; LR import drops "the bulk of my edits" [alt1 c#745] | Pros leave for performance and price. Transition "painful and longer than expected" [1kthg48]. Edits and presets do not travel, so many stay on LR |
| **DxO PhotoLab / PureRAW** | Denoise ("Best noise reduction in town" [1in9812]; 10-15 s per 45 MP [1ofj4wm]); lens database; fast on Windows; one-time price | Masking "less precise than Lightroom" (`alternatives_4`); no catalog; lost RNI presets on move [1kthg48]; no ProRaw; upgrades cost | Mostly used beside LR as a denoise step. "still go back to lr6 for some stuff" [1j5zii5] |
| **ON1 Photo RAW** | Perpetual or subscription; LR-like layout; frequent specials | Slow, random crashes, "feels like a child's toy" [1pfsi92]; export does not always match the editor; LR migration drops edits and presets | Users drift back after months as inconsistencies pile up |
| **Luminar Neo** | Lifetime licence; fast AI object removal | Results "looked fake" [16zklma]; slower batch; weak DAM; "lifetime" caveats | Deleted after about 3 months in one report |
| **Affinity (Canva)** | Free since Oct 2025; one-time-price legacy; PSD fidelity | Not a LR replacement: "No database that keeps track of photos" [1pfsi92]; basic RAW [1p8237a]; Canva AI paywall and telemetry fears [1pk7rwg]; Wine on Linux is glitchy [1okcpwy] | Used beside a RAW editor and DAM, not instead of one |
| **Photo Mechanic / FastRawViewer / Bridge / XnView** | Culling speed: 2-3 min for 4k images vs LR's 45+ [1rn6wme]; "bloody hell it's quick" (`lightroom_2`) | Single-purpose; Photo Mechanic costs about $300 | Complement LR, and show that many users only need a fast culler |
| **Zoner, digiKam, NX Studio, DPP** | Zoner: catalog plus develop, about $59/yr. digiKam: tagging. NX/DPP: maker-accurate colour | Zoner has no AI denoise and is Windows only; digiKam UI is complex; NX/DPP are brand-locked with no library | Niche or used in pairs, not as replacements |
| **ImageForge Raw** (solo, closed) | Shows demand: claims to auto-detect the user's Adobe profiles [1vti3xk] | Reception: "AI slop," "vibe coded," "bus factor," no legal docs [1vti3xk] | Trust, not features, was the blocker. Linux users: "Not interested until you have a Linux version" [1vti3xk] |

**Why people stay or return** (20+ admissions in `alternatives_2`, 11 in `alternatives_1`), in order: muscle memory; catalog and DAM; presets already owned ("I have all my RNI film presets, which is what's keeping me using Lightroom" [1n5j9hk]); AI masking and denoise; colleagues and retouchers expecting LR catalogs; cloud and mobile; the tutorial ecosystem.

This matches 09 §(b). Reddit adds that **nearly all successful leavers kept using Lightroom for something**, and that hardly anyone reports a successful catalog-plus-preset migration. Migration came up in "only two mentions" in one chunk and about 8 in another. We read that as no one having a path, not as no one wanting one.

---

## 4. Presets: how people really use them, and their pain points

### 4.1 How they are used

- **As a base edit, then tuned per photo. This is the dominant pattern.**
  - "I paste them and tweak every single one. This is the whole point, presets are a good base edit" [1ow831y]
  - One user cut a 15-20 min edit to 8-9 min; event shooters spend 30-60 s per image [1ow831y].
  - Sports shooters process 4-5,000 images with presets (`presets_3`).
- **Order of application:** profile or preset first, then local edits and masks [1t8sduk]. Users rely on the Amount slider to dial strength without overwriting manual work [1t8sduk].
- **Stacking and "preset as mask":** "I have hundreds and they stack on top of each other" [1ow831y]. Some apply full-image masked presets for tone-curve or point-colour changes. Complaint: "traditional preset stacks usually break down the second you layer masks over them" [1t8sduk].
- **Import-time and triage presets:** auto-tone on import [1vfidw7]; a preset setting daylight WB, an Adobe profile and a medium curve to judge keepers quickly [1t8sduk]. Users copy and paste settings more often than they apply presets for batches (`lightroom_5`).
- **Learning tool:** users deconstruct purchased presets to see what moved. "deconstructing a preset can be valuable in learning how hues shift" [1u5saix].
- **Film emulation is the main purchase.** Alex Ruskman (10+ recommendations [1ecgr72], [1txdajn]), Mastin Labs, RNI, VSCO, Dehancer and Lou&Mark come up most. Some buyers buy **profiles only, not presets** [1t8sduk]. Adobe's built-in presets are called "surprisingly decent" [1vui9bn].
- **Culture:** buying is contested.
  - Anti: "Absolute waste of money. YOU are the one supposed to make presets" [1v7r39l]; the "snake oil" view.
  - Pro: "Every photographer who does high volume...will use presets. But they will use their own presets" [1v7r39l].
  - Implication: our users are mostly **people with an existing collection**, not buyers in a new market.

### 4.2 Pain points (ranked)

1. **The same preset looks different on different photos, cameras and light** (about 55 mentions; the top complaint).
   - "Presets almost never work out of the box" [1v7r39l]
   - "You always have to do heavy adaptations, because the images, lighting, etc are not the same" [1qxlyxl]
   - A R5 Mark II owner could not recover their 5D IV look [1ukcgjx].
   - Marketing mismatch: "it looks like you put on a piss filter, and now you need to adjust 8 different things" (`presets_0`, score 206).
2. **Colour-profile and camera dependence** (about 21).
   - Presets are "basically just moving sliders around based on Adobe Color/Standard" (`presets_0`).
   - "Adobe would make Fuji film emulations based on assumption the RAW file has exact colors of one camera model" [1fj46i9].
   - New cameras lack profiles, so presets are "bricked" (`lightroom_2`; Sony A7V [1woo33c]).
3. **White balance and exposure assumptions** (about 25). Absolute Temp/Tint in a preset is the usual failure.
4. **Marketing, price and quality variance** (about 33). Presets are "overpriced and overused" [1mzuvgr], and seller brigading is reported.
5. **Portability and lock-in** (about 12).
   - "Lightroom presets only work in Lightroom" [1b6xcih]
   - "lost all my not-so-cheap RNI style presets" on moving to DxO [1kthg48]
   - Conversion tools exist for Capture One. A vendor admits "sliders do not transfer in a similar manner" and "A totally perfect 1:1 recreation is unnecessary...as long as it maintains the overall feel" [17ewec4].
6. **Loss, sync and version problems** (about 15).
   - Presets lost in catalog corruption [1jkngmu].
   - Classic vs Cloud preset incompatibility.
   - Updates "break older presets" (`lightroom_0`).
   - Mobile DNG presets are "not as precise" [1u5saix].
7. **Stacking with masks** (about 3, but a structural problem) [1t8sduk].
8. **Sameness and over-processing** (about 8): teal-and-orange fatigue [1orc1w9].
9. **Organising hundreds of presets** (`lightroom_0`). No good search or filter.
10. **Film emulation authenticity.** "old prints have this muddy quality in the shadows that presets dont replicate" [1vlm018].

### 4.3 What users wish preset tools did

Adapt to exposure, WB and camera (about 6 mentions, partly "implied"); show what a preset changed; convert across apps well enough to "keep the overall feel"; support `.cube` LUTs ("Lightroom has no LUT support" [1u40tu9]); keep presets safe in sidecars and backups.

---

## 5. Linux and FOSS-user specific needs

- **Native, no-hassle install.**
  - Wine and Proton route is flaky: Affinity under Lutris has black tooltips and a flickering UI [1okcpwy]; its pen tool is broken [1i3w3yx].
  - DaVinci Resolve needs containers [1pk7rwg].
  - "Doesn't require command-line" and "works immediately without configuration" were stated as expectations (`alternatives_2`).
  - Darktable's calibration flow needing CLI and XML is cited as exactly what not to do [1uucnxa].
- **Colour management on Wayland.** "Real problem...is current lack of proper color management on Wayland" [1mf6kiw]. Serious Linux photographers are not satisfied until display profiles work.
- **No account, no telemetry, offline, no cloud.**
  - "I use Linux so I don't have to deal with dumb AI bullshit that Windows has" (`alternatives_1`, c#2128).
  - Affinity v3's opt-out telemetry and Canva coupling were treated as betrayal [1pk7rwg], [1okucv1].
- **Trust signals for new FOSS projects.** ImageForge Raw was attacked as closed-source and "vibe coded" [1vti3xk]. Visible governance, tests and an honest licence matter.
- **Distro stability.** Debian, Ubuntu and Fedora are preferred over rolling releases (`alternatives_1`).
- **Integration.** digiKam alongside darktable; Lua export to Immich [1rzdmst]. This points to a CLI and hook-friendly export.
- **Tolerance.**
  - Users accept a learning curve "if UX is logical" (about 2-12 weeks for darktable) and YouTube tutorials as documentation.
  - They do not accept crashes, data loss, hidden controls or a missing DAM (`alternatives_2`, `alternatives_4`).
- **Professionals** often dual-boot: "For creative work, if you need 100% reliability for your job, don't use Linux" [1rk7stv].

---

## 6. Design implications for Ambrotype (prioritised)

Targets below are **proposals**, not measured facts. "Roadmap" refers to SCOPE §8.

### 1. Performance as a first-class feature with published budgets
- **Evidence:** about 160 mentions, the top complaint. Windows is the worst-served platform, with GPU leaks and lag after 4 masks [1kbguc3]. Users run 10-25 masks (`lightroom_2`; 11 masks [19f7t8m]). Large catalogs hit 60k-250k [1rn6wme].
- **Decision:**
  - Budgets, tested in CI on Windows first (the worst-served platform):
    - slider to pixels under 100 ms at 24 MP, under 250 ms at 45 MP with 10 masks
    - next image in Develop under 500 ms (embedded preview first)
    - grid scroll at 60 fps on 100k images
    - catalog open under 3 s and search under 1 s at 250k images
  - Heavy work (denoise, previews, export) always runs in the background and never blocks the UI.
  - GPU memory is bounded and leak-tested.
  - Windows and Linux numbers are reported alongside macOS.
- **Roadmap:** budgets become Phase 0 spike exit criteria. Engine benchmarks go in the Phase 1 CI. UI budgets are a Phase 2 exit gate. Mask scaling is a Phase 3 gate. Budgets are **new** to SCOPE.

### 2. Stability and data safety: edits that cannot be lost, renders that never change
- **Evidence:** corruption and loss (about 18 mentions) [1jkngmu], an old catalog 3 weeks stale [1rn6wme], the LR6 shutdown [1pfcqlx], and darktable edits that broke after module changes [1axg02l].
- **Decision:**
  - XMP sidecars are the source of truth. The catalog is a rebuildable index and is never the only copy of an edit.
  - Atomic writes, WAL, integrity check at open, automatic rotating backups, and a one-click "rebuild catalog from sidecars".
  - Originals are never modified.
  - Recipes are versioned per process version, so a 2026 edit renders identically in 2030. Golden-image tests guard it.
  - Presets live as plain files in an open folder.
  - Crash recovery restores the exact slider state.
- **Roadmap:** sidecar invariants as a Phase 0 ADR. Sidecar read/write is Phase 2 (already). Backups and integrity checks are Phase 3 with the catalog. Process-version pinning is Phase 1 (already in SCOPE §7); the golden-image gate is **new**.

### 3. Catalog/DAM versus folder workflow: folder-live first, optional catalog second
- **Evidence:**
  - DAM is the top retention reason (§3, [1j5zii5], [1gd742z]).
  - But the catalog is also what is slow, fragile and "a relic" [1rn6wme].
  - Many users want a folder-live, no-import start: DxO, FastRawViewer, XnView, and "don't want a subscription plan to cull photos" (`lightroom_2`).
  - Affinity fails because there is no file management (`alternatives_1`).
  - Capture One's multi-select view with no Library/Develop switch is praised [1gd742z].
- **Decision:**
  - Phase 2 ships a folder-live browser over sidecars, with a filmstrip, multi-select and no mode switch.
  - A catalog is a Phase 3 opt-in index over the same folders: keywords, collections, smart collections, stacks and virtual copies.
  - Import is incremental, resumable and background-capable, never an all-or-nothing import of 500k images.
  - Browse and catalog modes stay clearly separated.
- **Roadmap:** matches Phase 2 and Phase 3 as written. Resumable import and smart-collection mapping are **new** in Phase 3.

### 4. Fast culling as an on-ramp
- **Evidence:** culling is "a decision point" (`lightroom_2`); Photo Mechanic's 2-3 min versus 45+ min for 4k images [1rn6wme]; free tools like CullPro and FastRawViewer.
- **Decision:** a culling mode that opens on the embedded JPEG, with rate/flag/label, compare and survey views, and keyboard-driven keep/reject.
- **Roadmap:** Phase 2. **New** emphasis. More colour labels were asked for (`lightroom_1`).

### 5. UX: Lightroom vocabulary and layout, not darktable's pipeline
- **Evidence:**
  - Darktable's UX is the most-cited reason FOSS fails to convert (§3; 21 mentions of the learning curve in one chunk [1uucnxa]).
  - "Why can't Darktable just provide an easy mode?" [1uucnxa]
  - Its UI "does not put basics front and centre" (`alternatives_1`, c#906).
  - "Lightroom can be used with no context" [1vz98el]
  - Switchers leverage existing tutorials.
- **Decision:**
  - Panels, names and slider order match Lightroom, so existing YouTube tutorials transfer. Basics (histogram, exposure, WB) are prominent.
  - No module enable switches, module order or scene-referred jargon in the main UI. Good defaults with no setup, inline help on every control, and an in-app Lightroom cheat sheet.
  - Copy/paste and sync settings in 3-4 clicks [1omx2jh].
- **Roadmap:** Phase 2 (SCOPE already says "Lightroom-like workflow"). **New:** a Phase 2 exit criterion that 5 Lightroom switchers complete a first edit and a preset import unaided in under 15 minutes.

### 6. The preset experience: import, Amount, per-photo adaptation, fidelity report
- **Evidence:** §4. Nobody renders Lightroom presets faithfully (09 §(d)). The top complaint is that presets do not adapt (about 55 mentions). Users value Amount, profiles-first and learning from a preset.
- **Decisions:**
  - **Import.** Drag and drop `.xmp`, `.lrtemplate`, zip packs and whole preset folders. Keep the pack's grouping, search, tags and favourites (organisation at scale). Separate **profiles** from **presets**. Hover shows a live preview on the current photo.
  - **Fidelity report on every preset**, as SCOPE §7 plans, written in plain language: what mapped exactly, what was approximated, what was ignored, and **which Adobe profile it assumed and whether you have it**. The report also doubles as a readable "what this preset changes" slider-delta list, which serves learning [1u5saix].
  - **Amount** for presets and profiles, and presets are applied as ordered, individually adjustable layers in the recipe, not as destructive overwrites. Presets must not erase manual edits [1t8sduk]. Include a "preset as full-image mask" application (the user pattern in [1t8sduk]), which makes stacking with masks robust.
  - **Apply-time groups:** include or exclude WB, exposure, tone curve, colour, profile, detail, lens, crop and masks. A default of "ignore absolute WB and exposure" for presets that set them cuts the "piss filter" failures (§4.2).
  - **Adaptation assist:** an explicit, labelled **non-Lightroom** option that normalises WB and exposure before applying the preset. It is off by default, so fidelity tests still compare like for like.
  - **Honest expectations:** a one-line banner when the user's camera differs from the preset's.
  - **Presets stay local:** no marketplace or sharing host (preset theft and brigading, §4.2).
- **Roadmap:** import, apply with Amount and the fidelity report are Phase 1 to 2 as in SCOPE §8. Apply-time groups and the preset browser are Phase 2. Layered presets, preset-as-mask and adaptation assist are **new** (Phase 2-3).

### 7. Colour rendering and camera support
- **Evidence:** Fuji users hate LR profiles [1fj46i9]; Capture One's colour is the top reason to switch (`alternatives_2`); new-camera profile delays brick presets [1woo33c].
- **Decision:** fitted per-camera base profiles ship quickly for new bodies (faster than Adobe, as a visible differentiator). The user chooses "match Lightroom" or "native" rendering (09 §(e)); the choice is never silent.
- **Roadmap:** Phase 1 (profile registry). A camera-support process and SLA is **new**.

### 8. Masking and AI expectations
- **Evidence:** masking is the top love (about 92). Wishes: polygon lasso, drag-reorder, luminosity masks, feathering (`lightroom_0`, `lightroom_2`). AI denoise "sold" Lightroom (`lightroom_5`). Forced AI and credits are hated (about 29), while "AI as a faster version of what we already do" is welcomed (`lightroom_2`).
- **Decision:**
  - Phase 3 masking ships with polygon lasso, drag-to-reorder, feather and refine, and luminosity and colour range.
  - AI subject and sky masks run locally with downloadable models.
  - **AI denoise moves forward from Phase 4.** Without it, switchers with high-ISO files stay with Adobe or add a DxO step [1ofj4wm]. A strong conventional NR is the Phase 2 stand-in.
  - Generative remove stays Phase 4, with LaMa as the substitute.
  - **No credits, no cloud, no nag screens.** AI is an optional download, off until the user enables it.
  - AI work never has priority over performance fixes.
- **Roadmap:** masks Phase 3 (as SCOPE). AI denoise Phase 3 late (**proposed change**). AI subject and sky Phase 3 (as SCOPE).

### 9. Export workflow
- **Evidence:** thin but clear. Exports not GPU-accelerated [1mp7fmn]; ON1 export does not always match the editor (`alternatives_4`); Lua/Immich upload [1rzdmst]; "faster to import into Photoshop, heal there, then export back" [1cx95f5].
- **Decision:** GPU-accelerated, queued, background and resumable export. What you see is what you export, checked by a preview-vs-export test. Export presets and naming templates. Post-export hooks through the CLI. "Edit in external app" for Photoshop, GIMP, Topaz and PureRAW-style tools.
- **Roadmap:** Phase 2 export (as SCOPE). Background queue, hooks and edit-in-external are **new** (Phase 2-3).

### 10. Mobile and sync: out of scope, handled by interoperability
- **Evidence:** mobile and cloud are loved by about 49 mentions but sync is distrusted (about 37 complaints). Most desktop users call mobile secondary. Privacy objections to the cloud (`lightroom_2`). Presets are often bought and used on phones, and mobile presets ship as DNG files (`presets_1`).
- **Decision:**
  - Say plainly: desktop-only, local-first, no cloud. For many users that is a selling point.
  - Make any sync tool work by design. The catalog is rebuildable and **only sidecars need to sync**, so Syncthing, Nextcloud, Dropbox or a NAS all work. Publish short recipes.
  - Accept **DNG-embedded preset settings** (mobile preset distribution) in the importer. Phone captures arrive as DNG by USB or sync.
  - Keep the Android option from SCOPE §2 for later.
- **Roadmap:** non-goal stays. DNG preset import is **new** (Phase 1-2). Sync recipes are Phase 2 docs.

### 11. Pricing, subscription-free messaging and trust
- **Evidence:** price was the second-largest complaint (about 99 mentions). Lock-in and perpetual licences that lapse hurt trust: LR6 [1pfcqlx], Capture One [1kthg48], Skylum, and Canva's AI paywall and telemetry [1pk7rwg]. Single-developer closed tools were rejected [1vti3xk]. A $25-150 one-time fee was seen as acceptable (`lightroom_2`), but this project is free.
- **Decision:** a short, public charter:
  - free forever under GPL
  - no account, no telemetry by default (crash reports opt-in)
  - fully offline
  - no AI credits and no paywalled features
  - open formats (XMP, plain-file presets)
  - governance and funding stated openly (fiscal host, SCOPE §9)
  - an AI-assisted-development policy with strong tests
- **Roadmap:** messaging and charter are **new**, Phase 0. A "no rug-pull" licence statement also ties to the open decisions in SCOPE §12.

### 12. Migration from Lightroom: metadata first, edits second, presets always
- **Evidence:**
  - "What do I do with years of LR edits?" [1nug81j]
  - "you can migrate the photos but you can't migrate the processing" [1j5zii5]
  - Capture One's import drops highlights/shadows, curves, masks and presets [alt1 c#745].
  - Metadata is the trusted part (09 §(c)).
  - "make sure to write all your metadata to either xmp or embedded" [1j5zii5]
- **Decision:** one wizard:
  - (a) A prepare checklist ("save metadata to files", export presets).
  - (b) Import ratings, labels, keywords, collections and flags first, from the read-only `.lrcat`.
  - (c) Develop settings go through the preset importer, with a **catalog-wide fidelity summary** (for example "N images fully mapped, M approximated").
  - (d) Auto-find and import the user's preset folders.
  - (e) Never write to the `.lrcat` and keep sidecars so rollback is possible.
  - Reading plain XMP sidecars needs no `.lrcat` and works at Phase 2.
- **Roadmap:** `.lrcat` import is Phase 3 as SCOPE says. **Proposal:** split out a metadata-only `.lrcat` import and pull it into late Phase 2 (§8). Smart-collection rule mapping is **new**.

---

## 7. Anti-goals: things users hate that we must never do

1. **No subscriptions, price hikes, credit systems, expiring "perpetual" licences, paywalled AI, or feature removal.** Evidence: [1mteppe], [1kthg48], [1pk7rwg].
2. **No account, login, activation server or forced cloud.** The LR6 shutdown [1pfcqlx] and Affinity's account and telemetry fears [1okcpwy], [1pk7rwg] show why.
3. **No default telemetry, no AI training on user images.**
4. **No data hostage-taking.** No cloud-only edits, no catalog that is the only copy, no proprietary-only presets.
5. **No silent colour or look changes**, and no "your edits transfer" claims when only exposure and WB do (09 §(e)). Do not silently drop preset parts.
6. **No UI churn or renamed and moved features without notice** (about 13 mentions). Never break old edits when the pipeline changes [1axg02l].
7. **No forced AI.** No nag screens, no AI work that starves performance fixes [1wt9nsw].
8. **No Classic/Cloud/Desktop split** (about 17 mentions). One app, one catalog model.
9. **No second-class Windows or Linux.** No "Mac-first optimisation".
10. **No hidden or disabled-by-default controls**, no deprecated-module clutter, no command-line or XML steps in the core flow [1uucnxa].
11. **No single point of corruption, unbounded memory growth, or crashes on large mask stacks.**
12. **No marketplace, ads, pay-to-unlock forums or preset brigading** (§4.2).
13. **No overclaiming.** Publish conformance numbers, not adjectives [1vti3xk].

---

## 8. Proposed changes to SCOPE.md (proposals only; not applied)

- **§2 Goals:** add (a) published performance budgets (§6 item 1), (b) data-safety principles (sidecar-as-truth, rebuildable catalog, versioned recipes, backups), (c) a "no rug-pull" charter (§6 item 11).
- **§2 Non-goals:** keep mobile out, and add "interoperate instead": document sidecar-sync recipes and accept DNG-embedded mobile presets.
- **§7 importer:** accept DNG-embedded preset settings; add apply-time include/exclude groups; add layered presets and preset-as-mask; add an opt-in, non-Lightroom adaptation assist (default off); extend the fidelity report with the assumed Adobe profile and a readable slider-delta list; add `.cube` LUT import as later (Phase 3 or 4).
- **§8 Phase 0:** add performance budgets as spike exit criteria; an ADR on data-safety invariants; a pricing and governance charter; a usability-test plan with Lightroom switchers.
- **§8 Phase 1:** add a perf-benchmark CI and a camera-support process (fitted per-camera fallback profiles).
- **§8 Phase 2:** add a culling mode on embedded previews, colour labels, copy/paste and auto-sync with groups, auto-tone, a preset browser with hover preview, search and favourites, GPU background export queue with hooks and "edit in external app", and a **metadata-only `.lrcat` import pulled forward from Phase 3**. Add a switcher usability exit criterion.
- **§8 Phase 3:** add polygon lasso, mask reorder and luminosity masks; **pull AI denoise forward from Phase 4**; resumable import; smart-collection mapping.
- **§10 Risks:** add "UX rejection like darktable" (Medium-High; mitigation: LR vocabulary, switcher tests), "Reputation/bus factor and 'vibe-coded' perception" (Medium; mitigation: governance, CI, AI-use policy), and "Presets will not look like the seller's ad on user photos" (High; mitigation: report, include/exclude groups, adaptation assist).
- **§13 Document index:** add this document.
- **Personas to test against:** high-volume event/sports pro, Adobe-escapee hobbyist, preset-invested stylist, large-catalog archivist, Linux/FOSS enthusiast.
