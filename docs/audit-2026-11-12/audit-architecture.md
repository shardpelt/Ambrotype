# Ambrotype architecture audit (read-only, 2026-11-12)

Method: `git show --stat` plus diffs of the six named commits, greps over crates/ and ui/, Cargo.toml path deps, `git log -30 --name-only`. No cargo run, no repo edits. Counts are from the working tree at HEAD (87a74ab).

## A. Touchpoints per feature

### A1. Registration-only files per commit
(Feature-specific files left out. "Reg" = a name/line added to a list, arm, menu, wire call, docs row.)

| Commit | Registration-only edits |
|---|---|
| 2ca6e18 Clean up (17 files) | `amb-cli/src/main.rs` (`mod cleanup;`, USAGE lines, `"--trash"` into the bool-flag list, `"--skip"` into the value-flag list, match arm); `amb-gui/src/commands/mod.rs` (`mod` is not needed; handler entry `batch::library_cleanup`); `amb-ops/src/lib.rs` (`pub mod cleanup`); `ui/core/keys.js` (KEY_ACTIONS entry); `ui/library/library.js` (`wireCleanup()`); `ui/index.html` (dialog + `<script src>`); `ui/style.css` (26 lines); `docs/man/amb.1` (3 lines, generated, test-checked); `docs/ARCHITECTURE.md` (file list in an `amb-ops` cell); `docs/FEATURES.md` row; `docs/GAPS.md` (37-line diff: renumbering); `docs/WHATS-NEXT.md` (move to Recently done); `tests/ui/organise.js` step. **11 registration files**, 5 of them docs. |
| 3ba2e00 agent guides (21 files) | `main.rs` (mod, USAGE, arm); `commands/mod.rs` (2 handler entries); `keys.js` (1 new action + 1 edited); `index.html` (select, button, new dialog); `style.css`; `amb-protocol/src/lib.rs` (`pub mod guides`); `guides.rs` (GUIDES list, tested); `docs/man/amb.1`; `AGENTS.md`; `docs/ai/README.md`, `docs/ai/mcp.md` (two copies of the "list of guides" prose); `ARCHITECTURE.md`; `FEATURES.md` (2 rows); `GAPS.md` (35); `WHATS-NEXT.md`; `ui/ai-apps.json`; `tests/ui/organise.js`. `mcp.rs` resource list had to be edited by hand. **13 registration files**. |
| b66b07a kaleidoscope + heat-camera (26 files) | root `Cargo.toml` `members` (one long line, 2 plugins added); `Cargo.lock`; `amb-plugin/Cargo.toml` (2 path deps); `amb-plugin/src/lib.rs` `BUILTIN` (10 to 12, `include_str!`/`include_bytes!` x2); `amb-plugin/src/native.rs` (2 match arms); `plugins/trippy/plugin.json` (5 tiles); `ui/dialogs/plugins.js` `PLUGINS_ELSEWHERE`; `ui/tools/trip.js` `TRIP_PLUGINS` + the trip roll; `tests/ui/smoke.js` (undo count 5 to 7: a magic number that breaks whenever a step is added); `FEATURES.md`; `GAPS.md` (33); `WHATS-NEXT.md`; `docs/ai/plugins.md`; committed `plugin.wasm` x2 (230 KB binary, rebuilt by hand). **10 registration files** + 2 binaries. |
| 87a74ab AI Denoise (27 files) | `amb-ai/src/lib.rs` `TASKS` (6 to 7, array length literal), `models.json`; a task-specific branch in the models test (`else if s.task == "denoise"`); `amb-cli/src/main.rs` + `usage.rs` (USAGE moved out because main.rs grew; the hook install `set_raw_denoiser` in CLI main); `amb-gui/src/main.rs` (the **same** hook install again, 4 lines); `commands/mod.rs`; `amb-ops/src/lib.rs`; `amb-engine/src/lib.rs` (`mod raw_denoise`); `ui/core/app.js` (`wireDenoise()`); `keys.js`; `index.html`; `man/amb.1`; `ARCHITECTURE.md` (7); `FEATURES.md`; `GAPS.md` (33); `WHATS-NEXT.md`; `tests/ui/organise.js`. **14 registration files**. |
| 0bf3008 place on a map (12 files) | `commands/mod.rs` (`places::geocode`); `ui/library/library.js` (`wirePlacePicker()`); `index.html` (dialog + script tag); `style.css`; `slideshow.js` (a guard so the dialog's keys don't also drive the slideshow: see B3 key handlers); `FEATURES.md`; `WHATS-NEXT.md`; `tests/ui/browse.js`. **7 registration files**. This is the cheapest of the six: no CLI, no docs beyond two rows. |
| aab3c20 man page (8 files) | `main.rs` (mod + arm), `usage.rs` line, `AGENTS.md` (regen instruction), `tauri.release.conf.json`, `FEATURES.md`, `GAPS.md` (44 lines new), `WHATS-NEXT.md`. Small, and the only change here that **reduces** future cost (the man page is generated from USAGE and a test fails when stale). |

