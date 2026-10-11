# 15 - What app-store reviews say: Lightroom mobile and competing photo editors

Status: design input, 2026-10-10. Feeds [SCOPE.md](../SCOPE.md) §2 (vision, non-goals), §8 (roadmap) and §10 (risks). Complements [12](12-user-feedback-and-design-implications.md) (Reddit) and the commercial survey in [09](09-market-commercial-alternatives.md). Where this document adds something doc 12 did not have, it is flagged **new**. Where it confirms doc 12, that is said so.

SCOPE puts mobile out of scope for now (an Android app may come later). The apps reviewed here are all phone and tablet apps. So the findings split into two groups:
- **Desktop-relevant:** pricing and subscription, paywalled features, AI tools, export quality, presets, editing UX, sync and login. These feed the desktop design directly.
- **Mobile-only:** touch controls, camera-roll integration, overheating, iPad layouts, tethering over Wi-Fi. These are recorded for the later Android effort (§8) and are not used in the desktop recommendations.

---

## 1. Method and caveats

**Method.** Review text was pulled on 2026-10-10 from two kinds of public pages and split into 22 chunks. A model wrote one facts file per chunk (themes, complaints, praise, requests). This document synthesises those facts files. It did not re-read every raw review, but the average-rating figures were checked against the chunk headers.
- **App Store (US):** seven iOS apps, about 1,870 reviews in total, taken from Apple's public customer-reviews feed. Per app, the feed gives the most recent and most helpful reviews, first three pages of each. Counts stated in the chunks: Capture One 163, Darkroom 278, Lightroom 298, Photoshop Express 299, Snapseed 256. Photomator and VSCO counts are not stated in the chunks.
- **Google Play (US):** four Android apps, but only the reviews the store shows in its "See all reviews" dialog after scrolling. That is roughly 260 each for Snapseed, VSCO and Photoshop Express, and about 40 for Lightroom. Play shows "helpful" votes, not star ratings, per review.

**Counts are indicative, not statistical.**
- The sample is a slice of recent and highly voted reviews, not a random draw. It reads far more negative than the store averages suggest. Lightroom's iOS average is 4.77 from about 337,000 ratings, yet most sampled text is complaints.
- Unhappy users write reviews, and people who feel a pricing change was unfair write more of them. Recent versions (mostly 2026) dominate.
- Where the facts say "several users" that is the strongest count available. Theme ranks in §3 come from how many of the 11 app listings show the theme and how often. They are rank order, not percentages.
- Play reviews on Lightroom are too few (about 40) to rank anything. Treat them as colour.
- Reviews in several languages appear; only a few are not English.

**Extraction-quality notes.**
- Several chunks carry no source line. The citation is the app-store page of the app id in the file name.
- Photomator reviews from 2020 to 2022 sit under its earlier name, Pixelmator Photo.
- One Play reviewer of Photoshop Express mentions a newer free Photoshop mobile app; the facts say this may be a different app. It is not used here.
- Reviewer names are omitted. Reviews are paraphrased, with no direct quotes, because the wording of a single review carries no extra weight here.

Sources: the eleven app-store pages listed in §2.

---

## 2. Per-app summary

Average ratings are the store-wide figures, shown in the chunk headers for App Store listings and in the fetched Play page for the others. "Not stated" means the source did not give it. The sample is described in §1.

