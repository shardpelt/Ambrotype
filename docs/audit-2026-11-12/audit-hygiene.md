# Ambrotype hygiene audit (2026-11-12, HEAD 87a74ab)

Read-only. No cargo run. Helper scripts live next to this file in the scratchpad (sz.py, uw.py, pub.py, dead.py, cm.out).
Measuring notes: "production lines" = lines before first column-0 `#[cfg(test)]` (same rule as `code_size.rs`). Function length = brace matching, as in `tools/code-metrics.py` (so `impl` fns, closures inside fns count in the outer fn).

## A. Metrics

`python3 tools/code-metrics.py` headline:
- Rust 295 files, 108,298 lines (structure.md 2026-11-09: 235 files / 103,928). JS 92 files, 25,373 (was 67 / 23,259). HTML 2,058 (was 1,915). CSS 1,693. WGSL 2,277. Markdown 13,000 lines.
- Largest crates: amb-engine 33,245; amb-gui 18,377; amb-protocol 8,506; amb-cli 7,582; amb-library 7,316; amb-export 7,083.
- Rust fns over 50 lines: 278 (of 2610); over 100: 70; over 150: 30; over 300: 4. JS fns over 80: 30 (of 1052).
- UI top-level names: 1,199 (was 1,085). `core.js` is used by 83 other files.
- Over-800 list (all 8 are the OVER_BUDGET entries): retouch 979, sidecar 960, embed 932, object 925, matching 908, pipeline/settings 906, amb-ai/lib 885, gpu/nn 847.
- UI scripts over 700: none. Largest JS: browse.js 593, spots.js 582, library.js 554, crop.js 534, preview.js 532.

### Rust files >= 680 production lines (budget 800) and the seam

| Lines (prod/total) | File | Natural seam |
|---|---|---|
| 979/1254 | amb-engine/src/retouch.rs | Spots model + `place`/`search_offset`/`heal_one`/`push_pull` (clone/heal math) vs `fill_one`/`INPAINTER`/`ring_grain` (AI inpaint, ~L479-715) vs `find_source`/`footprints` (donor search). Make `retouch/{mod,heal,inpaint,source}.rs` |
| 960/1318 | amb-protocol/src/sidecar.rs | `read_info`(L264-418, 153 lines) + `PhotoInfo` + parse_* (reading) vs `merge`/`apply_info`/`escape_text`/`element_name` (writing, L418+). `sidecar/{read,write}.rs` |
| 932/1107 | amb-export/src/embed.rs | One section per container: jpeg+iptc, png, tiff, webp, avif (Bmff/iloc/av1 bits, L322-890), jxl. Move avif+jxl (~L322-917) to `embed/avif.rs`, `embed/jxl.rs` |
| 925/1088 | amb-ops/src/object.rs | `select`/`select_words`(197 lines) + overlap/bounds/shadow_of (selection) vs `impl Selection` + `dabs_inside`/`fill*`/`cover` (turning a selection into a Remove spot, L556+). `object/{select,words,remove}.rs` |
| 908/1085 | amb-engine/src/matching.rs | Stats (`LookStats`, `look_stats`, `distance*`, L31-285) vs solver (`Param`, `Start`, `solve_linear`, `solve`, `solve_stage`, L375-end). `matching/{stats,solve}.rs` |
| 906/906 | amb-engine/src/pipeline/settings.rs | `RENDERED_TAGS` table (L195-376, ~180 lines of data) + `RENDERED_DEFAULTS`; `impl DevelopSettings::from_settings` (216 lines, L516+); crossover helpers. `settings/{tags,from_settings}.rs` |
| 885/1035 | amb-ai/src/lib.rs | Specs/types (`ModelSpec`, `Input`, `Output`, L73-260) vs catalogue/install/download (L262-700) vs run (`try_model` L841). `spec.rs`, `install.rs` |
| 847/1011 | amb-gpu/src/nn.rs | Tensor/op enums (L19-245) vs `Kernels` pipeline table (L245-343) vs `impl Session` (L385-824, ~440 lines: one method per op). `nn/{ops,kernels,session}.rs` |
| 798/905 | amb-engine/src/raw.rs | Load/decode (L150-450) vs metadata (`read_metadata`, `calibration_matrices`, L78-150, 554-700) vs embedded preview/thumbnail (L719-800). `raw/{metadata,preview}.rs`. 2 lines under budget |
| 791/933 | amb-library/src/catalog.rs | `impl Catalog` is L263-649 (390 lines) then prints (L649-715) and vectors (L715-770). Move `Print`/vector storage and `Query`/`Entry` types out. 9 lines under budget |
| 788/1400 | amb-engine/src/dehaze.rs | 40+ lines of module doc, estimate (airlight, haze map) vs guided-filter helpers (`lerp_position*`, `transpose`, L655+). Helpers to `detail/util.rs` (see task 4) |
| 759/1002 | amb-engine/src/profile.rs | "reading" (L67-460: `Reader`, tag consts) vs "applying" (L461+: HSV tables, tone curve). Two section banners already mark it. `profile/{read,apply}.rs` |
| 753/833 | amb-cli/src/mcp.rs | Protocol (`serve`, `handle`, L32-165) vs tool table (`tools`, 135 lines) vs tool bodies (`call`..`set_info`, L319-731). `mcp/{proto,tools,calls}.rs` |
| 752/1097 | amb-export/src/lib.rs | Types (`Format`, `ExportSettings`, L45-235) vs `export`/`export_one` (275-line fn) and colour/dither helpers. `settings.rs`, `encode.rs` |
| 744/1046 | amb-library/src/lrcat.rs | Reading (`read`, `read_collections`, SQL helpers, L31-363) vs planning (`plan*`, L363-576) vs apply + `write_sample` (L576+). `lrcat/{read,plan,apply}.rs` |
| 716/841 | amb-plugin/src/fuji.rs | `Sensor` + `camera` table (L21-142) + `LOOKS` data (L156-203) vs `parse` (180 lines) vs `tone_curve`/`white_balance`/`limits` mapping (L443-684). `fuji/{data,parse,map}.rs` |
| 714/717 | amb-engine/src/pipeline/mod.rs | `develop` is a 265-line fn (leave: "the order is the point", per structure.md); split helpers (`srgb_encode` L65, camera stage) out |
| 710/710 | amb-cli/src/organise.rs | One section per subcommand (rename/cull/sort/tag); `tag` is 143 lines. Split by subcommand |
| 702/877 | amb-engine/src/upright.rs | Line detection vs solve vs warp (check section comments before cutting) |
| 699/699 | amb-cli/src/library.rs | `catalog` (130 lines) and other subcommands; split by subcommand like organise.rs |
| 688/707 | amb-lens/src/lcp.rs | XML read vs model build; only 112 lines below budget; low priority |
| 684/684 | amb-gui/src/looks/check.rs | Per-item-kind validators (layout/theme/...); `panel` is 132 lines. By kind |
| 684/705 | amb-lens/src/lensfun.rs | DB load vs lookup; low priority |