Average for a feature with GUI + CLI: 11-14 files that only register. A GUI-only feature (0bf3008): 7. Docs are 4-5 of every set: WHATS-NEXT (22 of the last 30 commits), FEATURES (18), ARCHITECTURE (9), GAPS (6, but with 33-44 line diffs each time).

### A2. Why GAPS.md costs so much
`docs/GAPS.md` section 0 is a **numbered** table ("| 12 | ... |"). Finishing item N renumbers every row below it: 87a74ab removes row 12 and rewrites rows 13-27 as 12-26 (33 changed lines for one feature). Four of the six commits do this. With several agents running in parallel (AGENTS.md says they do) this is a guaranteed merge conflict. The same one-line idea also lives in WHATS-NEXT.md ("Clean-up organiser: ...") and FEATURES.md: three copies of one fact, which docs/design/structure.md problem 7 already names.

### A3. Registration points table

| # | List / dispatch | Where | Entries | Must stay in sync with | Tested? |
|---|---|---|---|---|---|
| 1 | CLI dispatch `match positional.as_slice()` | `amb-cli/src/main.rs:259` | 73 arms | USAGE (2), man page (3), FEATURES `amb ...` strings (4) | **No.** A test (`man.rs:136`) spot-checks 4 commands only. |
| 2 | CLI flag tables: bool flags / value flags | `main.rs:~117-232` | 22 bool + ~108 value = 141 flags, global to all commands | each command's USAGE line; `--out`, `--to`, `--name`, `--format` mean different things per command | **No.** An unlisted flag gives "unknown option"; a listed flag no command reads is silently accepted. |
| 3 | `USAGE` text | `amb-cli/src/usage.rs` | 75 `    amb ` lines | man page, `docs/ai/*`, FEATURES | man page: **yes** (`the_man_page_in_docs_is_current`, `man.rs:124`, regenerate by hand). USAGE vs dispatch: no. |
| 4 | Man page | `docs/man/amb.1` | 76 `.B amb` entries | generated from USAGE | Yes (above). Good pattern. |
| 5 | Tauri handler `generate_handler![...]` | `amb-gui/src/commands/mod.rs:30` | 240 | 240 `#[tauri::command]` fns (24 files); 232 distinct `invoke("...")` names in ui/ | **No.** A forgotten entry compiles and fails at runtime. My diff of the lists: 3 names used by JS not in the list (`take_slideshow`, `ui_test`, `ui_test_done`, probably registered in main.rs), 7 listed but never invoked as a literal. |
| 6 | `mod x;` lines in `commands/mod.rs`, `amb-ops/src/lib.rs`, `amb-cli/src/main.rs` | those files | 23 / 11 / 24 | the files themselves | Compiler (fine). |
| 7 | `KEY_ACTIONS` | `ui/core/keys.js:24` | 117 | `MENU_BAR` in `ui/core/menu.js:12` (names), `docs/FEATURES.md`, search | Menu names vs actions: **yes** (`commands/menu.rs:250`). Docs: no. |
| 8 | `wire*()` calls | `ui/core/app.js:307-480` (`init`, 44 calls) + `library/library.js` (11) + 12 more elsewhere | 71 `function wire*` definitions | the function must exist and be called once | **No.** A defined but never called `wireX` is silent; a missing one throws at load. |
| 9 | `<script src>` tags | `ui/index.html` | 88 | files in ui/ | Yes (`main.rs:298`: every script loaded once). Load **order** is untested. |
| 10 | `<dialog>` markup | `ui/index.html` | 43 dialogs, 1,924 lines | their JS ids; CSS `.x-dialog` widths | Ids unique: yes (`main.rs:281`). JS `$("id")` exists in html: **no**. |
| 11 | Toolbar button (recipes: 11 steps) | html, layout.js, looks/mod.rs, app.js, keys.js, menu.js, editor.js, library.js, icons.js, ui-layouts.md | 10 files | each other | Partly (`format_reference_is_complete`, menu test, `recipes.rs` checks names exist). |
| 12 | Built-in plugin list `BUILTIN` | `amb-plugin/src/lib.rs:50` | 12 tuples (array length is a literal) | `native.rs:27-34` match (8 arms), root `Cargo.toml` members, `amb-plugin/Cargo.toml` deps (8), `plugins/<id>/plugin.wasm` (committed binary), JS `PLUGINS_ELSEWHERE` (`dialogs/plugins.js:23`, 8 ids), `TRIP_PLUGINS` (`tools/trip.js:12`, 7), `plugins/trippy/plugin.json` tiles | native vs wasm equality: **yes** (`tests.rs:347`). The JS sets: **no**. wasm freshness vs `src/`: **no**. |
| 13 | AI `TASKS` | `amb-ai/src/lib.rs:43` | 7 (literal length) | `data/models.json` (one default per task: tested `lib.rs:914`), engine hook in CLI main (`main.rs:96-102`) **and** GUI main (`main.rs:185-191`), per-task `else if` in the test, GUI models dialog | Default-per-task: yes. Hook installs: **no**, and they are duplicated in two hosts. |
| 14 | Engine hooks (`set_finder`, `set_selector`, `set_face_finder`, `set_inpainter`, `set_raw_denoiser`, `set_resolver`) | `amb-engine/src/{subject,retouch,raw_denoise,plugin_effects}.rs` | 6 setters x 2 hosts | each other | **No.** `amb mcp`/`amb do` rely on CLI main; forgetting one host = the feature "works in the editor, not in `amb`". |
| 15 | MCP tools | `amb-cli/src/mcp.rs` (tools/list JSON + `tools/call` match) | 17 JSON `name`s, 13 literal arms | USAGE `amb edit` line, `docs/ai/mcp.md`, `docs/ai/README.md`, command list in tool description (recipes step 6) | **No** beyond two names asserted in `handshake_tools_and_errors`. |
| 16 | MCP resources | `mcp.rs` `resources/list` | 3 hand-written + `GUIDES` | `docs/ai/mcp.md` | GUIDES: yes. The 3 hand ones: no. |
| 17 | `GUIDES` | `amb-protocol/src/guides.rs:30` | ~12 | `docs/ai/*.md` | **Yes** (`every_guide_for_ai_agents_is_built_in`). Best example in the codebase of "forget it and a test fails". |
| 18 | Protocol `Command` | `command.rs`, `state/mod.rs`, `schema.rs`, `compact.rs`, `card.rs`, `mcp.rs`, `ai/README.md` (recipes: 8 steps) | 7 files per command | schema vs serde | Schema vs serde yes; `core.md` current: yes (regenerated); the rest: no. |
| 19 | Develop setting (recipes: 7 steps) | tags.json, descriptions.json, settings.rs, plan.rs, schema.rs, 2 generated docs | 7 | each other | Generated docs: yes. GPU `Step` list (plan.rs): "preview won't show the setting" is silent, no test named. |
| 20 | `FEATURES.md` | `docs/FEATURES.md` (147 lines) | ~100 "- **want** -> how" rows | CLI commands, keys.js labels, buttons | **Parsed** by the app (`commands/features.rs`, one source for docs and Search, good). A test checks the parser only, not that named commands/buttons exist. |
| 21 | `ARCHITECTURE.md` file lists, `GAPS.md`, `WHATS-NEXT.md`, `NEXT.md`, `AGENTS.md` layout | docs | 405 / 359 / 160 / 173 | code; each other | Recipes: yes (names exist). The rest: no. Stale already: structure.md says "63 scripts, 1,085 names, 316 x Object.assign, 157 x toast(String(e))"; today: 88 script tags, ~1,200 top-level names, 381 and 166. |
| 22 | hub kinds | `hub/mod.rs` `known_kind`, `fetch_item`, `browse`, `kinds.rs`, `commands/mod.rs`, `hub.js`, `index.html` tabs, hub repo script | 7 places | each other | hub items vs `index.json`: yes; the kind plumbing: no. |
| 23 | UI test scripts | `tests/ui/{smoke,organise,browse}.js` | 3 files, ~250 steps total | each feature adds steps to the same file | n/a. `organise.js` was edited by 3 of the 6 commits; smoke.js has a hard-coded undo count. |
| 24 | OVER_BUDGET | `amb-cli/tests/code_size.rs:17` | 8 files | file sizes | Self-checking. |