| App (store) | Average rating | Top praise in the sample | Top complaints in the sample |
|---|---|---|---|
| **Capture One** (App Store) | 3.46 from 330 ratings | Fast, dependable tethering; raw colour; black and white; histogram | Separate monthly fee on top of a desktop licence; sign-in and account matching; catalog or session files cannot be opened on the iPad; one-way sync; missing tools (masks, curves, heal); camera gaps in tethering |
| **Darkroom** (App Store) | 4.78 from 29,361 ratings | Simple touch UI; film looks and grain; non-destructive edits; copy-paste batch edits; one-time lifetime purchase; privacy stance (no account, no cloud) | Crashes, especially on raw and batch export; export prompts a subscription; ads and upgrade banners; formerly free tools moved to paid; presets lost on update; no heal, noise reduction or white balance picker |
| **Lightroom** (App Store) | 4.77 from 336,931 ratings | AI remove and AI masks; presets and copy-paste; colour grading, HSL, curves; easy to learn; desktop sync | Almost every feature needs a subscription; "AI" branding and AI taking screen space; Recommended presets cut back in 2026; cancellation and trial billing; sync failures (edits, keywords, faces); crashes and heat |
| **Photomator** (App Store) | 4.55 from 3,808 ratings | One-time lifetime buy; Apple Photos integration; ML adjustments and denoise; clean native UI | Move from one-time purchase to subscription; licence checks fail offline; crashes with more than two masks; weak masks and heal; missing lens corrections; sidecar files far larger than the photo |
| **Photoshop Express** (App Store) | 4.75 from 741,274 ratings | Quick phone edits; collage; heal and red-eye; effects | Formerly free tools now paid; watermark and blurred export on free tier; AI credit pop-ups; removed tools (Combine, Fix, Mix); crashes; mandatory login |
| **Snapseed** (App Store) | 4.86 from 27,952 ratings | Free with no ads and no tiers; selective tool, curves, healing, film looks | The 4.0 redesign (tool panels shrink the photo; cartoon prompts); removed tools (white balance eyedropper, mask opacity, a few others); raw and HDR gaps; crashes |
| **VSCO** (App Store) | 4.69 from 277,787 ratings | Filters, film looks, recipes; calm, no-likes social side | Tools that were free for years moved behind membership; trial and cancellation billing; lost drafts and edits; camera-roll copies and overwrites; export differs from preview |
| **Lightroom** (Google Play) | Not stated | AI features and video noted by a few; pros use camera profiles on the go | Saves return the unedited original; blurry or mask-less exports; crashes after updates (several with hundreds of helpful votes); no native raw editing; cancellation fees |
| **Photoshop Express** (Google Play) | 4.5 | Easy, quick, no ads for some; high-resolution output on the free tier for some | Features moved to Premium after updates; save blocked by subscription pop-up; watermark; no layers or selections; mandatory Adobe sign-in; AI credits |
| **Snapseed** (Google Play) | 4.8 from about 1.79 million reviews (page figure) | Free, no ads, depth of tools; new colour grading welcomed | Redesign called childish and cluttered, with requests to restore the old UI; slower; export differs from preview, EXIF and name changes, 25 MP cap; raw formats fail |
| **VSCO** (Google Play) | 3.6 from about 1.29 million reviews (page figure) | Filters and presets; simple UI | Basic tools paywalled; trial billing and cancellation traps; black or unedited exports; import failures; crashes; Android buggier than iOS; no raw editing |

Sources:
- Capture One: https://apps.apple.com/us/app/capture-one/id1514081672
- Darkroom: https://apps.apple.com/us/app/darkroom-photo-video-editor/id953286746
- Lightroom: https://apps.apple.com/us/app/lightroom-ai-photo-editor/id878783582
- Photomator: https://apps.apple.com/us/app/photomator-photo-editor/id1444636541
- Photoshop Express: https://apps.apple.com/us/app/photoshop-express-photo-editor/id331975235
- Snapseed: https://apps.apple.com/us/app/snapseed-photo-editor/id439438619
- VSCO: https://apps.apple.com/us/app/vsco-photo-video-editor/id588013838
- Google Play: https://play.google.com/store/apps/details?id=com.adobe.lrmobile&hl=en_US&gl=US , https://play.google.com/store/apps/details?id=com.adobe.psmobile&hl=en_US&gl=US , https://play.google.com/store/apps/details?id=com.niksoftware.snapseed&hl=en_US&gl=US , https://play.google.com/store/apps/details?id=com.vsco.cam&hl=en_US&gl=US

---

## 3. Cross-app themes, ranked

Ranked by how many of the 11 listings show the theme and how strongly. "Desktop?" says whether it carries over to a desktop editor.

