# Known gaps and open questions

This is the register of every known gap, limitation, guess, open question and "later" item in Ambrotype, collected from the docs and the code, for future refinement. It is updated as items close: when something is built, fitted, measured or decided, change its row or delete it. Dated 2026-10-05, against the tree at that date. It lists things we can point to a source for; it does not rank features that nobody has mentioned.

## How to use

Each row has an item (area plus a one-line description), a tag, why it matters, and the source: a link to the doc, or `path:line` for code. Line numbers drift, so search for the nearby wording if one is off.

| Tag | Meaning |
|---|---|
| **Missing** | Not built. |
| **Approx** | Built, but the model or its constants are our own guess and not fitted to Lightroom. |
| **Limit** | Built, with a known limitation or a deliberate choice. |
| **Question** | Needs a Lightroom measurement or a project decision before we can settle it. |
| **Legal** | A legal, licensing or naming item. |

Project goal for ranking: faithful Lightroom preset compatibility, free, on Windows, macOS and Linux. The "Top 10 to close next" list at the end is ordered by impact on that goal.

## 1. Product, legal and process

| Item | Tag | Impact | Source |
|---|---|---|---|
| Product renamed from "Flightbroom" to "Ambrotype" (for now); trademark search, domains and handles not done; the GitHub repo is renamed to `shardpelt/Ambrotype` | Legal | A rename after launch costs users, links and search ranking; it touches repo, crates, `amb` binary, `amb:` XMP namespace URI and docs | [legal-and-naming.md](legal-and-naming.md) (Status, §2); [SCOPE.md](SCOPE.md) §12.6 |
| Trademark registers (TMview, WIPO, classes 9 and 42) not searched for the final pick; they were unreachable when the shortlist was made | Legal | An existing mark could force a second rename | [legal-and-naming.md](legal-and-naming.md) §2 (note before §3), §3.1 |

| No `LICENSE` file and no `license` field in any Cargo.toml. GPL-3.0-or-later is recommended but README says "not yet chosen" | Legal | The repo cannot go public. Relicensing is practically impossible once outside code or ported GPL code is in, so decide first | [legal-and-naming.md](legal-and-naming.md) (Before going public); [SCOPE.md](SCOPE.md):199, 426; `README.md` |
| `docs/adobe-docs/` holds about 1,000 verbatim Adobe quotes (about 12,000 words) that must be paraphrased before the repo goes public | Legal | Volume is beyond what the quotation right comfortably covers | [legal-and-naming.md](legal-and-naming.md) §1 table, row "Committing those notes publicly" |
| Trademark notices are in the README, NOTICE and the app's About tab (2026-10-08); keep them wherever we name Lightroom or Fujifilm | Legal | Notice "Lightroom is a trademark of Adobe Inc.; not affiliated with or endorsed by Adobe" is required wherever we say "imports Lightroom presets" | [legal-and-naming.md](legal-and-naming.md) §1 (rows on saying "imports Lightroom presets", before making public) |
| Counsel review of purchased-preset seller licences (some forbid conversion) before 1.0 | Legal | Reading a user's own presets is the defensible case; not confirmed by counsel | [SCOPE.md](SCOPE.md):221 |
| One-hour IP-lawyer review before 1.0 or before donations at scale | Legal | Docs are an engineer's reading, not legal advice | [legal-and-naming.md](legal-and-naming.md):3 |
| Patent risk "low, not zero" for local-Laplacian, dark-channel and Adobe dehaze/local-contrast methods; the Microsoft dark-channel patent is only "reportedly lapsed" | Legal | Our Dehaze is a dark-channel model and Texture/Clarity are guided-filter models | [legal-and-naming.md](legal-and-naming.md) §1 (Patents); [research/05](research/05-processing-algorithms.md) §6, §18.5; [research/02](research/02-preset-and-profile-formats.md):308 |
| DNG SDK licence vs GPL not reviewed by counsel; whether the SDK's ACR3 default curve table may be redistributed is open | Legal | Affects whether we can reuse SDK constants | [research/02](research/02-preset-and-profile-formats.md) (Open questions); [research/05](research/05-processing-algorithms.md) §20 |
| Funding model open: donations plus a fiscal host (Open Collective); NLnet round closed 2026-06-01 and Sovereign Tech Fund excludes end-user apps | Question | No funded path for a multi-person-year effort | [SCOPE.md](SCOPE.md):383; [legal-and-naming.md](legal-and-naming.md) §1 (Donations) |
| Open decision: the fidelity target that defines "compatible" (proposal: median dE00 < 2, p95 < 5 over the top-20 tags) | Question | No pass/fail criterion for the engine yet | [SCOPE.md](SCOPE.md):428 |
| Open decision: who holds the Lightroom Classic subscription, and where reference renders are stored (private; only CC0 inputs are committed) | Question | Blocks the calibration month | [SCOPE.md](SCOPE.md):429 |
| Open decision: contact RapidRAW's maintainer about a shared Adobe-compatible engine crate | Question | Could save or duplicate large effort | [SCOPE.md](SCOPE.md):430 |
| Open decision: own UI shell vs a RapidRAW-fork UI (Phase 2 gate, settled by a viewport spike plus the maintainer conversation) | Question | Shapes the whole GUI effort; GUI work with Tauri 2 is being started now | [SCOPE.md](SCOPE.md) §5, §12.2 |
| Phase 0 spikes have no recorded result and there is no `docs/decisions/` (ADRs): wgpu 100 MP viewport (Tauri vs Qt), fp16 vs fp32, `ort` + SAM2-tiny, `.lrcat` read on 3 catalog versions | Question | Decisions on GPU, UI and AI runtime are unrecorded | [SCOPE.md](SCOPE.md):276-285; [research/06](research/06-tech-stack-options.md) §8.6 |
| Governance not written: DCO sign-off, contributor AI-use policy, no-rug-pull charter, ADR on data-safety invariants, usability-test plan with Lightroom switchers. No `CONTRIBUTING.md` | Missing | Reputation and bus-factor risks rated Medium | [SCOPE.md](SCOPE.md):198, 284, 402 |
| Repo hygiene from the architecture study not in place: `cargo-deny`, pinned `rust-toolchain.toml`, a fuzz target per parser (TIFF/DNG, XMP, ICC, DCP, lrtemplate) | Missing | Raw files and presets are untrusted input | [SCOPE.md](SCOPE.md):380-382 |
| Camera-support process (per-camera fitted fallback profiles, sample-submission pipeline) not built | Missing | Users complain about Adobe's delays supporting new cameras | [SCOPE.md](SCOPE.md):297, 396 |

## 2. UI