Takeaway: the places where a test exists (GUIDES, man page, menu names, script tags, ids, recipes, settings reference, core card, natives vs wasm) are the places that rarely bite. The unguarded ones are the highest-frequency edit points: CLI dispatch vs USAGE, flag tables, Tauri handler, wire calls, plugin id sets in JS, engine hooks in two hosts.

## B. Duplication (counts from grep at HEAD)

### B1. CLI (`amb-cli/src/*.rs`, 6,384 lines, 24 files)
1. **Global flag tables** (main.rs 117-232): 141 flags, no per-command ownership. Every new option edits the central list; typos are only caught as "unknown option" for flags not in either table.
2. **`--json` plumbing**: `if json` 47 sites, `serde_json::to_value(&..)` 16, `print_json` 38 calls, `serde_json::to_string_pretty(...).map_err(|e| e.to_string())` hand-written again in places. Each command re-decides human vs JSON output (cleanup.rs:50, convert.rs:51, library.rs:216).
3. **Progress**: 6 hand-rolled `eprint!("\r...")` loops (cleanup.rs:52, convert.rs:51, library.rs:216, main.rs:67 and :399, search.rs:54), each with its own throttling (`done % 50 == 0`) and "skip when --json".
4. **Path expansion**: `organise::photos` and `photos_deep` (organise.rs:22-58) are shared (good), but `library.rs:256 absolute`, `mcp.rs:622 list_photos`, `presets.rs:76` (dir walk) re-walk folders separately. The `Options` alias exists in organise.rs and `BTreeMap<&str,&str>` is spelled out in 8 other signatures.
5. **Sidecar read-modify-write**: `read_info`/`write_info` (organise.rs:76-88) is the CLI copy of what GUI `library.rs:297 change_info`, `session/mod.rs:539`, `folder.rs:360`, `mcp.rs:721` each do (`with_info` + `write_atomic`): **5 implementations** of the same 3 lines, each with its own Lightroom-catalogue-in-place handling.
6. **Batch render naming**: `render.rs:271 render_many` (name template + `taken` loop adding `-2`) vs GUI `library.rs:426 export_many` + `unique_path` (`-2` too, different start rule). Two batch exporters.