| Rank | Theme | Seen in (of 11 listings) | Desktop? |
|---|---|---|---|
| 1 | Subscription, price and billing | Complaints in 9; Snapseed praised for being free | Yes |
| 2 | Paywalled and removed features | About 7 | Yes |
| 3 | Crashes, lost edits, overheating | All 11 | Partly (crashes and lost edits yes; heat no) |
| 4 | Export quality and fidelity | About 10 | Yes |
| 5 | Presets and filters | About 8 | Yes |
| 6 | AI tools | About 9, mixed | Yes |
| 7 | Sync, login and licence checks | About 8 | Partly |
| 8 | Ads, upsells and nag prompts | About 6 | Yes (as an anti-goal) |
| 9 | UI redesigns and workflow breakage | About 5 | Yes |

### 3.1 Subscription, price and billing

Complaints fall into five kinds. Doc 12 had price and lock-in; the billing mechanics are **new**.
- **Price level and increases.** Reviewers of Lightroom say the annual price rose while features were removed. VSCO users report a rise with no stated reason. Capture One's mobile fee is seen as a second charge for people who already pay for a desktop licence, and one reviewer names a one-time price near $69 as acceptable. For Photoshop Express, requests for a one-time fee of about $25 recur.
- **Trial traps (new).** Free trials that need a payment method up front are a recurring complaint in Capture One, Lightroom, Photoshop Express, VSCO (App Store and Play) and Photomator. Users report being charged a full year within days, being charged after cancelling, and getting no reminder email.
- **Cancellation friction (new).** Subscriptions bought through a website cannot be cancelled in the app (VSCO). A Lightroom account sat under a third-party login the reviewer did not own. Play users report no cancel option during a trial. Refund requests are described as refused or unanswered.
- **Licence and entitlement checks (new).** A paid Photomator licence fails when the device is offline: the app refuses to save after about an hour, asks for restore-purchase on nearly every launch, or does not recognise a purchase made on another device. Photoshop Express re-checks the subscription at every launch, which delays premium tools by minutes. VSCO and Photoshop Express purchases are not recognised on a new phone.
- **Grandfathering broken (new).** Owners of one-time purchases of Darkroom, Photomator/Pixelmator Photo and Photoshop Express say they were moved to subscriptions, lost earlier perks, or were asked to pay again. Others praise a developer who honoured the earlier licence.

One-time purchase is the loudest positive in the whole sample. Darkroom's lifetime option and Photomator's lifetime buy are cited as reasons to choose them over Lightroom. Snapseed's price of nothing is the main reason people stay.

Sources: pages for all eleven listings in §2.

### 3.2 Paywalled and removed features

- **Basic tools moved behind a paywall** after users had them free for years. VSCO is the clearest case (grain, tone, clarity, highlights and shadows, split tone, HSL, copy-paste edits). Photoshop Express moved dehaze, saving at maximum quality, heal, background cut-out and layers. Darkroom moved a curves tool and HSL. Lightroom reviewers say basic masking and RAW editing on mobile need a subscription, and one says presets already paid for were locked.
- **Export is the toll gate (new).** Darkroom, Photomator, Photoshop Express and VSCO let free users edit but block saving, or add a watermark, or blur the export. Users call the store listing misleading when it says "free". A common trap: tap a premium tool by mistake, then the only choices at save are subscribe or discard all edits.
- **Paid, then removed (new).** Reviewers ask for refunds after paying for tools that were later dropped (Photoshop Express reshape, Combine Photos; VSCO journals; Darkroom clarity and sharpness in one release).
- **Purchased filter packs lost** after an Apple ID change or after a membership lapsed (VSCO).

Sources: pages for Darkroom, Lightroom, Photomator, Photoshop Express and VSCO in §2.

### 3.3 Crashes, lost edits, stability

Every listing has crash reports. Patterns relevant beyond phones:
- **Edits lost on crash or on a small action** (a final slider move, a save, a paste). Darkroom, Photoshop Express, VSCO and Photomator all have these.
- **Updates that break things.** Darkroom reviewers say each release made it less stable. Lightroom's 2026 updates left some users stuck at launch. Photomator broke on specific cameras' raw files after OS updates.
- **Masks are the stress point.** Slowdown and freezes with more than two or three masks appear for Darkroom, Photomator and Lightroom. This matches doc 12's "more than about 4 masks" complaint.
- **Presets and settings disappear on update** (Darkroom, Snapseed, Lightroom, Photoshop Express). **New** as an app-store pattern; doc 12 had catalog corruption but not preset loss on update.
- Phone heat, battery drain and dimming screens are mobile-only.