| Item | Tag | Impact | Source |
|---|---|---|---|
| GUI v1 exists (`crates/amb-gui`: Tauri 2, plain HTML/CSS/JS, CPU preview), but there is no GPU viewport yet | Limit | Previews are rendered on the CPU from a downscaled copy and sent as JPEG | [ARCHITECTURE.md](ARCHITECTURE.md); `crates/amb-gui/src/session.rs` |
| Editor lacks: preset hover preview, window state, filmstrip, ([design/customization-and-hub.md](design/customization-and-hub.md)). Layouts have two side columns, not free splits or tabs; each setting can be shown once. Editor mode needs a mouse or touch to move things (no keyboard-only moving yet), and merging panels keeps the second panel's title as a heading. Keyboard shortcuts can be changed in Settings, but there are no shortcuts yet for nudging the focused slider or stepping through panels (Lightroom's `+`/`-`, `,`/`.`), and shortcut sets can't be shared. The 1:1 view renders the whole image at full size before cropping the region (about 33 s in a debug build); it needs a tiled render. The crop tool is tested in the GUI only on normal-orientation photos (rotated/mirrored ones are covered by Rust tests), has no JS tests, keeps the preset name in the aspect list after an X swap, and doesn't write `CropWidth`/`CropHeight`/`CropUnits` | Missing | Lightroom switchers expect these on day one | `crates/amb-gui/ui/app.js`, `crates/amb-gui/ui/crop.js`; `crates/amb-gui/src/session.rs` (`region`) |
| Exported AVIFs carry the rating and keywords in EXIF only (no XMP yet), and exported EXIF lacks some fields strict validators expect (resolution, ColorSpace, ExifImageWidth/Height, FlashpixVersion). The library shows one folder at a time (no subfolders, collections, stacks, compare view or metadata panel), has no pick/reject flags (Lightroom keeps those in its catalogue, not in sidecars) and no undo for rating, label and keyword changes; keyword hierarchies are only kept, not edited ([design/library.md](design/library.md)). Plugins are edit-only for now (see the plugin row below). The collage maker has no edge-dragging of grid cells, no undo of its own and no snapping when arranging freely, uses each photo's own edit (no per-cell copies), and its batch shapes come from the camera preview (a crop is ignored when matching photos to cells); collage layouts aren't in the hub or the CLI yet; borders have no caption or EXIF line yet. The hub covers layouts and themes only (not presets, size profiles or collage templates), has no ratings or screenshots, and its repo (`Ambrotypehq/ambrotype-hub`) exists but isn't filled from `hub/` yet. Submitting goes through a browser link, so very large layouts must be added by hand | Missing | Agreed features for sharing looks and layouts | [design/customization-and-hub.md](design/customization-and-hub.md); [design/collage-and-borders.md](design/collage-and-borders.md) |
| Export dialog exposes long-edge resize and exact sizes from size presets only (no short edge/fit/megapixels/percent), no subsampling, progressive or AVIF speed; no export presets or batch export | Missing | The CLI has more export options than the GUI | `crates/amb-gui/ui/index.html`; [ARCHITECTURE.md](ARCHITECTURE.md) export section |
| Preview is rendered at screen size from an area-downscaled raw, so pixel-radius effects (sharpening, NR, grain, Texture/Clarity, defringe) look different from the full-size export | Approx | What you see is not exactly what you export at 1:1 | `crates/amb-gui/src/session.rs` (`downscale`, `preview`) |
| Preview speed measured only in a debug build under Xvfb: about 0.6-1.0 s for an 860 px preview of a 24 MP raw (release is faster but unmeasured); no draft/progressive refinement beyond a 0.6x drag size | Question | Slider latency is the top Lightroom complaint | `crates/amb-gui/ui/app.js` (`previewSize`) |
| Display is not colour-managed: previews are sRGB JPEGs shown as-is by the webview | Limit | Wide-gamut screens show over-saturated colour | `crates/amb-gui/src/session.rs` |
| No automated GUI tests (only the Rust session/schema tests); CI only builds it | Missing | UI regressions go unnoticed | `crates/amb-gui/src/session.rs` tests |
| **Auto** button (auto tone and white balance, per-slider auto), deterministic and calibrated against Lightroom's Auto | Missing | The fastest way to a good start; Lightroom users expect it | [design/future-ideas.md](design/future-ideas.md) |
| **Randomize** ("Surprise me") button: seeded random looks in sensible ranges, with locks, strength and variants | Missing | Playful discovery of looks; easy presets | [design/future-ideas.md](design/future-ideas.md) |
| **Tutorial mode**: guided lessons on a sample raw, per-photo "what does this slider do" strips, Lightroom-switcher lesson, optional AI coach | Missing | Makes the app approachable; a free alternative to paid courses | [design/future-ideas.md](design/future-ideas.md) |
| **Feedback button and usage insights**: Bug/Idea/"Looks different from Lightroom"/Slow, pre-filled GitHub issue, opt-in local usage report (incl. not-rendered preset settings) | Missing | Decide what to build next from real use; privacy-first | [design/future-ideas.md](design/future-ideas.md) |
| Develop UI parts not built: histogram and clipping warnings, history and snapshots, copy/paste settings with groups, auto-tone, colour-managed display (ICC display profile) | Missing | Expected by Lightroom switchers | [SCOPE.md](SCOPE.md):303-311 |
| Preset UX not built: apply-time include/exclude per setting group, layered presets, apply a preset through a mask (preset masks themselves are applied), preset browser with hover preview, search and favourites, opt-in adaptation assist | Missing | Addresses the top preset complaint ("looks nothing like the ad") | [SCOPE.md](SCOPE.md):253-259 |
| Folder browser with grid and filmstrip, rating/flag/label, filters; culling mode on embedded previews | Missing | Needed even before a catalog exists | [SCOPE.md](SCOPE.md):302, 310 |
| GPU background export queue, post-export hooks, "edit in external app" | Missing | Batch workflow | [SCOPE.md](SCOPE.md):312 |
| Native viewport risk: wry `build_as_child` is X11-only on Linux; Wayland needs the `gtk::Fixed` route; viewport-over-webview stability on all three OSes is unproven | Question | Could force Qt 6 or a different layering | [research/08](research/08-cross-platform-and-mobile.md):35; [SCOPE.md](SCOPE.md):395 |
| Film Negative panel has the Auto switch, stocks and print controls, but no "freeze analysis" button or "Sync roll" yet | Missing | Roll consistency still needs `amb negative analyze` | [design/film-negatives.md](design/film-negatives.md); `crates/amb-gui/src/schema.rs` |

## 3. Library and catalog

| Item | Tag | Impact | Source |
|---|---|---|---|
| No library or catalog: folders, ratings, keywords, collections, search | Missing | No DAM at all; scope-explosion risk is rated High, so v1 is develop-first | [ARCHITECTURE.md](ARCHITECTURE.md):84; [SCOPE.md](SCOPE.md):394 |
| SQLite catalog (WAL, FTS5), import copy/move/add, keyword hierarchy and synonyms, collections, smart collections, stacks, virtual copies, preview pyramid | Missing | Phase 3 scope | [SCOPE.md](SCOPE.md):321, 326 |
| `.lrcat` migration (read-only: folders, keywords, ratings, labels, collections, develop settings). Modern catalog layout is unverified; the known facts are from LR 2/4/6 | Missing | The main thing holding people in Lightroom | [SCOPE.md](SCOPE.md):313, 322; [research/06](research/06-tech-stack-options.md):425 |
| Data-safety principles have no implementation yet: XMP sidecars as source of truth, rebuildable catalog, versioned recipes, automatic catalog backups | Missing | Catalog corruption is the number-three Lightroom complaint | [SCOPE.md](SCOPE.md):62-66 |
| Folder-live browsing with no import step | Missing | A competitor strength we chose to adopt | [SCOPE.md](SCOPE.md):122 |
| Phase 4 items: tethering (libgphoto2), print, map/GPS, face recognition, HDR/panorama merge, focus stacking | Missing | Parity features, deliberately last | [SCOPE.md](SCOPE.md):131, 332 |
| Soft proofing | Missing | Needed for print workflows; also relevant to gamut mapping | [SCOPE.md](SCOPE.md):323 |