### B2. GUI Rust (`amb-gui/src`, 16.8k lines)
- `#[tauri::command]`: 240 across 24 files. `map_err(|e| e.to_string())`: **306 sites in 41 files** (236 in commands/). `Res<T> = Result<T,String>` exists but no `Into` helper.
- `spawn_blocking(...).await.map_err(|e| e.to_string())?`: 105 `spawn_blocking` in 19 files, each followed by the same double-`?` unwrap (batch.rs:70-75, 186-190).
- The **throttled progress emitter** (`Mutex<Instant>` + `elapsed().as_millis() > 200` + `app.emit("x-progress", [done,total])`): batch.rs:34-41, batch.rs:184-190, export.rs:222-229 (3 copies; bursts emits unthrottled with an `AtomicUsize`, a 4th shape). Event names: `duplicates-progress`, `bursts-progress`, `cleanup-progress`, `denoise-progress`, `convert-progress`, `replay-progress`, `index-*`: one ad-hoc event per feature, each with a JS listener.
- `app.path().app_cache_dir().ok().map(|d| d.join("thumbnails"))`: batch.rs:21, 88, 162, commands/library.rs:67, main.rs:213 (5).
- `paths.iter().map(PathBuf::from).collect()`: 12 in 4 files.
- The "photo thumbnail with fallback to `folder::thumbnail` then `decode_jpeg`" closure is pasted three times in batch.rs (26, 92, 166) with `edge` hard-coded per use.
- State locking: `AppState` has 12 `Mutex`/`RwLock` fields (state.rs:8-29); 111 `.lock()` calls, 78 of which pick `unwrap_or_else(|e| e.into_inner())` or `unwrap` ad hoc (poison handling is inconsistent: 5 explicit `into_inner`, rest `unwrap`). No `with_session(|s| ..)` helper: `session: Arc<Mutex<Option<Session>>>` is unwrapped by hand in each command.

### B3. JS (`ui/**/*.js`, 87 files, ~23.3k lines)
| Shape | Count | Examples |
|---|---|---|
| `invoke(` | 316 in 59 files | everywhere |
| `toast(String(e), true)` | 166 in 46 files (157 on 2026-11-09: +9 in 3 days) | denoise.js, cleanup.js, app.js:354-356 |
| `document.createElement` | 589 in 69 files; `Object.assign(document.createElement` 381 in 56 files (316 three days ago: +65) | guess-places.js (0bf3008 adds another), masks, library-grid |
| `.showModal()` | 43 in 26 files; dialog `close` listeners 21 in 12 files; `$("x-dialog").close()` in every dialog's close button | cleanup.js:154,172,197; denoise.js:65,84 |
| `tauri.event.listen(` | 23 in 15 files, the progress ones share one body (`const [done,total]=e.payload; status(...)`) | bursts.js:128, cleanup.js:210, denoise.js:89, duplicates.js:139, folders.js:184 |
| plural strings | `=== 1 ? "..."` 70 in 24 files; `? "" : "s"` 61 in 22 files | denoise.js:35, bursts.js:114 |
| `keydown` listeners | 42 in 30 files; per-dialog guards `closest("dialog[open]")` 7 in 6 files (slideshow.js added one in 0bf3008 just because a new dialog opened over it) | |
| `try { await invoke } catch` | 308 `catch` in 66 files | |
| `localStorage` | 14 in 5 files (a `store` helper now exists; structure.md R-phase did this) | fine |
| `wire*` functions | 71, called from app.js (44), library.js (11), 12 elsewhere | |
Dialog skeleton (the same ~30 lines per dialog): flow function builds text -> `showModal()`; install button disables, `invoke`, `catch -> status text`, `finally -> re-enable`; `progress` listener; close button. denoise.js, cleanup.js, duplicates.js, bursts.js, guess-places.js, place-picker.js follow it. No `dialog()` / `runWithProgress()` helper in `core.js`.

### B4. amb-ops / amb-library
Mostly **fine**: ops share `burst`, `duplicates`, `object::...` (cleanup.rs imports `burst` and `duplicates`); sidecar writes inside ops go through `amb_protocol::sidecar` (cleanup.rs, denoise.rs). Repeated shapes:
- `progress: &dyn Fn(usize, usize)` in 8 functions (ops: replay, denoise, convert, cleanup; library: search, import, duplicates, catalog), 3 slightly different signatures (some `(done,total)`, `export_many` takes `(done,total,&str)`). No shared `Progress` type.
- rayon `par_iter` + `AtomicUsize` counter + callback: cleanup.rs, burst, merge, negative, object, library/duplicates, contact (7). Each re-writes the "count and report" step.
- "small preview for a photo" is passed in as closures by every caller (CLI: embedded only; GUI: embedded, then cached JPEG, then render). Not an ops step.
- Per-feature `Options` structs with `Default` carry the thresholds (good: `cleanup::Options::default()` has burst 2.0, alike bits); the GUI re-states `burst_seconds: 2.0` (batch.rs:177) instead of using the default.