Rust files 680-800 not already on OVER_BUDGET: 16 (raw.rs, catalog.rs, dehaze.rs, profile.rs, mcp.rs, export/lib.rs, lrcat.rs, fuji.rs, pipeline/mod.rs, organise.rs, upright.rs, cli/library.rs, lcp.rs, lensfun.rs, looks/check.rs; plus 3 within 20 lines of 800: raw.rs, catalog.rs, dehaze.rs). The next three will trip `code_size.rs` on the next feature.

### JS files >= 600 lines (budget 700)
None. Largest is `library/browse.js` 593 (seam: map code `drawMap` L349+ vs browse list), `tools/spots.js` 582 (`wireSpots` 131 lines). Watch these two.

### code_size.rs allow-list
`crates/amb-cli/tests/code_size.rs` has `OVER_BUDGET` with 8 entries: retouch.rs 1100, sidecar.rs 1100, embed.rs 1100, object.rs 1100, matching.rs 1100, pipeline/settings.rs 1100, gpu/nn.rs 1000, ai/lib.rs 1000. All are limits well above current sizes (979/960/932/925/908/906/847/885 production lines), so each file can still grow 100-250 lines silently. Limit was "size on 2026-11-09 rounded up to next hundred plus a hundred". Suggest tightening each to current size rounded up to 50 (see Fix task 2). No JS exceptions. The test also fails if a listed file drops under budget (good: forces removal).

## B. cleanup.md status (every task verified in code)

Result: 27 tasks (G1-G3, S1-S4, F1-F16, R1-R4). Tally: **DONE 26, PARTLY 1 (S4), OPEN 0, OBSOLETE 0**. Nothing is obsolete: every named thing exists in its moved form. cleanup.md itself says "Do them in order" and is fully ticked; it has no remaining open task. It is out of date in one sense: structure.md's numbers (see end of section D).