Sources: pages for Darkroom, Photomator, Lightroom, Photoshop Express and VSCO in §2.

### 3.4 Export quality and fidelity

Doc 12 §9 had thin evidence on export. App reviews add much more, and it is a major theme.
- **Saved output differs from the preview (new).** Brighter, darker, flatter or oversaturated results after save are reported for Darkroom, Lightroom, Snapseed, VSCO and Photomator. HDR and wide-gamut photos go flat. A crop can shift colour or contrast.
- **Saving returns the unedited original, or drops mask edits** (Lightroom on Play, VSCO on Play).
- **Resolution caps.** Snapseed exports cap around 25 MP even with the keep-resolution option on; some tools reduce 48 MP photos. Free tiers save at lower quality.
- **Metadata and names.** Snapseed strips EXIF and GPS, then restored GPS in a later version only to lose it again after another update, and changes file names to random strings. A Photoshop Express watermark added through metadata stops some apps posting the file.
- **Slow or hanging export** for batches and for raw (Darkroom, VSCO, Capture One).
- **Missing options (new, request side).** Custom file-name prefix with sequence numbers (Photomator, Lightroom has it), colour spaces such as Display P3, ProPhoto, AdobeRGB and sRGB, JPEG XL, scale by percent, size and target file size, one-tap export with the last settings, PNG with transparency, a save-as that keeps a copy.
- **Duplicate files.** Saving writes a new copy rather than updating (VSCO) or doubles storage through draft copies. Photomator sidecars for non-destructive edits can grow to many times the photo size.

Sources: pages for Darkroom, Lightroom, Snapseed, VSCO, Photomator and Photoshop Express in §2.

### 3.5 Presets and filters

- **Presets are a retention reason.** Darkroom, VSCO and Lightroom users value them as a quick base. VSCO users note the preset recipes of other apps can be rebuilt elsewhere, and some say they now build their own in Lightroom mobile.
- **No strength control (confirms doc 12 §6.6 and its "Amount" idea).** Lightroom's Recommended presets have no slider. Darkroom reviewers want subtler presets and one saw a saved 42 percent come back as 100 percent. Photomator and Snapseed have similar gaps.
- **Applying a preset wipes other edits (new).** Reports for Lightroom, Photomator, Snapseed and Darkroom: switching preset resets settings the preset does not touch, or reverts to the first preset used. Users also cannot tell which preset is applied.
- **Management is poor (new).** No renaming, no reorder or drag between groups on mobile, no preset sync across devices (Darkroom, Photomator, Snapseed), cannot delete own presets (Lightroom 11.6.1).
- **Import and sharing requests.** LUT or .cube import (Darkroom), importing presets on the phone with no desktop (Lightroom), a place to share presets (Darkroom), sharing styles between devices (Snapseed praised). Lightroom's Recommended tab was cut to a single row in August 2026 and reviewers named it a cancellation reason.
- **Copy and paste of settings across many photos** is a strong praise point (Darkroom, Lightroom) and a strong complaint when it is one photo at a time or crashes (Darkroom, Lightroom, Photomator, VSCO).

Sources: pages for Lightroom, Darkroom, Photomator, Snapseed and VSCO in §2.

### 3.6 AI tools