### B5. CSS (`ui/style.css` 1,602 lines, `widgets.css` 74)
- 46 `.x-dialog` rules, 34 plain `dialog` rules, 43 `<dialog>`s. A dialog width is set per dialog: `max-width: calc(100vw - 32px)` appears **10 times** (lines 295, 455, 559, 601, 758, 852, 1301, 1305, 1331, 1355...) next to a different width (560/680/720/780/820/860/900). Mixed `#id-dialog` and `.class-dialog` selectors.
- Thumbnail/strip rules re-written per dialog: `object-fit: contain; background: var(--bg-deep); display:block` 11 times; `*-strip`/`*-thumb` 14 rules (`.clean-thumb`, `.dupes-*`, `.cull-*`).
- Each new dialog adds 15-30 lines (clean: 26; pick-dialog 15). 18 of the last 30 commits touched style.css.
- Not all bad: colors go through variables, `color-mix` is used consistently (14).

## C. Layering

AGENTS.md rules checked.

### C1. Every edit goes through amb-protocol: **mostly true**
- Develop edits (settings, masks, spots, presets): GUI `execute` and CLI `do`/`edit` use `amb_protocol::edit/Command`. No violation found.
- Sidecar **metadata** (rating, flag, label, stack, place) is written outside `Command`: `amb-cli/src/organise.rs:82 write_info`, `cleanup.rs:70` (reject), `places.rs:56,125`, `organise.rs:332,339,567`; `amb-gui/src/library.rs:297`, `session/mod.rs:539`, `folder.rs:360`; `mcp.rs:721`. These go through `amb_protocol::sidecar::with_info` (so file format knowledge stays in protocol), but are not undoable commands and not one function. If "undoable" is meant to cover flags, this is a violation; if edits means develop settings only, it is fine but should be written down. Five copies of the write step is the real cost (B1.5).
- `std::fs::write` of `.xmp` in gui: only presets and hub installs (`preset_files.rs:100`, `hub/kinds.rs:208,266,279,314`), and they use `write_atomic`. OK.

### C2. Logic in cli/gui that belongs in amb-ops / amb-library
| Where | What | Where it belongs |
|---|---|---|
| `amb-gui/src/commands/batch.rs:75-135 library_best_in_bursts` | burst grouping (duplicates::find then filter by `Kind::Burst`), parallel `look` per photo, `rank`, a face-model probe (`subject::faces(&[0.0; 8*8*3],8,8).is_ok()`), progress | `amb-ops::burst` as `best_in_bursts(photos, thumb, progress)`. The CLI has no equivalent command: `amb duplicates` returns bursts unranked, so the same feature differs by front end. |
| `amb-gui/src/folder.rs:244 thumbnail` | embedded preview, else a session render, disk cache keyed by path+size+mtime+orientation, virtual-copy source | `amb-library` (thumbnail service). The CLI cannot reuse it: `cleanup.rs:48`, `organise.rs:317`, `convert.rs:213` call `embedded_thumbnail(..).ok().flatten()` only. **Likely behaviour drift**: `amb cleanup`/`amb duplicates` skip fingerprint/blur tests for photos without an embedded preview (JPEG/PNG/TIFF), while the editor includes them. Worth verifying by running both on a JPEG folder. |
| `amb-gui/src/library.rs` (716 lines) | `list_tree/list_many/catalog_folder/smart_photos/all_photos` (listing and merging catalogue + folders), `change_info`, virtual copies (`create/delete_virtual_copy`), paste-edit groups, `export_many` + `unique_path` (batch export naming and tag embedding) | `amb-library` (listing, info changes, virtual copies) and `amb-ops` (batch export). Its own header says "Everything here reads and writes the photos' XMP sidecars". The CLI re-implements `tag`, `info`, `render_many` instead of calling it. |
| `amb-gui/src/collages.rs:257 render/render_batch`, `save_project/open_project` | collage rendering and `.ambcollage` format; the CLI `collage` (library.rs:9) is a second driver | `amb-export` or `amb-ops` |
| `amb-cli/src/cleanup.rs`, `organise.rs duplicates/tag` | option parsing with thresholds ("0 to 20", rating 0-5, flag/label vocabulary, place parsing) in the CLI; the GUI uses different literals for the same knobs (`burst_seconds: 2.0`, `max_bits.min(20)` in batch.rs:44) | validation in the ops `Options` (`TryFrom`/`parse`), once |
| `amb-cli/src/mcp.rs:622 list_photos` and 832-line mcp.rs | folder listing and render tools implemented inline | `amb-library::folder` (already has `photo_paths`) |
| `amb-gui/src/session/*` (6.6k lines incl. tools, render, retouch) | `Session` holds the open photo. It is the GUI's, but `Session::open(p).and_then(|s| s.export(..))` is also the batch export path used by `export_many`, so ops depends on a GUI object. The CLI has its own `render()`. Two renderers of "a photo with its sidecar to a file". | Session's non-UI parts into amb-ops or amb-engine::session. Needs a design decision (opus). |