| Task | Status | Evidence |
|---|---|---|
| G1 UI scripts loaded once test | DONE | `every_ui_script_is_loaded_once` at crates/amb-gui/src/main.rs:298 |
| G2 size budget test | DONE | crates/amb-cli/tests/code_size.rs exists, 8 allow-list entries |
| G3 code-metrics.py | DONE | tools/code-metrics.py runs; prints all sections |
| S1 one smoothstep | DONE | only `amb-engine/src/math.rs:7` (f32) and `:17` (f64); the wgsl copy is the GPU's own |
| S2 one file_stamp | DONE | catalog.rs:16 `use amb_presets::linked::file_stamp`; only other `file_stamp` is gui/folder.rs:157 (different types, as the task says) |
| S3 core.js | DONE | ui/core/core.js defines `invoke` L9, `ui` L12, `toast` L44, `store` L93, `fileName` L128; 83 files use it |
| S4 store for localStorage | PARTLY | `store` used 65 times, but 8 raw `localStorage` calls remain outside core.js: looks/columns.js:214,229,231; library/slideshow.js:27; library/library.js:105,113,119; help/tips.js:118 (nested-map shapes the task said to leave). Also 6 `JSON.parse(localStorage` |
| F1 gui main.rs | DONE | main.rs 382 lines; config.rs, state.rs, commands/mod.rs (handler) exist |
| F2 session/ | DONE | session/{mod,context,render,retouch,tools,tests}.rs |
| F3 commands/photo, library | DONE | commands/ has 22 files; photo.rs 420 lines (was 956) |
| F4 looks/ | DONE | looks/{mod,check,setups,tests}.rs |
| F5 hub/ | DONE | hub/{mod,check,kinds,submit,tests}.rs |
| F6 state/ | DONE | amb-protocol/src/state/{mod,files,plan,query,snapshots}.rs; no state.rs |
| F7 mask/ | DONE | amb-engine/src/mask/{mod,apply,geometry,sky,tests}.rs; no mask.rs |
| F8 negative/ | DONE | negative/{mod,analysis,cast,frame,tests}.rs |
| F9 library.js | DONE | library/ has 21 scripts; library.js 554 lines (was 1488) |
| F10 collage.js | DONE | collage/ 4 files; collage.js 346 |
| F11 app.js | DONE | core/app.js 480 (was 1188) |
| F12 masks.js | DONE | tools/{masks,mask-draw,mask-panel}.js; masks.js 524 |
| F13 layout.js, editor.js | DONE | looks/layout.js 433, editor.js 500 (+ editor-keys, editor-inspector) |
| F14 protocol masks/ | DONE | masks/{mod,describe,fields,tests}.rs |
| F15 amb-ai net/ | DONE | net/{mod,plan,run,tests}.rs |
| F16 engine lens/ | DONE | lens/{mod,ca,camera,defringe,tests}.rs |
| R1 amb-ops | DONE | crates/amb-ops exists (9 modules + denoise/cleanup since); amb-library/Cargo.toml has no amb-gpu |
| R2 Prefs one lock | DONE | state.rs:14 `prefs: Mutex<Config>`; 11 other Mutexes remain but are not saved settings |
| R3 UI folders | DONE | ui/{core,looks,develop,tools,library,collage,dialogs,help} |
| R4 docs one home | DONE | docs/NEXT.md:9 single "Built" bullet |

New since: cli `main.rs` shrank (usage.rs split, 653 lines) but `main` is still a 434-line match (structure.md says leave it).

## C. Hygiene sweep