Mixed. Praise and irritation coexist, often in the same app.
- **Praised:** AI subject and sky masks, generative remove, AI denoise and super resolution, automatic keywording. Photomator reviewers say its automatic layer detection is close to Lightroom's.
- **Disliked as branding and as default behaviour (new in this detail).** Lightroom reviewers object to "AI" in the app title and to an always-visible AI strip, and ask for a switch to hide AI tools. Photoshop Express users want to hide AI features, and a Photoshop Express reviewer says upload was blocked until they agreed to buy credits. VSCO users say AI is pushed on them. Photomator runs automatic enhancement when a photo opens, and a reviewer cannot find a way to turn it off.
- **Credits.** Lightroom's generative tool became a tier that needs AI credits, and a reviewer calls it a downgrade for a paying subscriber. Photoshop Express users report credits spent on poor results and "no credits" errors on paid accounts.
- **Quality complaints.** Remove tools that add objects instead of removing them (Lightroom), weak object removal compared with the built-in phone tool (Photoshop Express, Snapseed, Photomator), subject masks that need manual clean-up (Photomator, Snapseed), and heal with no choice of source area.
- **Trust and consent (new).** A Lightroom reviewer says they cannot opt out of their work being used for AI features. A VSCO reviewer objects to terms about training AI on user content. A Photoshop Express reviewer objects to image generation built on artists' work.
- **Performance cost.** Snapseed and Lightroom reviewers blame AI features for slowness; Photomator's denoise and super resolution fail on a share of runs.
- **A false-positive label (new, single report).** One Lightroom reviewer says plain gradient edits on a real photo were flagged as AI content by a social network.

Sources: pages for Lightroom, Photoshop Express, Photomator, VSCO, Snapseed in §2.

### 3.7 Sync, login and licence

- **Sync drift between mobile and desktop** (Lightroom, Capture One, Darkroom, Photomator): edits made on one side do not reach the other, keywords and faces do not sync, collections differ across devices. One Lightroom wedding photographer reportedly had to redo a whole gallery. Capture One's sync is one-way.
- **Cloud storage side effects.** Lightroom reviewers say all camera-roll photos upload automatically, photos were erased when cloud storage became paid, and a library over 1 TB cannot be partly synced (selective sync requested).
- **Account lockouts (new).** A new phone with two-factor tied to an old number cost one Lightroom user their photos and presets. VSCO and Photoshop Express users report logouts, failed verification emails and no account recovery. Photoshop Express and VSCO make sign-in mandatory, and reviewers resent it.
- **Privacy asks.** Requests to grant access to chosen photos only, not the whole library (Lightroom, Photoshop Express, VSCO), and praise for Darkroom's no-account, no-cloud policy.
- **Library handling (mobile).** Edit copies stack up beside originals; deleting from the camera roll deletes the edit (VSCO); externally stored files cannot be culled (Capture One, Photomator).

Sources: pages for Lightroom, Capture One, Darkroom, Photomator, Photoshop Express, VSCO in §2.

### 3.8 Ads, upsells and nag prompts

Upgrade banners that cannot be dismissed, pop-ups after most edits, repeated rating requests (including during batch work), survey pop-ups that return after completion, and a splash screen on each launch. Photoshop Express and VSCO lead; Darkroom, Lightroom and Capture One have them too. Snapseed and some Photoshop Express Android users praise the lack of ads. Confirms doc 12's "no nag screens"; the specific forms are **new**.

Sources: pages for Photoshop Express, VSCO, Darkroom, Lightroom in §2.

### 3.9 UI redesigns and workflow breakage

- Snapseed's 4.0 redesign is the biggest single theme for that app on both stores: panels take screen space so the photo shrinks, buttons moved so muscle memory fails, a lighter background makes it harder to judge shadows, playful animations were disliked, and many ask for the old layout or a toggle. A few say it grew on them.
- Lightroom: a pick shortcut changed key and broke habits (Classic, 1/5); Learn tab tutorials moved to desktop only; no app-wide grid in the mobile develop view.
- VSCO: redesigns added taps for import and save, and removed swipe-to-undo.
- **New:** removed tools and moved controls produce loud reactions from long-time users, and some state they would stay on an old version if they could (VSCO, Snapseed).

Sources: pages for Snapseed, Lightroom, VSCO in §2.

---

## 4. Lightroom specifically, versus competitors

### 4.1 What users say about Lightroom mobile

**Praise.** AI remove ("game changer"-style comments), subject and sky selection, colour grading, HSL and curves, presets and quick settings paste, rating by swipe, tutorials, and RAW import from an SD card with sync to the desktop catalog. The desktop-companion role is liked: edit on the go, finish on the desktop. Reviewers of Capture One's mobile app also name Lightroom as a place they moved to.