### C3. UI hard-codes of registry knowledge
- Develop settings: registry-driven (`editor_schema`), no violation found in `develop/*`. A few default/flag numbers: `develop/presets.js:250 set("Vibrance", 15)`, `help/teach.js:167,256` (names plus a "15"): lesson content, tolerable.
- **Mask parameters are not in the registry**: `ui/tools/mask-panel.js:188-228, 317-320` has ~13 `sliderRow({ label, min, max, step, value })` literals (Feather 0-100, Angle -180..180, From/To 0-100, Smoothness 0-50, Refine 0-100, Size 0.5-25, Flow 1-100, Density 1-100); the clamping lives separately in `amb-protocol/src/masks/fields.rs:209,303,412` (Feather 0-100 default 50 etc.). Two copies, free to drift. Mask params could be served by `editor_schema` like settings.
- `looks/looks-dialog.js:169` slider `max: 16`, `develop/fx.js:151` Amount `max: wild ? 200 : 100` with `currentTask() === "trippy"` hard-wired in a core effects panel (and `core/keys.js:38` lists the five task names). Plugin-specific rule in core UI; belongs in the task plugin manifest.
- JS name sets mirroring Rust: `PLUGINS_ELSEWHERE`, `TRIP_PLUGINS` (A3 row 12).

### C4. Dependency order (from each `crates/*/Cargo.toml` path deps)
```
L0  amb-color, amb-lens, amb-presets, amb-plugin-sdk                      (no amb deps)
L1  amb-protocol -> presets;   amb-testkit -> presets, color              (dev only for engine/library/gui)
L2  amb-engine -> color, lens, presets
L3  amb-export -> color, engine;   amb-gpu -> engine, presets
L4  amb-ai -> engine, gpu;   amb-library -> engine, export, lens, presets, protocol
L5  amb-ops -> engine, export, gpu, lens, library, presets, protocol
    amb-plugin -> sdk, engine, export, presets, protocol, + 8 built-in plugin crates
L6  amb-cli -> all but gui;   amb-gui -> all but cli
```
No cycles. Notes:
- **Engine hooks registered from above** (deliberate and documented in structure.md): `subject::set_finder/set_selector/set_face_finder`, `retouch::set_inpainter`, `raw_denoise::set_raw_denoiser`, `plugin_effects::set_resolver`. Six global `OnceLock`-style setters in the engine, installed by **both** host `main`s with copy-pasted lists (CLI main.rs:96-111; GUI main.rs:185-205). Any third host (tests, a daemon, MCP in-process) must remember them. Not a layering bug, but the #1 "works in the editor, not in `amb`" risk.
- `amb-plugin` depends on 8 individual plugin crates for the native fast path, so every built-in effect is a dependency edge of a core crate (and a workspace member and a registration in `native.rs`).
- `amb-library` -> `amb-export` (contact sheets, import naming): fine.
- `amb-cli` -> `amb-gpu` and `amb-ai` directly (to run hooks/ops): fine.
- `amb-ops` -> `amb-gpu`: legitimate (renders photos).
- GUI-only app code at L6 contains L5 content (C2). That is the only "downward-missing" layering problem; nothing depends upward.

## D. Scalability traps

### D1. Files every feature edits (last 30 commits, `git log -30 --name-only`)
| Commits | File |
|---|---|
| 22 | docs/WHATS-NEXT.md |
| 18 | docs/FEATURES.md |
| 18 | crates/amb-gui/ui/style.css |
| 16 | crates/amb-gui/ui/index.html |
| 11 | crates/amb-gui/tests/ui/browse.js |
| 10 | crates/amb-gui/ui/core/keys.js |
| 9 | docs/ARCHITECTURE.md; tests/ui/smoke.js |
| 8 | ui/library/library.js |
| 7 | commands/mod.rs; amb-cli/src/main.rs |
| 6 | docs/GAPS.md |
| 5 | ui/core/tasks.js; ui/core/app.js |
| 4 | docs/man/amb.1, looks/layout.js, slideshow.js, cull.js, menu.js |
Docs and the two shared UI files (style.css, index.html) are the real bottleneck for parallel agents: 4 files touched by more than half of all commits.

### D2. index.html
1,924 lines (1,781 on 2026-11-09: +143, +8% in three days), 43 dialogs, 88 script tags, one file; structure.md decides it "stays whole" (no build step). Every dialog is inline markup + a `<script>` line, so each GUI feature adds to a file that already conflicts. A runtime fix that keeps "no build step": `<template id="x-dialog">` files fetched on first use, or `<dialog>` markup created by each feature's own JS from a string. Alternatively accept it: 1.9k lines of mostly flat HTML is cheap to read; the cost is conflicts, not comprehension.