## 4. Raw decoding and colour

| Item | Tag | Impact | Source |
|---|---|---|---|
| JPEG, PNG, TIFF and WebP open (2026-10-08), but not HEIC, AVIF or PSD. Only matrix/TRC ICC profiles are read (LUT-based ones fall back to sRGB). Their edits go to `photo.jpg.xmp`; Lightroom keeps JPEG edits inside the file, which we read but don't write. A JPEG next to a same-name raw is always hidden (Lightroom has a setting to show both). Thumbnails of rendered images are full decodes (cached), not embedded previews | Partial | Lightroom edits non-raw files too | `crates/amb-engine/src/raster.rs`; `crates/amb-color/src/icc.rs`; `crates/amb-gui/src/folder.rs` |
| 4-colour CFA sensors are rejected ("not supported yet") | Limit | Those cameras cannot be rendered | `crates/amb-engine/src/raw.rs`:95-99, 118-122 |
| Cameras without a colour matrix in rawler are rejected | Limit | Unsupported cameras give an error, not a render | `crates/amb-engine/src/raw.rs`:69-85 |
| Black/white scaling and the Bayer demosaic (PPG) are rawler's; X-Trans uses our port of darktable's Markesteijn 1-pass (the 3-pass variant, with fewer false colours on thin lines, isn't ported). No choice of demosaic (AMaZE/RCD). Clipped highlights are only made neutral (clipped channels raised to the brightest one), not reconstructed from neighbouring detail. No capture sharpening or deconvolution | Missing | Roadmap lists RCD/AMaZE, highlight reconstruction and capture sharpening for Phase 1-2 | `crates/amb-engine/src/raw.rs`, `xtrans.rs`, `pipeline.rs` (`Unclip`); [SCOPE.md](SCOPE.md):128, 290, 315 |
| One XYZ-to-camera matrix per camera (first of D65, D50, D55, A); no interpolation between the two DNG calibration illuminants, no ForwardMatrix, no BaselineExposure | Limit | Colour of the base render differs from the DNG reference render | `crates/amb-engine/src/raw.rs`:69-85; [SCOPE.md](SCOPE.md):291; [research/05](research/05-processing-algorithms.md) §1 |
| No DCP camera profiles (HueSatMap, LookTable, ToneCurve) and no Adobe-like base look. A placeholder filmic ACES-fit curve is used and is explicitly "NOT Adobe's curve" | Missing | Every preset renders through the profile, so this is the biggest colour gap | [ARCHITECTURE.md](ARCHITECTURE.md):87; `crates/amb-engine/src/pipeline.rs`:56-59; [SCOPE.md](SCOPE.md):25-29 |
| Fitted per-camera replacement of Adobe Color / Standard / Neutral / Landscape / Portrait / Vivid / Camera Matching profiles not done (planned from Lightroom renders, week 3) | Missing | We cannot ship Adobe's DCPs, so a substitute is the only way to match "Adobe Color" looks | [lightroom-trial-plan.md](lightroom-trial-plan.md):54; [SCOPE.md](SCOPE.md):28, 392 |
| Reading the user's own installed Adobe profiles in place (like the LCP reader) not built | Missing | Most faithful option when the user has them | [SCOPE.md](SCOPE.md):26, 251, 392 |
| Adaptive profiles (image-adaptive AI, intensity 0-200) and adaptive presets are "not reproducible"; fallback is a base profile plus a warning | Limit | Presets using them will differ | [SCOPE.md](SCOPE.md):244, 332 |
| Camera Raw "Calibration" panel: Shadow Tint and Red/Green/Blue Hue and Saturation (7 tags, all planned, none rendered) | Missing | Common in teal-and-orange style presets | `crates/amb-presets/data/tags.json` (group `calibration`); [research/05](research/05-processing-algorithms.md) §11 |
| Black and white mixer renders (`ConvertToGrayscale` and 8 `GrayMixer*`, 2026-10-08), but its strength per band is a guess (unfitted), there is no Auto mix (`AutoGrayscaleMix`), and B&W camera profiles don't exist | Approx | B&W presets look roughly right, not exactly | `crates/amb-engine/src/color.rs`; `crates/amb-presets/data/tags.json` (group `bw_mixer`) |
| Point Color (`PointColors`, planned); its encoding (9 floats per entry) is from memory | Missing | Newer presets use it | `crates/amb-presets/data/tags.json`; [research/02](research/02-preset-and-profile-formats.md):159 |
| White balance: `IncrementalTemperature` and `IncrementalTint` render on non-raw images with our own scale (1 mired and 1 Tint unit per step, unfitted); Adobe's conversion is unknown | Approx | Relative-WB edits look roughly right, not exactly | `crates/amb-engine/src/pipeline.rs` (`incremental_neutral`); [research/02](research/02-preset-and-profile-formats.md):119 |
| Temp/Tint uses the table-based Robertson method (1667 K and up); Adobe's newer extended range below 2000 K and the interpolation details are unmeasured | Question | Low-Kelvin readouts may differ from Lightroom | `crates/amb-color/src/temperature.rs`:12-17; [research/05](research/05-processing-algorithms.md) §2 |
| A Custom white balance without Temperature in the settings is not rendered; without Tint it renders with tint 0 (the as-shot value lives in the raw file). Tint alone is refused | Limit | Editing Tint alone on an as-shot photo needs a workaround | `crates/amb-protocol/src/state.rs`:671-678 |
| Curves are applied on sRGB-encoded values "for now"; Adobe's curve domain is unknown (ProPhoto with sRGB TRC vs gamma 1.8) | Question | Every point-curve preset depends on it | `crates/amb-engine/src/pipeline.rs`:651; [research/05](research/05-processing-algorithms.md) §8, §20 |
| Exact tone-curve interpolation and Adobe's Medium/Strong Contrast point sets unknown; legacy `ToneCurve` and `ToneCurveName*` tags planned | Question | Curve shape differences are visible | [research/02](research/02-preset-and-profile-formats.md) (Open questions, line 138); `crates/amb-presets/data/tags.json` |
| Process-version numbers after "11.0" (PV5/PV6, 13.x/15.x) are from memory; ProcessVersion-to-preset mapping is "not stated in docs" | Question | Version gating of presets may be wrong | [research/02](research/02-preset-and-profile-formats.md):102, 259, 329; [adobe-docs/camera-raw.md](adobe-docs/camera-raw.md) |
| LibRaw fallback in a sandboxed helper process not built (rawler only) | Missing | Resilience for cameras rawler cannot decode | [SCOPE.md](SCOPE.md):375, 379 |

## 5. Tone and colour models

All of these are "Model v1 (ours, not Adobe's)": every constant is a starting guess in the `Calibration` struct, awaiting fitting (see section 14).

| Item | Tag | Impact | Source |
|---|---|---|---|
| Highlights and Shadows are local and edge-aware since 2026-10-08 (a guided-filter base with the region curve on top; detail kept, no halos at strong edges), but their strength, curve shape and radius are our guesses, not fitted to Lightroom | Approx | Nearly every preset sets them | `crates/amb-engine/src/local_tone.rs`, `tone.rs` (`region_lut`); [SCOPE.md](SCOPE.md):393; [research/05](research/05-processing-algorithms.md) §4 |
| Contrast, Whites, Blacks and parametric-curve strengths and shapes are guesses (`ToneStrengths`) | Approx | Tone of every render | `crates/amb-engine/src/tone.rs`:12-13 |
| Whether PV2012 Whites/Blacks are purely global is unknown | Question | Decides if they need a local model too | [research/05](research/05-processing-algorithms.md) §20 |
| Exposure: Adobe's soft highlight roll-off for positive Exposure is only inferred; ours is a plain gain after the placeholder base look | Question | Highlight handling in bright scenes | [research/05](research/05-processing-algorithms.md) §3 |
| Parametric curve is a smooth-bump model; Adobe's formula is undocumented | Approx | Parametric-curve presets | [research/05](research/05-processing-algorithms.md) §8; `crates/amb-engine/src/tone.rs` |
| Vibrance, Saturation, 8-band HSL and Color Grading run in OkLab/OkLCh; HSL band centres, weights and hue mapping are guessed from HSV hues; Adobe's band centres are "not stated in docs" | Approx | HSL and grading are in most looks | `crates/amb-engine/src/color.rs`:5-9, 16; [research/05](research/05-processing-algorithms.md) §9; [adobe-docs/classic-develop.md](adobe-docs/classic-develop.md) |
| Color Grading region masks, Blending and Balance shapes are guesses; legacy Split Toning is mapped onto the shadow/highlight wheels | Approx | Grading look differs | `crates/amb-engine/src/pipeline.rs`:306-322; [research/05](research/05-processing-algorithms.md) §10 |
| Texture and Clarity are a guided-filter base/detail model; Adobe's is likely Local-Laplacian-style. Texture rated "hard" to match | Approx | Local-contrast look differs | `crates/amb-engine/src/local.rs`:3-10, 70; [research/05](research/05-processing-algorithms.md) §5 |
| Dehaze is a dark-channel-prior model; Adobe also adds contrast, saturation and tone shaping (inferred). Rated "hard" for fidelity | Approx | Dehaze look differs by content | `crates/amb-engine/src/dehaze.rs`:8, 63; [research/05](research/05-processing-algorithms.md) §6 |
| Optional "native" scene-referred rendering mode (filmic/sigmoid/AgX) and a tone equaliser not built | Missing | Planned next to the Lightroom-match mode | [SCOPE.md](SCOPE.md):126-127, 315, 327 |

## 6. Local adjustments and retouch

| Item | Tag | Impact | Source |
|---|---|---|---|
| Masks render and edit since 2026-10-08: linear, radial, brush, luminance and colour range, add/subtract/intersect, the local sliders, in Lightroom's own tags (the old per-tool tags render too). The fall-off curves, Feather mapping, brush dab model and every local slider's strength are our guesses | Approx | Masks look right but not yet like Lightroom's | [design/masks.md](design/masks.md); `crates/amb-engine/src/mask.rs` (`MaskStrengths`) |
| Unverified mask conventions: a radial's `Angle` sign, what a brush `Radius` is relative to (we use the longer side), how Lightroom stores erasing in the current format, the brightness scale of luminance ranges | Question | Lightroom-made brushes or turned radials may land differently | [design/masks.md](design/masks.md) open questions |
| Lightroom's colour-range samples (`AreaModels`) are in an unknown format: its colour ranges select nothing here (reported). Ours keep their own sample positions in `amb:ColorSamples`, which Lightroom won't read | Partial | Colour-range masks don't cross between the two apps | [design/masks.md](design/masks.md) |
| Not rendered inside masks: Moire, Defringe, `LocalHue`, `LocalGrain`, local curves, `LocalPointColors`; depth ranges | Missing | Kept in the file and reported | `crates/amb-presets/src/masks.rs` (`UNRENDERED_FIELDS`) |
| Mask editing not built: reordering masks and parts, duplicating, polygon lasso, mask presets, auto-mask (edge-aware brush), the old per-tool tags (render only) | Missing | Lightroom has them | [design/masks.md](design/masks.md); [SCOPE.md](SCOPE.md):320, 324 |
| AI masks (subject, sky, background, people, objects, landscape) are never stored as pixels by Adobe, so presets carrying them must be regenerated with our own segmenters (SAM2/BiRefNet via `ort`); today they select nothing and are reported | Missing | Boundary fidelity will always be approximate | `crates/amb-engine/src/mask.rs`; [SCOPE.md](SCOPE.md):244, 324 |
| No heal/clone/spot removal; `RetouchAreas` in presets are ignored by design (image-specific) | Missing | Retouch is part of the Lightroom develop surface | [ARCHITECTURE.md](ARCHITECTURE.md):86; `crates/amb-presets/src/report.rs`:173-178 |
| Phase 4 retouch and AI: content-aware remove (LaMa), Lens Blur (Depth Anything V2-S), Super Resolution, people parts, depth-range masks | Missing | `LensBlur` and Enhance tags are marked unsupported | [SCOPE.md](SCOPE.md):331; `crates/amb-presets/data/tags.json` |
| Permissive-only AI model manifest with a CI gate not built. RMBG-1.4/2.0 and Depth Anything Base+ are non-commercial and must not be bundled | Legal | A wrong model would taint the GPL distribution | [SCOPE.md](SCOPE.md):156, 215 |

## 7. Detail and effects

| Item | Tag | Impact | Source |
|---|---|---|---|
| Sharpening is an unsharp mask with Detail as an overshoot limiter; Adobe's Detail also mixes in a deconvolution-like term (inferred). Radius, Detail and Masking curves are guesses | Approx | Sharpened look differs; rated medium (low dE impact) | `crates/amb-engine/src/detail.rs`:9-33, 150; [research/05](research/05-processing-algorithms.md) §12 |
| Noise reduction measures noise on the image because there is no camera noise profile; Adobe's NR is undisclosed. Constants guessed | Approx | NR strength differs by ISO and scene | `crates/amb-engine/src/detail.rs`:40-44; [research/05](research/05-processing-algorithms.md) §13 |
| Colour NR keeps an unused `color_low_res_radius` field only so old calibration files still load | Limit | Dead field in the calibration schema | `crates/amb-engine/src/detail.rs`:254 |
| Post-crop vignette (3 styles) is a model guess; it is now centred on and scaled to the crop | Approx | Wrong centre and extent once a preset carries a crop | `crates/amb-engine/src/effects.rs`:8-40, 118; [research/05](research/05-processing-algorithms.md) §16 |
| Grain model (size, roughness, midtone weighting) is a guess. Whether Lightroom stores and reuses a per-image seed (`GrainSeed`) is unknown ("check") | Approx | Grain texture will not match pixel for pixel | `crates/amb-engine/src/effects.rs`:40-70; [research/05](research/05-processing-algorithms.md) §16 |
| Enhance (Denoise AI, Raw Details, Super Resolution) writes a new DNG in Lightroom; our 8 `Enhance*` tags are unsupported. No open equivalent of AI Denoise; AI denoise is brought forward to Phase 3 | Missing | The AI feature users say sold them on Lightroom | `crates/amb-presets/data/tags.json` (group `enhance`); [SCOPE.md](SCOPE.md):325; [research/05](research/05-processing-algorithms.md) §13 |
| Lens-softness correction from measured lens data (DxO-style) | Missing | "Later: needs data we don't have" | [SCOPE.md](SCOPE.md):132 |

## 8. Lens and geometry

| Item | Tag | Impact | Source |
|---|---|---|---|
| No Upright or Transform. Crop and straighten render (`HasCrop`, `Crop*`, `CropAngle`), but their angle sign, corner semantics and interaction with Constrain Crop are unverified against Lightroom ([design/crop.md](design/crop.md)); 17 transform tags are unrendered and 8 `Perspective*` tags are planned | Missing | Presets and sidecars carrying perspective render unleveled; crops may differ for off-centre straightened crops | [ARCHITECTURE.md](ARCHITECTURE.md):86; [design/lens-corrections.md](design/lens-corrections.md):18; `crates/amb-presets/data/tags.json` (groups `crop`, `transform`) |
| Upright and Transform to be done as a homography in front of the existing single resampling pass. Auto Upright is rated "hard" (port ART/darktable `ashift`); XMP carries post-solve parameters, so numeric transforms may not need a solver | Missing | Architecture photos | [design/lens-corrections.md](design/lens-corrections.md):88; [research/05](research/05-processing-algorithms.md) §15; [SCOPE.md](SCOPE.md):320 |
| Transform slider numeric ranges, Projection (added in 18.3) and the parameter-to-homography mapping are "not stated in docs" | Question | Needs Lightroom measurement | [adobe-docs/camera-raw.md](adobe-docs/camera-raw.md); [adobe-docs/classic-develop.md](adobe-docs/classic-develop.md); [research/05](research/05-processing-algorithms.md) §15 |
| Built-in lens profiles in raw files are not read: maker notes (Micro Four Thirds, Fujifilm X, Sony E, many compacts) and DNG `WarpRectilinear`/`FixVignetteRadial` opcodes. The `Embedded` source exists but is "not read yet" | Missing | Lightroom applies these automatically; lenses with no database entry stay uncorrected | [design/lens-corrections.md](design/lens-corrections.md):83-86; `crates/amb-lens/src/model.rs`:131 |
| Fisheye profiles and projection changes unsupported | Missing | Fisheye lenses cannot be corrected | [design/lens-corrections.md](design/lens-corrections.md):18, 89 |
| Local Defringe inside masks unsupported (masks exist; the local slider is kept but not rendered) | Missing | Local fringe fixes don't show | [design/lens-corrections.md](design/lens-corrections.md):18 |
| lensfun database is not bundled and `amb lens fetch` does not exist. It must come from a system install, `AMB_LENSFUN_DB` or `--lens-db`. When shipped, keep CC BY-SA 3.0 attribution | Missing | Out of the box, no lens correction without a manual database | [design/lens-corrections.md](design/lens-corrections.md):43-45, 87 |
| `ProfileHint.name` (`LensProfileName`) is read from the edit but unused; only `LensProfileFilename` selects an LCP directly | Limit | An edit that names a profile without a filename falls back to auto-match | `crates/amb-lens/src/lib.rs`:56-59, 76-83 |
| LCP auto-match prefilters by file name, else by file text, across every LCP found; on very large Adobe installs the scan may be slow | Limit | First render with a large profile folder; no timing recorded | `crates/amb-lens/src/lcp.rs`:599-640 |
| lensfun calibrations are independent of Adobe's, so results differ a little from Lightroom | Limit | Exact match needs the user's own LCP files | [design/lens-corrections.md](design/lens-corrections.md):44 |
| Profile matching checked on only two real raws (D750 + Tamron SP 35 mm f/1.8; EOS R10 + Sigma 150-600 C) | Limit | Matching heuristics are lightly tested | [design/lens-corrections.md](design/lens-corrections.md):69-72 |
| Does Lightroom fill blank areas with white (`blank_fill` is 1.0, "to verify")? Does profile auto-scale also zoom out for barrel lenses? | Question | Edge appearance and framing of corrected images | [design/lens-corrections.md](design/lens-corrections.md):28, 76; `crates/amb-engine/src/lens.rs`:43 |
| With Remove CA off, does an LCP's CA model still apply (`profile_tca_always`, default no)? | Question | Whether CA is corrected by profile alone | [design/lens-corrections.md](design/lens-corrections.md):77; `crates/amb-engine/src/lens.rs`:41 |
| Profile amount semantics are guesses: linear displacement for distortion, gain power for vignetting | Question | Partial-amount presets | [design/lens-corrections.md](design/lens-corrections.md):78 |
| Manual Distortion strength, Lens Vignetting stops and Midpoint mapping, Defringe hue-slider-to-hue mapping and edge width, and auto-CA estimation constants all need fitting | Approx | Lens panel numbers will be off | [design/lens-corrections.md](design/lens-corrections.md):3, 79; `crates/amb-engine/src/lens.rs`:28-50 |

## 9. Film negatives

| Item | Tag | Impact | Source |
|---|---|---|---|
| Auto levels assume the deepest shadows and brightest highlights are close to neutral; this fails on sunset-type rolls (workaround: freeze values from another frame) | Limit | Colour casts on unusual scenes | [design/film-negatives.md](design/film-negatives.md):34 |
| Positive (slide) film unsupported: levels and base handling without inversion | Missing | Slide scans cannot use the tool | [design/film-negatives.md](design/film-negatives.md):71 |
| No dust and scratch removal (infrared channel where available) | Missing | Scan quality | [design/film-negatives.md](design/film-negatives.md):70 |
| No frame detection or auto-crop of the rebate (also needs crop support) | Missing | Manual framing of every scan | [design/film-negatives.md](design/film-negatives.md):70 |
| Halation and grain are not matched to the stock | Missing | Stock looks are colour and tone only | [design/film-negatives.md](design/film-negatives.md):72 |
| Film stocks are our own generic looks (13) with no measured data; measured stock profiles and a way to share them as data are "next". Chromogen already sells measured stocks | Missing | "Looks like Portra" claims are deliberately avoided | [design/film-negatives.md](design/film-negatives.md):47-53, 73; [SCOPE.md](SCOPE.md):149 |
| Model v1 constants (2.2-decade scene range, 18 % midtone, 0.2/99.8 % level percentiles, 0.3 % base fraction) are ours; the docs record only a synthetic scanned negative as a test target | Approx | Real-scan quality unknown | [design/film-negatives.md](design/film-negatives.md):26-45; `crates/amb-engine/src/negative.rs`:32-60 |
| Does Lightroom keep `amb:` tags when it rewrites a sidecar (Metadata, Save Metadata)? | Question | If it strips them, negative settings are lost on a round trip | [design/film-negatives.md](design/film-negatives.md):5; [lightroom-trial-plan.md](lightroom-trial-plan.md):50 |
| The `amb:` namespace URI `http://ns.ambrotype.org/develop/1.0/` uses a domain we don't own yet; it must stay stable once sidecars exist in the wild | Legal | Existing sidecars with `amb:` tags need migration after a rename | [design/film-negatives.md](design/film-negatives.md):5; [legal-and-naming.md](legal-and-naming.md) (Status) |

## 10. Presets and compatibility

| Item | Tag | Impact | Source |
|---|---|---|---|
| Preset Amount is a provisional linear lerp that extrapolates above 100 %. Adobe's maths is undocumented. Non-numeric settings (including curves) are taken whole whenever Amount is above 0, although SCOPE says curves and LUTs blend toward identity | Question | The Amount slider is a headline preset feature | `crates/amb-presets/src/amount.rs`:3-8; [SCOPE.md](SCOPE.md):250; [lightroom-trial-plan.md](lightroom-trial-plan.md):51 |
| Two Amount sliders exist (preset Amount since ACR 14.4, and profile Amount only for Adaptive, Artistic, B&W, Modern, Vintage profiles); neither scaling is documented, and which settings opt out of Amount is "not stated in docs" | Question | Needs measurement | [SCOPE.md](SCOPE.md):250; [adobe-docs/classic-develop.md](adobe-docs/classic-develop.md); [adobe-docs/camera-raw.md](adobe-docs/camera-raw.md) |
| `Look` and `CameraProfile` tags are planned; the profile registry is not built and `RGBTable`/`Table_<md5>` LUT decoding is not implemented (no complete public decoder known; only the DNG SDK is authoritative) | Missing | Creative-profile presets lose their colour look; `amb preset inspect` warns | `crates/amb-presets/src/report.rs`:135-152; [SCOPE.md](SCOPE.md):251; [research/02](research/02-preset-and-profile-formats.md):241 |
| PV2003 and PV2010 presets: 7 legacy tone tags are planned (not rendered), while the inspect warning says they "will only be approximated"; conversion to the nearest 2012 model is not built | Limit | Old presets render incorrectly or not at all | `crates/amb-presets/src/report.rs`:128-131; `crates/amb-presets/data/tags.json` (group `tone_legacy`); [research/02](research/02-preset-and-profile-formats.md):316 |
| `Enable*` group toggles (`EnableDetail`, `EnableLensCorrections`, `EnableToneCurve`, ...) are not in the registry and the pipeline never reads them | Missing | A preset with a group switched off still renders it | [research/02](research/02-preset-and-profile-formats.md):219; `crates/amb-presets/data/tags.json` (no `Enable*`); `crates/amb-engine/src/pipeline.rs` |
| `.cube` LUT import not built | Missing | Common non-Adobe look format | [SCOPE.md](SCOPE.md):261 |
| HDR editing tags (`HDREditMode`, `HDRMaxValue`, `SDR*`, 9 tags) unsupported | Limit | HDR-edited photos render as SDR | `crates/amb-presets/data/tags.json` (group `hdr`); [SCOPE.md](SCOPE.md):332 |
| Tag registry ranges, defaults and steps are best-known values, mostly unverified; several (Point Color, LensBlur, HDR tags, mask enums) are from memory | Question | Wrong clamping or defaults change preset results | `crates/amb-presets/data/tags.json`:2; `crates/amb-protocol/src/reference.rs`:75; [research/02](research/02-preset-and-profile-formats.md):329 |
| No crs: tag has status "exact": all 105 rendered Camera Raw tags are "approx". The 13 "exact" tags are our own `amb:` negative settings | Approx | No Camera Raw setting is yet fitted to Lightroom | `crates/amb-presets/data/tags.json` |
| Preset corpus (purchased and free presets, private, never committed) not assembled for end-to-end regression | Missing | No whole-preset fidelity score is possible | [lightroom-trial-plan.md](lightroom-trial-plan.md):15, 55 |

### Tag registry status (253 tags, 2026-10-08)

Counted from `crates/amb-presets/data/tags.json`. `amb render` reports the planned and unsupported tags as "not rendered yet" when a preset sets them.

| Status | Tags | Notes |
|---|---|---|
| approx | 120 | rendered with an unfitted model: tone, presence, tone curve (11), HSL (24), color grading (13), split toning (5), detail (10), effects (10), lens (18), white balance (5), B&W mixer (9), local (masks) 4 |
| planned | 32 | tone_legacy 7 of 7; calibration 7 of 7; point_color 1 of 1; profile 6 of 6 (`CameraProfile`, `CameraProfileDigest`, `Look`, `RGBTable`, `LookTable`, `OverrideLookVignette`); transform 8 (`Perspective*`); tone_curve 3 (`ToneCurve`, `ToneCurveName`, `ToneCurveName2012`) |
| unsupported | 18 | enhance 8; hdr 9; lens_blur 1 |
| image-specific | 41 | crop 12; transform 17; negative 10; bw_mixer 1; tone 1 (not expected in presets; also not rendered, because there is no crop) |
| exact | 13 | our own `amb:` negative settings only |
| metadata | 26 | preset descriptors and process version |
| ignored-in-presets | 3 | `RetouchAreas`, `RetouchInfo`, `RedEyeInfo`, by design |

## 11. Export

| Item | Tag | Impact | Source |
|---|---|---|---|
| TIFF has no EXIF and no compression (uncompressed only; LZW/ZIP "next") | Limit | Large files and no metadata in the Photoshop round trip | [ARCHITECTURE.md](ARCHITECTURE.md):56; `crates/amb-export/src/lib.rs`:7 |
| AVIF is sRGB only, 8-bit, with no ICC embedded; other colour spaces are refused | Limit | No wide-gamut or HDR AVIF | [ARCHITECTURE.md](ARCHITECTURE.md):58; `crates/amb-export/src/lib.rs`:222-227 |
| WebP is lossless only (lossy needs libwebp) | Limit | Large WebP files | [ARCHITECTURE.md](ARCHITECTURE.md):57; `crates/amb-export/src/lib.rs`:49 |
| No JPEG XL (via libjxl, or lossless pure Rust) | Missing | Modern delivery format | [ARCHITECTURE.md](ARCHITECTURE.md):70 |
| No HDR AVIF/JXL or gain-map export | Missing | HDR output; Lightroom HDR editing is also unsupported | [ARCHITECTURE.md](ARCHITECTURE.md):72; [SCOPE.md](SCOPE.md):332 |
| No HEIC/HEIF, by choice (HEVC patent licensing); AVIF covers modern delivery | Limit | Apple-ecosystem delivery needs AVIF or JPEG | [ARCHITECTURE.md](ARCHITECTURE.md):67 |
| No PSD, by choice; TIFF covers the Photoshop round trip | Limit | No layered Photoshop output | [ARCHITECTURE.md](ARCHITECTURE.md):68 |
| No DNG export of the developed image | Missing | No interchange of a developed result as DNG | [ARCHITECTURE.md](ARCHITECTURE.md):73 |
| No watermarks, filename templates or export presets | Missing | Batch export workflow | [ARCHITECTURE.md](ARCHITECTURE.md):74-76; [SCOPE.md](SCOPE.md):306 |
| Integer outputs hard-clip to the gamut; no gamut mapping | Limit | Saturated colours clip when exporting to sRGB or P3 | `crates/amb-export/src/lib.rs`:241; `crates/amb-engine/src/output.rs`:21; [ARCHITECTURE.md](ARCHITECTURE.md):77 |
| Output sharpening (screen, matte, glossy; low/standard/high) is Model v1 with guessed radius and strength | Approx | Differs from Lightroom's output sharpening | `crates/amb-export/src/geometry.rs`:176-185 |

## 12. AI control

| Item | Tag | Impact | Source |
|---|---|---|---|
| Token-efficient AI layer: ≤ 300-token core card with per-group references on demand, compact `amb do` command syntax, `--brief` diff-only output, `amb stats --brief` (numbers instead of images), token budget checked in CI | Missing | Cheaper and more reliable AI control; current AI docs total about 19,000 tokens | [design/future-ideas.md](design/future-ideas.md) |
| MCP server (`amb mcp`) not built (Phase 2: develop, preset, render-preview and export tools) | Missing | The "bring your own AI" route depends on it | [ARCHITECTURE.md](ARCHITECTURE.md):88; [design/ai-control.md](design/ai-control.md):23, 62 |
| `amb match` (deterministic look matching from sample images) not built (Phase 2, v1 global tone and colour statistics) | Missing | The headline AI feature | [ARCHITECTURE.md](ARCHITECTURE.md):88; [design/ai-control.md](design/ai-control.md):30-47, 63 |
| Protocol covers `set_param`, `adjust_param`, `reset_param`, `reset_group`, `apply_preset`, the mask commands (`add_mask`, `set_mask`, `paint_mask`, `remove_mask`), `undo`, `redo` plus queries; `render_preview`, `image_stats`, `match_look`, `export`, `list_presets` are in the design but not in the CLI | Missing | AI clients cannot render a preview or export through the protocol | [design/ai-control.md](design/ai-control.md):16-17; `crates/amb-cli/src/main.rs`:35-40 |
| `--json` is not on every CLI command (it exists on preset inspect/census, edit, negative, settings; not on render, lens, sweep, calibration) | Limit | Phase 1 goal was `--json` everywhere | [design/ai-control.md](design/ai-control.md):57-59; `crates/amb-cli/src/main.rs`:13-62 |
| In-app assistant panel (user API key or local model such as Ollama/llama.cpp, keys in the OS keychain) not built | Missing | Phase 3 | [design/ai-control.md](design/ai-control.md):24, 64-65 |
| Vision-model refinement loop and match with masks (sky, subject) not built | Missing | Phase 3 | [design/ai-control.md](design/ai-control.md):45, 66-67 |
| Safety UX not built: pending-diff Accept/Undo, permission prompts, step and cost limits, user-visible log of AI tool calls | Missing | Required by the design's own rules | [design/ai-control.md](design/ai-control.md):49-54 |
| Learn-my-style from edit history, and batch "apply my look to this shoot" | Missing | "Later" | [design/ai-control.md](design/ai-control.md):68 |

## 13. Performance and GPU

| Item | Tag | Impact | Source |
|---|---|---|---|
| No GPU backend (wgpu), tiling, ROI planning or per-node caching; the engine runs whole-image CPU passes (rayon) | Missing | Interactive sliders need a fast preview path; a full 24 MP render is about 3.5-4 s on 4 cores including decode | [ARCHITECTURE.md](ARCHITECTURE.md):48, 85; `crates/amb-engine/src/lib.rs`:5-8; [SCOPE.md](SCOPE.md):345 |
| Lens warp costs about 1.5 s per 24 MP on 4 CPU cores; GPU resampling is "next". Defringe is a separate full-image CPU pass after the warp and has no recorded timing | Limit | Biggest single stage in a lens-corrected render | [design/lens-corrections.md](design/lens-corrections.md):90; `crates/amb-engine/src/lens.rs`:18, 651 |
| Default Sharpening plus colour NR take about 1.0 s per 24 MP; Luminance NR (off by default) adds about 1.4 s | Limit | NR-heavy presets are slow | `crates/amb-engine/src/detail.rs`:86-91 |
| Performance budgets (slider-to-preview latency, import, export, mask count, catalog size) not written, and no benchmark CI (CI runs fmt, clippy and tests only) | Missing | "Performance is a feature"; Lightroom's top complaint is lag | [SCOPE.md](SCOPE.md):59-61, 284; `.github/workflows/ci.yml` |
| GPU precision and wgpu limits unresolved: fp16 vs fp32 intermediates, storage-texture formats, buffer sizes, HDR swapchain control | Question | Banding and speed on Intel, AMD, NVIDIA and Apple GPUs | [SCOPE.md](SCOPE.md):279; [research/06](research/06-tech-stack-options.md):89, 383, 402 |
| CPU/GPU equivalence tests (checkasm-style) and the SIMD/assembly performance ladder not started | Missing | Needed before trusting a GPU backend or hand-tuned kernels | [SCOPE.md](SCOPE.md):328, 352-361, 376 |

## 14. Testing and calibration

| Item | Tag | Impact | Source |
|---|---|---|---|
| Every model constant is a first guess; the `Calibration` struct has 8 sections (tone, color, local, dehaze, detail, effects, lens, negative) and none is fitted. Fitted values load with `amb render --calibration fitted.json`; defaults print with `amb calibration defaults` | Approx | The whole fidelity claim rests on fitting | `crates/amb-engine/src/calibration.rs`:1-23; `README.md` |
| The one-month Lightroom subscription has not been run; the sweep kit must be ready first ("Rule 1"). The full plan is 74 sweeps and about 680 exports per input image | Question | Source of all ground truth | [lightroom-trial-plan.md](lightroom-trial-plan.md):5, 39 |
| CC0 real raws (about 40-100 from raw.pixls.us: skin, sky, foliage, night, backlit, several brands) not collected | Missing | Real-image regression set | [lightroom-trial-plan.md](lightroom-trial-plan.md):12 |
| Interaction sidecars (Highlights x Whites, Shadows x Blacks, Contrast x Exposure, Vibrance x Saturation, Texture x Clarity) not in the sweep plan | Missing | Coupled controls cannot be fitted from single-slider sweeps | [lightroom-trial-plan.md](lightroom-trial-plan.md):14, 49 |
| Run sheet (labelling every export with Lightroom version, process version and settings) not written | Missing | Reference renders would be unlabelled | [lightroom-trial-plan.md](lightroom-trial-plan.md):16 |
| Fidelity metrics and CI not built: median and p95 dE00 per control, whole-preset scores, fidelity dashboard. Fidelity tiers are estimates until then | Missing | No measured preset fidelity can be published ("no overclaiming") | [SCOPE.md](SCOPE.md):237, 266, 295 |
| Preset fidelity suite and parser fuzz targets do not exist | Missing | Untrusted-input safety and regression coverage | [SCOPE.md](SCOPE.md):351, 380 |
| Synthetic ColorChecker is approximate (commonly published 8-bit sRGB values), not a measured reference | Limit | Colour fitting needs a real chart capture | `crates/amb-testkit/src/patterns.rs`:147-149; [research/05](research/05-processing-algorithms.md) §18.2 |

### Needs Lightroom measurement

Items that can only be settled with a Lightroom subscription. They are batched in the [trial plan](lightroom-trial-plan.md). Lens, negative and Amount questions are in sections 8, 9 and 10; they are not repeated here.

| Item | Tag | Impact | Source |
|---|---|---|---|
| Range, default and step size of every slider, and double-click reset behaviour ("Adobe's docs leave most of these out") | Question | Registry ranges and defaults are unverified | [lightroom-trial-plan.md](lightroom-trial-plan.md):34; [research/05](research/05-processing-algorithms.md) |
| Exact Lightroom Classic and Camera Raw versions and the current process version | Question | Everything fitted is tied to a version | [lightroom-trial-plan.md](lightroom-trial-plan.md):32 |
| What "Auto" and Lightroom's default settings write to XMP | Question | Auto-tone cannot be built without it | [lightroom-trial-plan.md](lightroom-trial-plan.md):35 |
| Slider sweeps for all global controls (74 sweeps) to fit the section 5 and 7 models; spatial controls on 1024 px crops | Question | Source data for all `Calibration` constants | [lightroom-trial-plan.md](lightroom-trial-plan.md):37-47 |
| Interaction pairs, run on synthetic and real raws | Question | Coupled-control fitting | [lightroom-trial-plan.md](lightroom-trial-plan.md):48-49 |
| Adobe profiles (Color, Standard, Neutral, Landscape, Portrait, Vivid, Camera Matching, creative profiles) at several Amounts, on ColorChecker and real raws | Question | Fits the replacement base profiles | [lightroom-trial-plan.md](lightroom-trial-plan.md):54 |
| Local masks: linear and radial gradients with known Feather on flat grey, luminance and colour range masks on ramps and wheels; each local slider at +50 against its global one; a turned radial and a brush stroke read back from the sidecar | Question | Fall-off curves, strengths and conventions are guesses ([design/masks.md](design/masks.md)) | [lightroom-trial-plan.md](lightroom-trial-plan.md):56 |
| Effects and detail: vignette in every style, grain, sharpening on edge patterns, NR on flat noise frames | Question | Fits section 7 | [lightroom-trial-plan.md](lightroom-trial-plan.md):57 |
| Upright modes on architecture shots; Transform behaviour | Question | Needed before Upright or Transform | [lightroom-trial-plan.md](lightroom-trial-plan.md):61 |
| Temp/Tint readouts for synthetic DNGs at known white points, and Adobe's extended range below 2000 K | Question | Exact interpolation details | `crates/amb-color/src/temperature.rs`:16-17 |
| Tone-curve domain (curves act on sRGB-encoded values today) and whether Whites/Blacks are global | Question | Curve and tone fidelity | `crates/amb-engine/src/pipeline.rs`:651; [research/05](research/05-processing-algorithms.md) §20 |

## 15. Platforms and packaging

| Item | Tag | Impact | Source |
|---|---|---|---|
| No installers or release builds: Flatpak/AppImage, MSI/MSIX (SignPath; x64 and ARM64), notarised universal DMG. CI only runs fmt, clippy and tests | Missing | Nobody outside the repo can install it | [SCOPE.md](SCOPE.md):308; `.github/workflows/ci.yml` |
| Windows, macOS, Linux (Wayland and X11), Windows on ARM64 and Apple Silicon must all be first-class, but today only the CLI is built and tested on four CI runners | Limit | No GUI viewport, GPU or display-profile behaviour has been tried per OS | [ARCHITECTURE.md](ARCHITECTURE.md):13; [SCOPE.md](SCOPE.md):50, 350 |
| Wayland colour management is a staging protocol (v3); display-profile handling on Linux is unproven | Question | Colour-managed display on Linux | [research/08](research/08-cross-platform-and-mobile.md):27 |
| Plugins exist as sandboxed WebAssembly "action" plugins that return edit commands (settings in, commands out). Not yet: plugins that add pixel effects to the pipeline or their own panels and dialogs, batch runs over many photos, plugins in the hub's in-app browser (they're shared as pull requests to `plugins/` and installed from a folder), or a CI build that rebuilds hub plugins from source. Recipes are per plugin only (not shared through the hub). The Film recipe looks are our own first guesses, not fitted to anything; the plan to measure them against a Fujifilm camera is [design/film-look-measurement.md](design/film-look-measurement.md). Saved presets have no rename or delete in the app yet (use the presets folder). Lightroom Lua SDK plug-ins are a non-goal | Partial | Third-party extension | [ai/plugins.md](ai/plugins.md); [design/customization-and-hub.md](design/customization-and-hub.md) §5 |
| Mobile is out of scope (an Android app may follow); an iOS App Store version is effectively impossible under the GPL | Limit | Deliberate scope choice | [SCOPE.md](SCOPE.md):51, 207 |