**Complaints, ranked for Lightroom.**
1. Subscription: the app is free to download but almost all features need a plan; annual price rose while features were removed; trial charges; hard cancellation.
2. The 2026 AI-forward direction: AI in the app name, an always-on AI strip, credit-gated generative tools.
3. Mobile and desktop feature gap: point colour, layers, masking types, HDR merge, plugins and native raw on Play are missing.
4. Sync and cloud: edits lost between mobile and desktop, keywords and faces not syncing with Classic, auto-upload of the phone's camera roll.
5. Stability and heat; launch hangs after 2026 updates; Android crashes after updates.
6. Presets: Recommended list cut back, no strength slider, limited management.
7. Support: slow, automated, or hard to reach.

### 4.2 Comparison

| Point | Lightroom | Competitors in the sample |
|---|---|---|
| Price model | Subscription only | Darkroom and Photomator sell lifetime options (both have since moved partly to subscription); Snapseed is free; VSCO, Photoshop Express, Capture One are subscriptions |
| Free tier | Free to download, most tools gated | Snapseed fully free; Photoshop Express and VSCO gate basics and export; Darkroom blocks export |
| Masks and AI selection | The benchmark; Photomator reviewers say its automatic layer detection is close; others say Photomator masks are weak | Darkroom has depth and subject masks but no brush; Snapseed has none of the AI selections |
| Presets | Strong ecosystem, weak management on mobile | Darkroom and VSCO stronger as filter products; VSCO fidelity to film stocks questioned |
| Raw handling | Strong on iOS; none on Android | Photomator raw praised by some, weak white balance on raw for others; Darkroom shadow and highlight recovery questioned; Snapseed raw formats fail |
| Export | Fails or drops edits in some Android cases | Same class of preview mismatch in nearly all |
| Desktop link | The main reason to stay | Capture One's desktop link is one-way and tied to cloud; Darkroom and Photomator rely on Apple Photos and iCloud |
| Privacy | Cloud by default; AI-use opt-out requested | Darkroom praised for no account, no cloud |

**Switching patterns (new in detail).**
- People leave Lightroom for Darkroom, Apple's built-in editor, DaVinci Resolve and other apps after the 2026 changes. A few use Darkroom mainly for round-trip editing with the iCloud library, not because it is better.
- Many leave other apps for Lightroom after paywalls: VSCO, Darkroom, Snapseed (after its redesign) and Photoshop Express reviewers name Lightroom mobile as the destination.
- Leavers from Adobe name Photomator and Darkroom, mainly on cost; the lifetime option is the stated trigger.
- Net: Lightroom is where unhappy users of other editors land, and also the app they say they will drop when it adds AI branding or price rises. That makes mobile loyalty fragile on both sides.

Sources: pages for Lightroom (App Store and Play), Darkroom, Photomator, Capture One, Snapseed, VSCO, Photoshop Express in §2.

---

## 5. Mobile-only findings (input for the later Android effort)

Recorded here for SCOPE §2's later Android option. None of these affect the desktop plan.
- **Library choices.** Users want a choice between editing the camera roll copy and keeping originals untouched. VSCO's draft copies double storage and tie edits to camera-roll deletions. Photomator duplicates photos when merely viewing them. Reviewers praise save-a-copy and edit-in-place options.
- **Offline rules.** A licence check must work without a network. This also applies to desktop (§6.2).
- **Touch controls.** Dial-style controls and crowded gesture zones draw complaints (Capture One, Snapseed). Slider sensitivity that changes with swipe speed is praised (Darkroom). A left-handed toggle and hide-while-dragging toolbars are requested.
- **Heat, battery and memory** during masking, denoise and long edits on phones. Android: much less capable than iOS in VSCO and Photoshop Express (no raw, fewer tools, buggier).
- **iPad and big screens.** Reviewers ask for tablet-specific layouts, external-drive libraries with ratings and culling, and clean external-monitor output for proofing.
- **Tethering and camera capture** (Capture One): live view, wireless support gaps, camera cards keeping raw while the device gets JPEGs.
- **Import friction.** SD card via adapter is a loved flow when it works (Lightroom, Darkroom). Photo pickers that hide folders (Snapseed) and 100-photo import limits are not.

Sources: pages for Capture One, Darkroom, Photomator, Snapseed, VSCO, Lightroom in §2.