### D3. Global JS namespace
- 87 files, 981 top-level `function`s + 216 top-level `const/let/class` = ~1,200 shared names. Duplicates now: **0** (`ui_scripts_declare_each_top_level_name_once` passes and reads subfolders). Fine.
- Risks the test does not cover: a feature that shadows a *browser* global (`status`, `name`, `top`, `open`: `status()` is already used as a function in bursts.js and denoise.js, `top` is a window property), load-order dependence for top-level `const x = otherFileFn()`, and names created via `window.x =`/`globalThis`. Moderately likely over time, low severity.
- Hot names: structure.md reports `masks.js` used by 32 files, `library.js` 24, `preview.js` 22: change in those ripples (no module boundary).

### D4. Startup
All 88 scripts parse eagerly (none lazy), then `init()` (app.js:307-480) runs `await invoke("editor_schema")`, `await loadLooks()`, `await loadTaskPlugins()`, `await loadKeybindings()` **serially before attaching any click handler** (lines 309-323), then 44 `wire*` calls + ~60 inline `addEventListener`s, then library.js adds 11 more. Each new feature adds one wire call and one script parse to the startup path; library dialogs (cleanup, denoise, duplicates, bursts, guess-places, place-picker) wire eagerly although most users open them rarely. I have not measured it (no run), so the real cost is unknown; the serial awaits are the part that is certainly fixable (`Promise.all`). Honest assessment: probably fine at 1.2k functions; do not refactor unless a measurement shows >200 ms.

## What is already fine (do not invent work)
- Crate graph: clean, no cycles, documented, engine hooks are a conscious trade-off.
- One edit path for develop settings across GUI/CLI/MCP.
- GUIDES, man page, menu names, script-tag, id-uniqueness, name-uniqueness, recipes, settings reference, core card, plugin native-vs-wasm: all test-guarded.
- `FEATURES.md` being the one parsed source for docs and in-app Search is a good pattern; `man.rs` generated from USAGE is another. Extend these, do not replace them.
- `amb-ops` already took the "batch operations" job; cleanup.rs/denoise.rs are thin glue (CLI 108 lines, ops 401).
- File-size budget test (`code_size.rs`) works and kept big files from growing; JS max file is now 593 lines.
- Per-ops `Options`/`Default` structs are the right place for thresholds.

## Proposals

Order roughly by payoff per effort. Tier: haiku = mechanical, sonnet = multi-file refactor guarded by tests, opus = needs a design decision.