## Top 10 to close next

Chosen by impact on the main goal: faithful Lightroom preset compatibility, free, cross-platform.

1. **Run the Lightroom measurement month and fit the `Calibration` constants.** Every one of the 120 rendered Camera Raw tags is "approx"; nothing has been fitted, so no fidelity claim can be made. Finish the prerequisites first (interaction sidecars, run sheet, CC0 raws, `dng_validate` check).
2. **Replace the placeholder base look and add camera profiles.** Every preset renders through the profile, and the placeholder ACES-fit curve is not Adobe's; read the user's own DCPs in place, fit substitute profiles, and decode `Look` LUTs.
3. ~~**Local, edge-aware Highlights and Shadows.**~~ Done 2026-10-08; their strengths still need fitting (item 1).
4. ~~**Masks and local adjustments (linear, radial, brush, range).**~~ Done 2026-10-08; their fall-offs and strengths still need fitting (item 1).
5. **Transform/Upright (numeric `Perspective*` first)**, and verifying the crop against Lightroom. Presets and sidecars that carry perspective render unleveled.
6. **Finish the single-photo GUI editor.** Zoom/1:1, point curve, colour wheels, WB picker, clipping, the crop tool, layouts/themes and editor mode are done; installers remain, and test the cross-platform promise.
7. **Fix the preset path: measure Amount, honour `Enable*` toggles, handle PV2003/PV2010 tags.** These decide whether purchased presets apply correctly; run `amb preset census` on real collections to rank what else matters.
8. **Calibration panel and Point Color (8 planned tags; the B&W mixer is done).** Frequent in colour-grading and monochrome presets, and cheap compared with masks.
9. **Clear the pre-public checklist: add `LICENSE`, paraphrase `docs/adobe-docs/`, pick the name.** The repo cannot go public, and a rename gets harder after launch.
10. **A GPU backend with tiling and caching for an interactive preview path.** A full render is 3.5-4 s on 4 cores; the GUI needs interactive latency, which is Lightroom's top complaint.