---

## 6. Design implications for Ambrotype (prioritised)

Targets are **proposals**, not measured facts. "Roadmap" refers to SCOPE §8. Each item notes whether it confirms doc 12 or is **new**.

### 1. Pricing, licence and trust: say it, and prove it in behaviour
- **Evidence:** price and billing complaints lead the sample (§3.1). Trial traps, cancel friction, offline licence failures, broken grandfathering and paid-then-removed features are all concrete.
- **Decision:** confirms doc 12 §11. Add specifics, which are **new**:
  - The charter says in one line: no account, no sign-in, no licence check, so the app works fully offline from the first launch.
  - No feature is ever removed after release without a deprecation period and a documented export path. Keep a public changelog that names removed or changed behaviour.
  - Never move a shipped tool behind anything. This is moot for a free project, but it should be written down (SCOPE §12 "no rug-pull").
  - If a donation or sponsor tier ever exists, it must not gate editing or export.
- **Roadmap:** Phase 0 messaging; changelog policy Phase 1.

### 2. Export is the product: preview equals output, tested
- **Evidence:** §3.4. Output differing from the preview appears in nearly every app, as do resolution caps, stripped metadata and altered file names.
- **Decision:** extends doc 12 §9 (**new** detail):
  - A preview-vs-export regression test for every release, covering colour space, HDR/wide gamut, crop and rotate.
  - Never cap resolution; never change file names unless the template says so; metadata (EXIF, GPS, ratings) is kept by default and stripped only by an explicit, visible option.
  - Export presets with: file-name templates and sequence numbers, scale by percent or long edge, target file size, colour spaces (sRGB, Display P3, AdobeRGB, ProPhoto), JPEG XL, PNG with alpha, watermark text that actually uses the entered text.
  - One-click "export with last settings". Save-a-copy is the default; overwrite never.
  - Keep sidecar size bounded: do not store large caches (denoise or upscale results) inside sidecars (**new**; Photomator reports sidecars many times the photo size).
- **Roadmap:** Phase 2 export (as SCOPE); the regression test belongs in the Phase 1 harness.

### 3. Presets: strength slider, additive apply, safe storage
- **Evidence:** §3.5. Missing strength control, presets wiping other edits, presets lost on update.
- **Decision:** confirms doc 12 §6.6 (Amount slider) and adds three **new** rules:
  - Applying a preset changes only the settings it contains, and an undo step restores the earlier state. "Reset sliders the preset does not mention" is an explicit, off-by-default choice.
  - The panel shows which preset is currently applied (or "modified") and lets users rename, reorder, group, favourite and delete their own presets.
  - Presets live as plain files in a user folder, never inside an app database, so an update or reinstall cannot lose them. Include a migration test in CI: install an old build, update, check presets.
  - Accept LUT (.cube) files as look presets.
- **Roadmap:** Phase 1 file format; LUT import Phase 2-3.

### 4. AI tools: optional, hideable, local, no credits
- **Evidence:** §3.6. Praise for masks, remove and denoise; heavy irritation at branding, forced prompts, credits and unclear data use.
- **Decision:** confirms doc 12 §8 (AI optional, no credits). **New** details:
  - A single switch hides all AI UI, and the product name never leads with "AI".
  - No automatic enhancement on open. Auto settings are a button the user presses.
  - Every AI result is an editable adjustment stack (masks, sliders), not a replaced image, so users keep control. A prompt-based tool that generates a whole new image is out of scope.
  - Plain statement that no photo leaves the machine and nothing is used for training. This is also the privacy selling point against Lightroom's cloud model.
  - Do not label output as AI-generated when only adjustments were made (**new**; cover this in the metadata policy).
  - Heal and remove: let the user choose the source area, and show the result before committing.
- **Roadmap:** as doc 12 §8.