| # | Structural cause | Change | Files | Test that keeps it true | Risk | Tier |
|---|---|---|---|---|---|---|
| 1 | CLI dispatch, USAGE, flag tables and man page are four hand-synced lists (A3 rows 1-4) | A `COMMANDS: &[Command]` table in `amb-cli/src/commands.rs`: `{name, usage lines, flags (bool/value, per command), run: fn(&Ctx)->Result}`. Dispatch, `--unknown option for this command`, USAGE text and the man page are generated from it; flags move to their command. Do it in two steps: first the table with the same `match` (haiku-sized), then flags per command. | main.rs, usage.rs, man.rs, every `amb-cli/src/*.rs` entry fn signature (`Ctx{options,json,dry_run}`) | Test that the generated USAGE equals the checked-in man page (exists); new: every FEATURES.md `` `amb x` `` names a table entry, every entry has a man line | medium (many call sites, but each entry is mechanical and `tests/` run the binary) | sonnet |
| 2 | Tauri handler list is hand-typed, forgetting compiles (A3 row 5) | Either a `build.rs`/macro that collects `#[tauri::command]` fns, or (cheaper) a test that greps `#[tauri::command]` fn names in `src/` and asserts each appears in `commands/mod.rs`, and every `invoke("x")` literal in `ui/` appears in the list or an allowlist (`ui_test`, `ui_test_done`, `take_slideshow`). | commands/mod.rs, new test in amb-gui | the test itself | low | haiku (the test); sonnet (a macro) |
| 3 | Engine hooks installed in two hosts by copy-paste (A3 rows 13-14, C4) | `amb_ai::install_hooks(models_dir)` and `amb_plugin::install_resolver(plugins_dir)` (or one `ambrotype_host::init()` in `amb-ops`), called by CLI main, GUI main and MCP. Also remove the literal `[(&str,&str); 7]` length with a slice. | amb-ai/src/lib.rs, amb-cli/src/main.rs:96-111, amb-gui/src/main.rs:185-205, amb-ai tests | a test in amb-cli (or amb-ops) that after `init()` every engine hook reports installed (add `is_installed()` to each setter's module) | low | haiku |
| 4 | "Small preview for a photo", the sidecar read-modify-write step, batch export naming, and best-in-bursts all live in amb-gui/amb-cli, so the front ends drift (C2, B1.5, B2) | Move `folder::thumbnail`'s core into `amb-library::thumbs`, `change_info` into `amb-library::info::change`, `export_many`/`render_many` into `amb-ops::export_batch`, `library_best_in_bursts` into `amb-ops::burst::best_in_bursts`. GUI and CLI keep only glue. CLI cleanup/duplicates then also read JPEGs. | amb-gui/src/library.rs (716), folder.rs, commands/batch.rs; amb-cli organise.rs, cleanup.rs, render.rs, convert.rs; amb-ops; amb-library | Existing ops tests plus one cross-front-end test: `amb cleanup --json` on a folder with a JPEG equals the ops result with the same inputs; `export_batch` naming collisions test | medium: touches Session (see 5) | sonnet (opus for the Session split in C2 row 6) |
| 5 | Docs: one fact in 3-4 places; renumbered tables (A2) | (a) Turn GAPS section 0 into an unnumbered list or ids (`I-12` that never renumber); (b) one "Recently done" place: WHATS-NEXT only, FEATURES gets the user-facing row, ARCHITECTURE stops listing files per crate (a test already knows the crates: generate the table or drop the file column); (c) a `docs/CHANGES` convention: one new file per feature in `docs/done/YYYY-MM-DD-name.md`, assembled by a script, so parallel agents never touch the same lines. | docs/GAPS.md, WHATS-NEXT.md, ARCHITECTURE.md, AGENTS.md | test: no two numbered rows reference each other by number; ARCHITECTURE crate table lists exactly `crates/*` (cheap) | low for (a); (c) changes agent workflow: ask the owner | haiku for (a)(b); opus for (c) |
| 6 | New dialog = 30 lines HTML + 25 lines CSS + 40 lines JS skeleton + script tag + wire call (B3, B5, D2) | `core.js` helpers: `openDialog(id, opts)`, `runWithProgress(event, invokeFn, onProgress)` (the invoke + `catch -> text` + `finally -> re-enable` + progress listener skeleton), `plural(n, "photo")`, `showError(e)`; one CSS rule set `dialog[data-size="wide"|"medium"|"narrow"]` replacing the 10 `max-width: calc(...)` lines. Convert only the six newest dialogs first (denoise, cleanup, duplicates, bursts, guess-places, place-picker), leave old ones unless touched. | ui/core/core.js, ui/style.css, library/*.js, index.html (add `data-size`) | existing UI tests (`organise.js`, `browse.js`) must pass unchanged; add a test that `toast(String(e), true)` count does not rise (like `ui_scripts_ask_with_ask_yes`, `main.rs:350`) | low-medium | sonnet |
| 7 | Built-in plugin = 8 edit points, JS id sets hand-copied (A3 row 12) | `plugins_list` already returns built-ins: serve the "elsewhere"/"trip" membership from plugin manifests (`"ui": {"hidden_in_list": true, "trip": true}`), delete `PLUGINS_ELSEWHERE` and `TRIP_PLUGINS`; derive `native.rs` arms and `BUILTIN` from one macro table (`builtin_plugins! { "kaleidoscope" => amb_plugin_kaleidoscope::kaleidoscope, ... }`); a test that `plugins/*/plugin.wasm` is newer than `src/` (or rebuilds in CI) | amb-plugin/src/{lib,native}.rs, plugin manifests, ui/dialogs/plugins.js, tools/trip.js | native-vs-wasm test (exists); new: every `plugins/*` with `"builtin": true` is in BUILTIN | medium | sonnet |
| 8 | Mask parameter ranges/labels in JS, clamping in Rust (C3) | Add `mask_schema` (from `amb-protocol/src/masks/fields.rs`) to `editor_schema`, make `mask-panel.js` build `sliderRow` from it. Same for `looks-dialog.js` slider range and the `trippy` special-case in `fx.js` (move to the task manifest). | amb-protocol/masks/fields.rs, amb-gui/src/schema.rs, ui/tools/mask-panel.js, fx.js | a test that every `fields.rs` field appears in the schema and the panel has no numeric `min:` literals (grep test like `ui_scripts_ask_with_ask_yes`) | medium (UI behaviour must stay identical) | sonnet |
| 9 | Shared UI/test files conflict (D1) | Split `tests/ui/organise.js` and `smoke.js` per feature (`tests/ui/features/<name>.js`, CI already runs every script in the folder); replace smoke.js's hard-coded undo count with "undo until the history is empty". | tests/ui/*.js | CI runs all | very low | haiku |
| 10 | CLI/GUI progress and `Res` boilerplate (B1.3, B2) | `amb_ops::Progress` type (`&dyn Fn(done,total)` + a throttle wrapper) and in amb-gui `fn emit_progress(app, event) -> impl Fn(usize,usize)` and `trait ResultExt { fn s(self) -> Res<T> }` replacing 306 `map_err(|e| e.to_string())`. Apply when touching a file, no sweep. | amb-ops/src/lib.rs, amb-gui/src/main.rs (+ commands as touched) | none beyond compiler | very low | haiku |
| 11 | USAGE/`FEATURES.md`/KEY_ACTIONS only checked in part (A3 rows 7, 15, 20) | Tests: every `amb ...` in FEATURES.md exists in USAGE; every `**Button**` name in FEATURES exists in `index.html` text or keys.js labels; MCP: `tools/list` names equal the `tools/call` arms (expose arms as a const list). | amb-cli tests, amb-gui tests | the tests | low (may find existing mismatches, which is the point) | haiku |

Not recommended now: ES modules for the UI, a JS framework, splitting index.html with a build step, rewriting `Session` (structure.md "Not now" is still right; item 4's Session split is the only part that needs an owner decision). Startup (D4): measure first; at most make the four serial awaits in `init()` parallel.