### C1. unwrap/expect in non-test Rust (crates/*/src, before first `#[cfg(test)]`)
Total 62, zero `.unwrap()` on I/O or locks except raw.rs:530. Per crate: amb-engine 16, amb-gui 11, amb-ai 8, amb-presets 6, amb-protocol 4, amb-gpu 3, amb-ops 3, amb-cli 3, amb-library 3, amb-export 3, amb-testkit 2, others 0. Almost all are `.expect("reason")` on invariants, builtin JSON, or fresh mutexes. Riskiest 20 (by input exposure; file:line):
1. amb-engine/src/raw.rs:530 bare `.unwrap()` (only bare unwrap; on a raw's data)
2. amb-cli/src/mcp.rs:444 `serde_json::from_value(query).expect("valid query")` (query comes from an MCP client: panics the server on bad input)
3. amb-ops/src/convert.rs:136 `results.lock().expect("results")` inside rayon (a panicking worker poisons it; then 143 `into_inner().expect`)
4. amb-ops/src/convert.rs:143
5. amb-engine/src/plan.rs:321 `d.lock().expect("masks")` in render worker
6. amb-engine/src/plan.rs:522 same lock
7. amb-protocol/src/state/snapshots.rs:62 `s.expect("checked above")` on user-named snapshot
8. amb-protocol/src/state/query.rs:120 `as_object_mut().expect("object")`
9. amb-presets/src/xmp.rs:124,127,135,140,151 stack pops in the XML parser on foreign XMP (`"root"`, `"balanced"`); safe only if quick-xml guarantees balance. Five sites
10. amb-engine/src/pano/align.rs:191,221,267,283 (`"at least one pair"`, `"candidates"`, `"a link is a pair"`, `"placed"`) on user photos in a stitch
11. amb-engine/src/merge.rs:417 `out.last().expect("level 0")`
12. amb-engine/src/pipeline/mod.rs:254 `.expect("the conversion is on")`
13. amb-engine/src/mask/mod.rs:110 `local_slider(tag).expect("known slider")` (tag from a sidecar)
14. amb-ai/src/search.rs:74 `search.as_ref().expect("checked in load")`
15. amb-ai/src/objects.rs:66 and faces.rs:55 `guard.as_mut()/as_ref().expect("loaded above")` (guard may be re-locked between)
16. amb-gui/src/session/render.rs:122,234,338 `expect("set above")`
17. amb-gpu/src/device.rs:173 `.expect("the GPU stopped responding")` (device loss panics the editor)
18. amb-engine/src/profile.rs:345 `illuminants.last().expect("one")` (loaded .dcp with 0 illuminants?)
19. amb-library/src/catalog.rs:275 `open_in_memory().expect(...)` (OK; test-only helper outside cfg(test)?)
20. amb-gui/src/main.rs:119,122,140 and config.rs:27 `lock().expect("fresh mutex")` (startup, fine) and main.rs:226 (Tauri start; fine)

Low risk (builtin JSON / invariants): registry.rs:66, protocol/catalog.rs:92,95, gui sizes.rs:54, borders.rs:45, export/collage.rs:491, ai/lib.rs:269, plus `"some name is free"` x4 (engine/lib.rs:151, library/smart.rs:309, collections.rs:189, gui/session/mod.rs:287).

### C2. `#[allow(`: 16 total, no `dead_code`, none unexplained by lint kind
- clippy::too_many_arguments x10: gpu/local.rs:135, engine/xtrans.rs:176, engine/rcd.rs:312, engine/detail/util.rs:26, engine/lens/ca.rs:78, engine/lens/camera.rs:25 and :42, engine/mask/apply.rs:49, gui/session/render.rs:95 and :197, export/lib.rs:675 (11 sites)
- clippy::type_complexity x3: engine/retouch.rs:829, cli/render.rs:137 and :183
- clippy::needless_range_loop x2: engine/pano/linalg.rs:6 (whole file `#![allow]`), engine/pano/render.rs:448

### C3. `pub` items used only inside their own crate (candidates; checked as words across the other crates, tests included; also check own `tests/` before narrowing)
amb-ops (19): dust.rs:52 `spots_for`; dust.rs:99 `spot_command`; merge.rs:40 `default_output`; merge.rs:45 `pano_output`; denoise.rs:13 `default_output`; cleanup.rs:54 `Reason`; cleanup.rs:121 `Seen`; snap.rs:84 `box_mean`; burst.rs:30 `Ranked`; burst.rs:161 `openness`; convert.rs:36 `CAMERA_JPEG_EDGE`; convert.rs:41 `AT_ONCE`; negative.rs:17 `analyze_file`; negative.rs:150 `SCENE_GAP`; negative.rs:153 `ODD_GAP`; negative.rs:157 `RollFrame`.
amb-ai (5): search.rs:23 `SearchSpec`; search.rs:177 `similarity`; lib.rs:147 `ModelSpec`; lib.rs:199 `ModelFile`; lib.rs:841 `try_model`.
amb-library (18): gpx.rs:16 `MAX_GAP`; gpx.rs:20 `Track`; gpx.rs:75 `locate`; folder.rs:11 `is_raw`; duplicates.rs:278 `dhash`; places.rs:38 `LOOK_ALIKE`; lensdb.rs:16 `SERVER`; lensdb.rs:18 `VERSION`; catalog.rs:337 `records_counted`; catalog.rs:693 `store_prints`; catalog.rs:749 `store_vectors`; lrcat.rs:47 `LrCollection`; lrcat.rs:56 `Contents`; lrcat.rs:371 `PlannedPhoto`; lrcat.rs:387 `PlannedCollection`; lrcat.rs:415 `plan_contents`; insights/mod.rs:117 `bucket_of`; insights/mod.rs:140 `Tally`.
Total 39 hits in these three crates. Caveat: types like `ModelSpec`, `Track`, `Contents`, `Seen`, `Ranked` are reachable through public signatures (private-in-public would fail to compile), so keep those `pub`; the clean ones to narrow to `pub(crate)` are the fns/consts: spots_for, spot_command, the three `default_output`/`pano_output`, box_mean, openness, CAMERA_JPEG_EDGE, AT_ONCE, analyze_file, SCENE_GAP, ODD_GAP, similarity, try_model, MAX_GAP, locate, is_raw, dhash, LOOK_ALIKE, lensdb SERVER/VERSION, records_counted, store_prints, store_vectors, plan_contents, bucket_of. Check each against the crate's own `tests/` first.

### C4. Longest functions (brace-matched, `tools/code-metrics.py` method, limit raised to 40/30)
Rust (top 30): cli/main.rs:89 `main` 434; engine/xtrans.rs:177 `run` 343; gui/schema.rs:154 `panels` 311; engine/plan.rs:198 `plan_with` 310; export/lib.rs:382 `export_one` 275; gpu/develop.rs:235 `render` 273; engine/pipeline/mod.rs:449 `develop` 265; ai/net/plan.rs:81 `step` 250; gui/commands/mod.rs:29 `handler` 244; protocol/state/plan.rs:7 `plan` 237; engine/rcd.rs:72 `band` 236; engine/pipeline/settings.rs:516 `from_settings` 216; engine/lens/camera.rs:43 `apply_camera_window` 215; ai/net/plan.rs:433 `matmul` 199; ops/object.rs:248 `select_words` 197; cli/render.rs:336 `export_settings` 187; plugin/manifest.rs:217 `validate` 181; plugin/fuji.rs:260 `parse` 180; plugins/film-recipe/lib.rs:43 `look` 167; protocol/sidecar.rs:492 `apply_info` 166; gui/main.rs:85 `main` 165; protocol/masks/mod.rs:221 `paint` 164; engine/retouch.rs:266 `search_offset` 160; plugins/film-recipe/lib.rs:243 `run` 158; engine/mask/geometry.rs:109 `draw_shape` 158; cli/lens.rs:91 `lens_match_output` 158; engine/pano/align.rs:163 `align` 157; protocol/sidecar.rs:264 `read_info` 153; export/exif.rs:252 `export_dirs` 153; engine/merge.rs:114 `hdr` 152.
JS (top 30): tools/crop.js:124 `create` 408; develop/curve.js:124 `create` 364; looks/looks-dialog.js:8 `looksFlow` 288; core/keys.js:258 `settingsFlow` 264; develop/wheels.js:82 `makeWheel` 225; tools/masks.js:217 `wireMasks` 209; dialogs/hub.js:177 `hubFlow` 176; core/app.js:307 `init` 172; library/library.js:394 `wireLibrary` 161; looks/layout.js:252 `buildPanel` 157; looks/editor-inspector.js:130 `openInspector` 147; core/picker.js:17 `enhance` 133; tools/spots.js:320 `wireSpots` 131; develop/fx.js:90 `makeEffectsControl` 126; tools/mask-panel.js:124 `renderMaskDetail` 125; tools/mask-draw.js:92 `drawMasks` 114; dialogs/hub.js:186 `render` 108; help/tips.js:43 `showTip` 106; develop/frame.js:242 `wireFrame` 103; tools/crop.js:191 `draw` 99; collage/collage-free.js:197 `wireStage` 99; help/camsim.js:37 `camScene` 94; tools/select.js:307 `wireSelect` 93; tools/croptool.js:333 `sizesFlow` 88; looks/layout.js:52 `applyLayout` 88; develop/controls.js:68 `knob` 88; tools/mask-panel.js:36 `renderMaskPanel` 87; library/browse.js:349 `drawMap` 86; dialogs/borders.js:233 `wireBorders` 82; dialogs/export.js:363 `exportFlow` 81.
Note: structure.md already blessed the demosaic, pipeline-order and cli `main` ones as "leave". The ones it called splittable and which are still long: `export_one` 275, `schema::panels` 311, `state::plan` 237, `net::step` 250, JS `create` x2, `looksFlow`, `settingsFlow`, `makeWheel`, `wireMasks`.

### C5. Magic numbers in the last 6 commits (non-test code)
- amb-ops/src/cleanup.rs:134,136 `v < 16`, `v > 245` (black/white frame thresholds, no const); :253 `s.black > 0.9, s.white > 0.6` (asymmetric thresholds, no names, no comment on why 0.6); :42 `burst_seconds: 2.0` default (inline in `Default`; other tuning consts there are named: BLURRY, SOFT_SHARE, SHOOT).
- plugins/kaleidoscope/src/lib.rs:78 `p[0] * 1.7 - 0.55` (fractal fold scale/offset); :37 `clamp(2.0, 32.0)`, :39 `.max(0.05)`, :40 `clamp(1.0, 8.0)` (limits duplicate the manifest ranges, so can drift).
- amb-ai/src/denoise.rs:94 `clamp(1e-3, 1.0)` (feather floor); :155 `mean.abs() > 1e-12`.
- amb-engine/src/raw_denoise.rs:155 `PROPHOTO_TO_REC2020` matrix is a named const (fine) but hard-codes Bradford numbers, and amb-color already has matrix helpers (invert3/mul3), so it is a candidate to derive from amb-color's primaries.
- amb-ops/src/denoise.rs:34 `AMOUNT: f32 = 70.0` is named (good); :58 sets `LuminanceSmoothing` 0.0 as a literal.
- amb-ai/src/archive.rs:53 `take(65_557)` and zip offsets `+10 +16 +20 +24 +28 +30 +32 +42 +46`: file-format offsets, commented-by-name would help but acceptable.
- heat-camera palettes are data tables (fine).
Commits otherwise (man.rs, usage.rs, guides.rs, connect.rs, places.rs, JS dialogs) show no magic-number logic.

### C6. JS hygiene (crates/amb-gui/ui, 92 files incl. tests)
- Top-level `let` at column 0: 7 total, one each in browse.js, develop/split.js, develop/presets.js, develop/photo.js, core/search.js, core/core.js, core/app.js. (Shared state is mostly `const` objects: 1,199 top-level names, all classic-script globals; `every_ui_script_is_loaded_once` also checks each name is declared once.)
- Dead top-level functions: 0 (983 top-level `function`s, each referenced at least once beyond its definition across ui/, tests and Rust; also 0 unreferenced top-level `const`/`let`).
- `console.log`/`console.debug`: 0.
- Inline `style="` in index.html: 17 (widgets-demo.html: 0).
- Duplicate helpers: `fileName` now single (core.js:128), no `split("/").pop()` copies. Still: `const el = (tag, attrs, ...kids)` helper local to library/slideshow.js:302 (same idea as the 381 `Object.assign(document.createElement(` uses, which was 316 on 2026-11-09, so growing); `toast(String(e), true)` 166 times (was 157); 6 `JSON.parse(localStorage...` left. Exact function-body duplicates under different names: none found (hash of whitespace-stripped bodies > 25 chars).

### C7. Rust dead code
- `#[allow(dead_code)]` / `unused`: none.
- Files under src/ not declared as `mod`: none (checked every .rs under crates/*/src against `mod name` in sibling mod.rs/lib.rs/main.rs/parent file). One `include!` (gui/src/hub/kinds.rs:215, build-script output).
- Real duplicates in Rust worth merging: `srgb_encode` x4 (engine/detail/util.rs:290, profile.rs:463, local.rs:446, pipeline/mod.rs:65; differ in visibility only, verify bodies); `lerp_position` x2 identical (detail/util.rs:194 and dehaze.rs:666); `percentile` x4 (negative/analysis.rs:44, dust.rs:500, matching.rs:72, plugin-sdk) differ by design; `box_mean` in amb-ops/snap.rs:84 vs `filters::box_blur` in engine.
- `amb-lens` has `serde.workspace = true` but no code uses serde (C8).

### C8. Dependencies
Unused: only `serde` in crates/amb-lens/Cargo.toml:13 (no `serde` in its src). Everything else is referenced (word search of crate name with `-` to `_`, incl. dev/build deps, in src, tests, build.rs).
Not in `[workspace.dependencies]` (declared separately with versions):
- `serde` and `serde_json` in amb-ai as `"1"` literal (amb-ai/Cargo.toml:14-15) while 10 other crates use `.workspace = true`; the workspace table has only 4 entries (quick-xml, serde, serde_json, thiserror).
- `image`: amb-engine `0.25`, amb-export `0.25.10` with custom feature set, amb-library `0.25`, amb-ops `0.25`, amb-gui dev `0.25`. Versions differ in text, not in lock.
- `sha2 0.10` x4 (amb-ai, gui, library, plugin); `ureq 2` x3 (ai, gui, library, with `native-certs` in at least ai); `rayon 1` x6; `flate2 1.1` x2 (ai, export); `jpeg-encoder 0.6` x2 (export, gui).
No version conflicts, only repetition: a bump means 3-6 edits.

### C9. Tests
Rust: 60 files under crates/*/src with > 150 lines, no `#[cfg(test)]`, no `tests.rs` in their directory and no same-named file under the crate's `tests/`. Largest 30 (many are Tauri command wrappers and CLI subcommands covered only indirectly by the UI tests):
710 amb-cli/src/organise.rs; 638 amb-gui/src/commands/presets.rs; 634 commands/catalog.rs; 621 commands/batch.rs; 598 commands/export.rs; 542 amb-cli/src/render.rs; 528 amb-engine/src/plan.rs; 526 amb-engine/src/pano/align.rs; 463 amb-gpu/src/local.rs; 421 gui commands/photo.rs; 381 amb-protocol/src/state/plan.rs; 354 gui commands/plugins.rs; 353 amb-protocol/src/state/mod.rs; 340 gui commands/select.rs; 321 amb-cli/src/usage.rs; 295 gui commands/library.rs; 290 gui commands/collage.rs; 273 gui commands/mod.rs; 262 protocol/state/files.rs; 261 gui commands/files.rs; 252 gui commands/hub.rs; 249 amb-cli/src/lens.rs; 246 engine/pano/features.rs; 231 amb-gpu/src/denoise.rs; 216 gui commands/tools.rs; 214 amb-protocol/src/edit.rs; 207 gui commands/models.rs; 196 gui commands/appearance.rs; 195 amb-cli/src/models.rs; 189 gui/src/preset_files.rs.
Caveats: engine/plan.rs and protocol/state/{plan,files}.rs are exercised by crate-level integration tests (name not matching); `pipeline/settings.rs`, `cli/render.rs`, `pipeline/camera.rs` are named in tests/ files so excluded. New from recent commits with no unit tests: amb-ai/src/denoise.rs? (has cfg(test)), cli/usage.rs (320 lines, none), gui commands/places.rs (55, small).
UI dialogs: 43 `<dialog id="...-dialog">` in index.html; tests/ui has 3 scripts (browse.js, organise.js, smoke.js). 19 dialog ids are not mentioned in any: assistant-dialog, basics-dialog, hdr-dialog, hub-dialog, import-dialog, learn-dialog, lrcat-dialog, match-dialog, meta-dialog, name-dialog, pano-dialog, paste-dialog, plugins-dialog, settings-dialog, shortcuts-dialog, sizes-dialog, smart-dialog, surprise-dialog (+ 1 more counted by id text only; the test may open some through a function name, e.g. `denoiseFlow()`, `#lib-cleanup`, so confirm before adding). Newest dialogs (denoise, cleanup/`clean-dialog`, place picker) have steps in organise.js/browse.js.

### Doc drift (bonus)
docs/design/structure.md "Measured" table (235 Rust files, 103,928 lines; 63 UI scripts; 1,085 global names; 27 locks; 735 tests, 89 UI steps) is the 2026-11-09 snapshot; now 295 / 108,298 / 92 / 1,199. Its "Big files" list still names the pre-split files (session.rs 1,855, mask.rs 1,497, ...) that cleanup.md says are split. AGENTS.md does not list `tools/film-r.sh` (minor).

## Fix tasks

Ordered by value/risk. Each is one agent sitting. Always run the "Before you commit" trio for Rust tasks (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`) unless the task names a narrower scope; stage only your files; commit as `area: summary`.

1. **Drop the unused `serde` dep in amb-lens** (tier: haiku, risk: low)
   - Files: crates/amb-lens/Cargo.toml (line 13), Cargo.lock.
   - Steps: delete `serde.workspace = true`; `cargo build -p amb-lens`.
   - Verify: `cargo test -p amb-lens` and `cargo check --workspace`. If something derives serde via a re-export, the build fails: restore the line.

2. **Tighten OVER_BUDGET limits** (tier: haiku, risk: low)
   - Files: crates/amb-cli/tests/code_size.rs.
   - Steps: for each of the 8 entries set the limit to current production lines rounded up to the next 50 (retouch 1000, sidecar 1000, embed 950, object 950, matching 950, settings 950, nn 900, ai/lib 900). Update the doc comment above to say so.
   - Verify: `cargo test -p amb-cli --test code_size`.

3. **Dedup `lerp_position`, `srgb_encode` in the engine** (tier: sonnet, risk: low)
   - Files: crates/amb-engine/src/detail/util.rs, dehaze.rs, profile.rs, local.rs, pipeline/mod.rs, math.rs.
   - Steps: confirm bodies equal; keep one `lerp_position` (dehaze.rs:666 imports `detail::util::lerp_position`, make it `pub(crate)`); keep one `srgb_encode` in `math.rs` (pub), replace the four with `use crate::math::srgb_encode`. Do not touch the GPU side. Keep names public where they already were.
   - Verify: `cargo test -p amb-engine` and `cargo test -p amb-gpu` (GPU stages are checked against CPU).

4. **Narrow dead `pub` items in amb-ops, amb-ai, amb-library** (tier: sonnet, risk: low)
   - Files: the ~25 fns/consts listed in C3 (not the types).
   - Steps: change `pub` to `pub(crate)` per item; build after each crate; if another crate's `tests/` needs one, revert that item.
   - Verify: `cargo clippy --workspace --all-targets -- -D warnings` then `cargo test --workspace` (unused-item warnings will flag any that become dead).

5. **Split amb-engine `retouch.rs`** (tier: sonnet, risk: med)
   - Files: crates/amb-engine/src/retouch.rs to `retouch/{mod.rs,heal.rs,inpaint.rs,source.rs}`; code_size.rs.
   - Steps: follow docs/cleanup.md "Moving Rust code" (move items unchanged; only `mod`/`use`/visibility edits): heal (place/sample/search_offset/heal_one/correction/push_pull), inpaint (INPAINTER, set_inpainter, fill_one, ring_grain), source (find_source, find_spot_source, footprints). Remove the file from OVER_BUDGET.
   - Verify: full trio; `cargo test -p amb-engine` unchanged count; UI tests since spots are used there.

6. **Split `sidecar.rs` and `embed.rs`** (tier: sonnet, risk: med; two commits)
   - Files: amb-protocol/src/sidecar.rs to `sidecar/{mod,read,write}.rs`; amb-export/src/embed.rs to `embed/{mod,avif,jxl}.rs`; code_size.rs.
   - Steps: same move-don't-rewrite rules; keep all `pub` names re-exported from `mod.rs` so callers don't change.
   - Verify: full trio; embed has tests for each container, sidecar tests cover round trips.

7. **Split `matching.rs`, `object.rs`, `pipeline/settings.rs`** (tier: sonnet, risk: med; one commit each)
   - Files: see seam table; code_size.rs.
   - Steps: matching to `{stats,solve}`; object to `{select,words,remove}`; settings: move `RENDERED_TAGS`/`RENDERED_DEFAULTS` table to `settings/tags.rs`, `from_settings` stays.
   - Verify: full trio; `amb docs`/recipes tests (`cargo test -p amb-cli --test recipes`) if settings tags move.

8. **Split `amb-ai/src/lib.rs` and `amb-gpu/src/nn.rs`** (tier: sonnet, risk: med)
   - Files: amb-ai/src/lib.rs to `spec.rs`+`install.rs` (re-export from lib.rs); amb-gpu/src/nn.rs to `nn/{ops,kernels,session}.rs`; code_size.rs.
   - Steps: the `impl Session` block is 440 lines of one-method-per-op; it can stay as one file `session.rs` (~450 lines).
   - Verify: `cargo test -p amb-ai -p amb-gpu` (GPU tests skip without an adapter: note it), full clippy.

9. **Pre-empt the next three budget breaches (raw.rs 798, catalog.rs 791, dehaze.rs 788)** (tier: sonnet, risk: low-med)
   - Files: amb-engine/src/raw.rs to `raw/{mod,metadata,preview}.rs`; amb-library/src/catalog.rs to move `Print`/vector storage (L649-770) to `catalog/prints.rs`; dehaze.rs move guided-filter helpers.
   - Verify: full trio. Do after 3 (the dehaze helpers overlap).

10. **Finish the `store` migration in the UI** (tier: sonnet, risk: low-med)
    - Files: ui/looks/columns.js (214,229,231), library/slideshow.js:27, library/library.js (105,113,119), help/tips.js:118; core/core.js.
    - Steps: these read nested maps stored as one JSON key. Use `store.get("amb.tabs", {})`/`store.set(...)` with the same keys and the same JSON shape; change no key.
    - Verify: `cargo test -p amb-gui` (script-name tests) and the UI tests (`ambrotype --ui-test crates/amb-gui/tests/ui/smoke.js photo.dng`, per docs/recipes.md Xvfb); then `grep -rn localStorage crates/amb-gui/ui` shows only core.js.

11. **Replace the panicking `expect` on client input** (tier: haiku, risk: low)
    - Files: crates/amb-cli/src/mcp.rs:444 (`from_value(query).expect("valid query")` should return the JSON-RPC error text like its neighbours); engine/raw.rs:530 bare `.unwrap()`; amb-protocol/state/snapshots.rs:62.
    - Steps: turn each into `?`/`map_err(|e| e.to_string())` in a function that already returns Result; for snapshots, restructure with `let Some(s) = ... else`.
    - Verify: `cargo test -p amb-cli -p amb-protocol -p amb-engine`.

12. **Name the cleanup and kaleidoscope thresholds** (tier: haiku, risk: low)
    - Files: crates/amb-ops/src/cleanup.rs:134,136,253; plugins/kaleidoscope/src/lib.rs:37-40,78.
    - Steps: add consts `DARK_LEVEL: u8 = 16`, `BRIGHT_LEVEL: u8 = 245`, `DARK_SHARE: f32 = 0.9`, `BRIGHT_SHARE: f32 = 0.6` with one-line comments; in the plugin name the fold scale/offset (1.7, 0.55). Values unchanged. If the plugin's `plugin.wasm` is rebuilt, run `amb plugin build plugins/kaleidoscope` and commit its wasm.
    - Verify: `cargo test -p amb-ops -p amb-plugin`.

13. **Move shared dep versions to `[workspace.dependencies]`** (tier: haiku, risk: low)
    - Files: Cargo.toml (root), crates/*/Cargo.toml for amb-ai, amb-engine, amb-export, amb-gui, amb-library, amb-ops, amb-plugin.
    - Steps: add `sha2`, `ureq`, `rayon`, `image`, `flate2`, `jpeg-encoder` (keep export's `0.25.10` and feature lists per crate: use `workspace = true` plus `features = [...]` where they differ; set `default-features = false` only where it is today). Switch amb-ai's `serde`/`serde_json` to `.workspace = true`.
    - Verify: `cargo metadata` unchanged lock (`git diff Cargo.lock` empty or trivial), full trio.

14. **Add UI test steps for untested dialogs** (tier: sonnet, risk: low)
    - Files: crates/amb-gui/tests/ui/smoke.js (or a new `dialogs.js`).
    - Steps: for each id in C9's list, `open` it by its button/shortcut, assert `.open`, close with Escape; skip dialogs that need a network or a file. Follow the helper style in organise.js.
    - Verify: `ambrotype --ui-test crates/amb-gui/tests/ui/<script>.js photo.dng` under Xvfb (docs/recipes.md).

15. **Split `export_one`, `schema::panels`, `state::plan`, `net::step` by case** (tier: opus, risk: high)
    - Files: amb-export/src/lib.rs:382, amb-gui/src/schema.rs:154, amb-protocol/src/state/plan.rs:7, amb-ai/src/net/plan.rs:81.
    - Needs a design decision first: where case boundaries go (per format, per panel group, per command, per op) and whether the existing helper signatures stay `pub(super)`. These are long matches whose order matters; a mistake changes output silently.
    - Verify: full trio plus UI tests; export tests compare bytes.

16. **Split the big UI factories (`crop.js create`, `curve.js create`, `looksFlow`, `settingsFlow`, `makeWheel`, `wireMasks`)** (tier: opus, risk: high)
    - Files: ui/tools/crop.js, develop/curve.js, looks/looks-dialog.js, core/keys.js, develop/wheels.js, tools/masks.js.
    - Needs a design decision: closures share local state (draw/pointer handlers); extracting means choosing an explicit state object and keeping the 1,199 global names unique (`every_ui_script_is_loaded_once` enforces).
    - Verify: `cargo test -p amb-gui` and all three UI test scripts by hand-driving the editor (no UI tests cover crop/curve drag).

17. **Refresh docs/design/structure.md numbers and big-file list** (tier: haiku, risk: low)
    - Files: docs/design/structure.md.
    - Steps: re-run `python3 tools/code-metrics.py`, update the Measured table, crate table line counts, drop the pre-split "Big files" rows that cleanup.md marks done and list the 8 current ones; set the date.
    - Verify: `cargo test -p amb-cli --test recipes` (checks doc links).