### 5. Editing UX: steady layout, discoverable tools, no regressions
- **Evidence:** §3.9 and Photomator, Darkroom, Snapseed reports on sliders, panels and crop.
- **Decision:** supports doc 12 §5 (Lightroom vocabulary and layout). **New**:
  - Typed values for every slider; sliders that remember where zero is; fine-adjust with a modifier key.
  - Before/after toggle and history list are always reachable (missing in several mobile apps).
  - Keep keyboard shortcuts compatible with Lightroom's defaults, and make them rebindable. A shortcut change broke habits in Lightroom.
  - Layout changes ship behind a "classic layout" option for at least two releases.
  - Panels should never hide the photo; offer a collapse and a hide-while-dragging option.
  - Copy and paste of settings to many photos at once, including masks where possible. This is praised when it works and criticised where limited to one photo or global edits.
- **Roadmap:** Phase 2 develop UI.

### 6. Stability and data safety: update-proof state
- **Evidence:** §3.3. Crashes on batch export, lost edits on a final action, preset loss on update, mask slowdown.
- **Decision:** confirms doc 12 §2 and §1 (budgets). **New** tests:
  - A soak test of batch export and paste-to-many with more than 4 masks.
  - Autosave of the edit state after every change, so a crash loses nothing.
  - An upgrade test: sidecars and presets from the previous release must open and render identically.
  - Crash reporting is opt-in; the app offers to save a report locally.
- **Roadmap:** Phase 1-2 harness.

### 7. Sync: sidecar-based, with explicit conflict rules
- **Evidence:** §3.7. Edits lost between devices, one-way sync, auto-uploading everything, account lockout.
- **Decision:** confirms doc 12 §10 (only sidecars need to sync; works with Syncthing, Nextcloud, Dropbox or a NAS). **New:**
  - Publish how conflicts resolve (latest modified wins, with the older version kept as a backup in the sidecar folder).
  - Selective sync is a matter of folder choice, so large libraries can stay partly local.
  - No account recovery problem exists because there is no account.
  - Document that keyword and face data are stored in sidecars or the catalog and which one syncs.
- **Roadmap:** Phase 2 docs.

### 8. Library handling on the desktop
- **Evidence:** camera-roll duplicates, delete-the-original-deletes-the-edit, storage doubling (§5, §3.4).
- **Decision:** reinforces doc 12 §3 (folder-live first). **New:** never copy originals on view; edits are sidecars beside the file; deleting or moving an original in the app asks first and says what happens to the edit; "save a copy" is clear in the UI.
- **Roadmap:** Phase 1-2.

### 9. Anti-goals additions
Add to doc 12 §7: cap on resolution; watermark on output; blurred preview as a sales tool; trial that needs a card; offline licence checks; removing a tool after release without notice; renaming or restructuring menus in a point release; "AI" in the product name; upgrade prompts inside the editing flow.

### 10. For the later Android effort (SCOPE §2)
- Use §5 as the checklist: choose-between-edit-in-place and keep-original; offline-first licence-free operation; tablet layouts; heat and memory budgets for masking and denoise; SD-card import that survives reconnects; raw support from day one (the Android raw gap hurts VSCO and Lightroom).
- Reuse the desktop sidecar format so no sync service is needed.
- The Play sample is small and Lightroom's is about 40 reviews. Re-sample before any Android scoping.

---

## 7. What is new relative to doc 12

- Trial-and-billing mechanics, offline licence failures, and broken grandfathering (§3.1, design item 1).
- Export as the toll gate in free tiers, and the width of export fidelity problems (§3.2, §3.4, design item 2).
- Preset apply behaviour and storage loss (§3.5, design item 3).
- AI branding, hide switch, credit gating, consent and the AI-label false positive (§3.6, design item 4).
- Redesign backlash as a first-class risk (§3.9, design item 5).
- Sidecar bloat and camera-roll duplication (§3.4, §5).
- Switching flows between mobile editors, and the point that Lightroom is both the main destination and a source of leavers (§4.2).

---

## 8. Limits of this document

- Reviews are self-selected, recent and highly voted. They overstate complaints against store averages.
- The Lightroom Play sample is about 40 reviews. Nothing about Android Lightroom should be ranked from it.
- No sample for Lightroom's desktop apps; those come from doc 12.
- Statements about "several users" are the only counts available for many items.
- Not collected: Apple Store reviews outside the US storefront, reviews older than the pages fetched, Play reviews beyond those shown in the dialog, and any review of Lightroom Classic as a separate listing.
