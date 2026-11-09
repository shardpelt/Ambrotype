# Cleanup tasks

Exact steps for the cleanup planned in [design/structure.md](design/structure.md). They are written so that any coding agent, a small model included, can do one task with no other context.

- Each task is one commit and changes nothing the app does.
- Do them in the order given: later tasks expect the earlier ones done.
- ☐ is to do, ☑ is done (with the date).

## Rules for every task

1. **One task, one commit.**
   - Do only what the task says: no fixes, renames or reformatting on the side.
   - If you notice a bug, add one line about it under "Found while cleaning" at the end of this file, in the same commit.
2. **Move, don't rewrite.** Cut and paste whole items unchanged, with their doc comments, attributes and the comment lines above them. The only edits allowed are:
   - `mod` and `use` lines;
   - visibility: `pub(super)` or `pub(crate)` where the compiler asks; never plain `pub` unless the item already was;
   - `include_str!` and `include_bytes!` paths: one more `../` for each folder deeper the file moves;
   - paths in the Tauri handler list;
   - `<script>` tags in `index.html`.
3. **Names stay.** Never rename these, even when a new name seems clearer:
   - public items and Tauri commands;
   - setting names, localStorage keys, CSS classes, element ids and event names;
   - file formats.

   Sidecars, saved settings, plugins, hub items and tests depend on them.
4. **Start clean:** `git status` must show nothing to commit. If it doesn't, stop and say so.
5. **Stage only your files:** `git add <each path>`. Never `git add -A` or `git add .`.
6. **The checks below pass before you commit.**
   - If a check fails and two attempts don't fix it, undo the task: `git restore --staged --worktree .`, then delete the new files you made.
   - Then report what failed. A half-done move is worse than none.
7. **Fix what names the moved code.**
   - Search: `grep -rn "old/path.rs" docs AGENTS.md README.md crates tools`.
   - Correct each mention. `docs/recipes.md` is checked by a test.
   - Take a split file off `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs`; the test tells you when.
8. **Mark the task ☑ here** with the date, in the same commit.
9. **Commit message:**
   - the task's first line, a blank line, its "why" line;
   - then the trailer lines your environment asks for, if any.
   - No model names in the first line or the why.
10. **Push only if the person who gave you the task said to.**

## Checks

Rust, before every commit (as AGENTS.md says):

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

While working, `cargo check -p <crate> --all-targets` is faster.

A task that changes only `amb-gui` (no other crate depends on it) or only the UI's scripts can run these instead of the whole workspace's clippy and tests:

```sh
cargo fmt --all
cargo clippy -p amb-gui --all-targets -- -D warnings
cargo test -p amb-gui
cargo test -p amb-cli --test code_size --test recipes
```

Whoever pushes runs the whole workspace's checks first.

UI, for any task that touches `crates/amb-gui/ui/`: also run the UI tests, as CI does. The UI is built into the binary, so build first.

```sh
cargo build -p amb-gui -p amb-cli
T=/tmp/ui-check && rm -rf $T && mkdir -p $T/more
target/debug/amb testimage colorchecker $T/test.dng
target/debug/amb testimage ramp $T/more/other.dng
target/debug/amb lrcat sample $T/more/Sample.lrcat $T/more/other.dng
export XDG_CONFIG_HOME=$T/cfg XDG_CACHE_HOME=$T/cache XDG_DATA_HOME=$T/data
for t in crates/amb-gui/tests/ui/*.js; do
  xvfb-run -a -s "-screen 0 1600x1000x24" target/debug/ambrotype --ui-test "$t" $T/test.dng || echo "FAILED: $t"
done
```

Each script prints its steps and ends with how many passed; any `FAILED` line means the task isn't done.

Disk: a debug build folder takes about 13 GB. If a build stops with "No space left on device", run `rm -rf target/debug/incremental target/release` and build again.

## Moving Rust code

**A file into a folder:**
- `git mv src/foo.rs src/foo/mod.rs` first, so git keeps its history.
- Then cut each part into `src/foo/<part>.rs`, and add `mod <part>;` near the top of `mod.rs`.
- Paths from outside stay the same: in `mod.rs`, `pub use <part>::{Name, …};` for each item other files use. Add the re-export where the compiler says an item isn't found; don't change the callers.
- A new file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`, then a `//!` line or two saying what it holds, then `use super::*;` (and any `use` lines the compiler asks for).

**Methods of one type can live in several files.**
- Each file gets its own `impl Type { … }` block.
- Child modules see their parent's private fields and functions, so nothing needs to become public for that.
- A private helper used by two of the new files moves to `mod.rs` as `pub(super)`.

**Tests:**
- A `#[cfg(test)] mod tests { … }` block moves to `foo/tests.rs`, without the `mod tests {` wrapper.
- In `mod.rs`, write `#[cfg(test)] mod tests;`. In `tests.rs`, `use super::*;` still works.

**Data files:** `include_str!("../data/x.json")` in `src/foo.rs` becomes `include_str!("../../data/x.json")` in `src/foo/mod.rs`.

## Moving UI code

The editor's scripts are plain `<script>` files sharing one global scope, loaded in the order of `index.html`.

- **Where a new file loads:** add its `<script src="…">` tag right after the file it came from. Two exceptions:
  - `core.js` loads first;
  - `app.js` stays last, because its last line starts the app, so files split out of `app.js` load just before it.
- **What runs while loading:** code outside functions (a top-level `const x = …` or a call) runs when its file loads. It may only use names from files loaded before it.
  - When you move a section, check its top-level lines.
  - If one uses a name from a file that loads later, or if the file it came from uses one of the section's names in its own top-level lines, keep that line where it was.
- **Each top-level name is declared once** across all files; a test checks.
- A new file starts like the others:
  - `// SPDX-License-Identifier: GPL-3.0-or-later`;
  - two to four comment lines saying what it holds;
  - `"use strict";`.
- Cut whole sections, from one `// ---------------------------------------------------------------- name` line to the next, with the marker line.

## Phase 0: guard rails

- ☑ **G1.** The tests on the UI's scripts read `ui/` and its subfolders, and `every_ui_script_is_loaded_once` checks that `index.html` loads each script exactly once (`crates/amb-gui/src/main.rs`). (2026-11-09)
- ☑ **G2.** The size budget test, `crates/amb-cli/tests/code_size.rs`. (2026-11-09)
- ☑ **G3.** `tools/code-metrics.py` measures sizes, long functions and the UI's shared names. (2026-11-09)

## Phase 1: small merges

### ☑ S1. One `smoothstep` for the engine (2026-11-09)

Why: eight copies of one function, two with a guard for equal edges and six without (those give NaN when the edges are equal and `x` is on them). Every call passes the lower edge first, so the guarded form changes no result.

1. Create `crates/amb-engine/src/math.rs`:
   ```rust
   // SPDX-License-Identifier: GPL-3.0-or-later
   //! Small numeric helpers shared by the pipeline's stages.

   /// 0 below `e0`, 1 above `e1`, a smooth S between. Equal edges give a
   /// step at `e0`.
   pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
       if e1 <= e0 {
           return if x >= e0 { 1.0 } else { 0.0 };
       }
       let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
       t * t * (3.0 - 2.0 * t)
   }

   /// [`smoothstep`] in `f64`.
   pub fn smoothstep_f64(e0: f64, e1: f64, x: f64) -> f64 {
       if e1 <= e0 {
           return if x >= e0 { 1.0 } else { 0.0 };
       }
       let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
       t * t * (3.0 - 2.0 * t)
   }
   ```
2. In `crates/amb-engine/src/lib.rs`, add `pub(crate) mod math;` with the other `mod` lines, in alphabetical order.
3. Delete each local `fn smoothstep` and import the shared one instead. The call sites stay as they are:
   - f32 files: `detail/util.rs`, `color.rs`, `effects.rs`, `merge.rs` get `use crate::math::smoothstep;`.
     - In `detail/`, the other files reached `util`'s copy through `use super::*` or `use super::util::…`.
     - Add `use crate::math::smoothstep;` to each file that the compiler says lacks it (`detail/sharpen.rs`).
   - f64 files: `lens.rs`, `mask.rs`, `negative.rs` get `use crate::math::smoothstep_f64 as smoothstep;`.
   - `retouch.rs` imports it from `mask` (`use crate::mask::{Frame, smoothstep};`). Change that to `use crate::mask::Frame;` and `use crate::math::smoothstep_f64 as smoothstep;`.
   - `tone.rs` has a one-argument `smoothstep(t)`. Delete it, add `use crate::math::smoothstep_f64;`, and write its one call line as:
     ```rust
     let ends = smoothstep_f64(0.0, 1.0, x / 0.04) * smoothstep_f64(0.0, 1.0, (1.0 - x) / 0.04);
     ```
   - Leave `plugins/tilt-shift`'s own copy: plugins are separate crates.
4. Check: `grep -rn "fn smoothstep" crates/amb-engine/src` shows only `math.rs`. Then the Rust checks.

Commit: `engine: one smoothstep`. Why: "Eight copies of it, two guarding equal edges; now one, guarded."

### ☑ S2. One `file_stamp` (2026-11-09)

Why: `amb-library`'s `catalog.rs` has a private copy of `amb_presets::linked::file_stamp`, the same function.

1. In `crates/amb-library/src/catalog.rs`, delete `fn file_stamp` and its doc comment.
2. Add `use amb_presets::linked::file_stamp;` with the file's other `use` lines.
3. Leave `crates/amb-gui/src/folder.rs`'s `file_stamp`: it returns other types.
4. The Rust checks.

Commit: `library: one file_stamp`. Why: "The catalogue had its own copy of amb_presets::linked::file_stamp."

### ☑ S3. `core.js`: the UI's shared helpers, loaded first (2026-11-09)

Why: the helpers every file uses lived in `app.js`, the last file to load. A few small helpers were copied between files.

1. Create `crates/amb-gui/ui/core.js` with the SPDX line and this header:
   ```js
   // The editor's shared helpers, loaded before every other script: the
   // backend (`invoke`), elements by id (`$`), the shared state (`ui`), notes
   // (`toast`, `status`), questions (`askYes`), building elements (`el`) and
   // paths (`fileName`, `folderName`, `dirOf`).
   "use strict";
   ```
2. Move these from `app.js` to `core.js`, unchanged and in this order:
   - `tauri`, `invoke`, `$`;
   - `ui`, with its comments;
   - from the helpers section: `toast`, `status` and `askYes`.
   - Leave the rest of the helpers section (`setStatusBar`, `wireStatusBar`, `decimals`, `format`, …) in `app.js`.
3. Move `el` from `masks.js` to `core.js`, unchanged, with its comment.
4. Move `fileName` from `collage.js` and `dirOf` from `filmstrip.js` to `core.js`, unchanged. Add after them:
   ```js
   // A folder's own name, also when the path ends with a slash.
   const folderName = (p) => p.split(/[\\/]/).filter(Boolean).pop();
   ```
5. In `index.html`, add `<script src="core.js"></script>` as the first of the `<script>` tags.
6. Replace the copies, matching only these exact shapes; leave anything else:
   - `X.split(/[\\/]/).pop()` → `fileName(X)`;
   - `X.replace(/^.*[\\/]/, "")` → `fileName(X)`;
   - `X.split(/[\\/]/).filter(Boolean).pop()` → `folderName(X)`. Keep anything after it, such as `?? folder`.
   - Find them with `grep -nE 'split\(/\[\\\\/\]/\)(\.filter\(Boolean\))?\.pop\(\)|replace\(/\^\.\*\[\\\\/\]/, ""\)' crates/amb-gui/ui/*.js`.
7. Update the docs that say these live in `app.js`: `grep -rn "app.js" docs AGENTS.md` (ARCHITECTURE.md names `askYes` and `ui`).
8. The Rust checks (the UI-script tests are Rust tests) and the UI tests.

Commit: `editor: core.js, the shared helpers, loaded first`. Why: "They lived in app.js, which loads last, and small path helpers were copied between files."

### ☑ S4. `store`: localStorage in one place (2026-11-09)

Why: 69 reads and writes of localStorage, most in their own `try` with `JSON.parse`.

1. Add to `core.js`, after `askYes`:
   ```js
   // The page's own storage (localStorage), which can be missing or full:
   // reads give `fallback` when there's nothing or it can't be read, and
   // failed writes are dropped. get/set store JSON; text/setText plain text.
   const store = {
     get(key, fallback = null) {
       try {
         const text = localStorage.getItem(key);
         return text === null ? fallback : JSON.parse(text);
       } catch {
         return fallback;
       }
     },
     set(key, value) {
       try { localStorage.setItem(key, JSON.stringify(value)); } catch {}
     },
     text(key, fallback = null) {
       try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
     },
     setText(key, text) {
       try { localStorage.setItem(key, text); } catch {}
     },
     remove(key) {
       try { localStorage.removeItem(key); } catch {}
     },
   };
   ```
2. Go file by file (`grep -n localStorage crates/amb-gui/ui/*.js`). Rewrite only these shapes, with the same key, and leave any other shape as it is:
   - `JSON.parse(localStorage.getItem(K)) ?? F`, alone or as the body of a `try` whose `catch` returns `F` → `store.get(K) ?? F`;
   - `localStorage.setItem(K, JSON.stringify(V))`, with or without a `try` around it that ignores errors → `store.set(K, V)`;
   - `localStorage.getItem(K)` read as plain text (compared with a string, like `=== "1"`) → `store.text(K)`;
   - `localStorage.setItem(K, S)` where `S` is a string, not JSON → `store.setText(K, S)`;
   - `localStorage.removeItem(K)` → `store.remove(K)`.
3. **Never change a key's format.** A key written as plain text stays plain text: people's saved choices are in it. If one key is read as JSON in one place and as text in another, leave both.
4. The Rust checks and the UI tests.

Commit: `editor: localStorage through store`. Why: "Sixty-nine reads and writes, most in their own try and JSON.parse."

## Phase 2: split big files

Each task splits one file (two in F3) along its sections. Sizes are production lines on 2026-11-09.

### ☑ F1. amb-gui `main.rs`: config, state and the handler list out (2026-11-09)

Why: `main` was 460 lines: startup, then the list of 232 Tauri commands. The saved settings and the app's state are their own concerns.

1. **`crates/amb-gui/src/config.rs`** (new). Move from `main.rs`:
   - `ImportChoice`, `Config`, `EditIn` with `prophoto`, `sixteen` and its `Default`, `MaskEffect`, `BUILTIN_MASK_EFFECTS`, `save_config`;
   - two new functions, cut from the `.setup(|app| { … })` closure in `main`:
     ```rust
     /// The saved settings at `path`, or the defaults.
     pub(crate) fn read_config(path: &Path) -> Config { … }
     /// Puts the saved settings into the app's state, and gives back the
     /// custom interface's folder, which `main` opens.
     pub(crate) fn apply_config(state: &AppState, config: Config) -> Option<PathBuf> { … }
     ```
     - `read_config`'s body is the `let config: Config = std::fs::read_to_string(&path)…unwrap_or_default();` expression.
     - `apply_config`'s body is the run of `if let Ok(mut …) = state.….lock() { … }` blocks, from `preset_dirs` to `metadata_templates`, then `config.interface` as its result.
   - In `main`, those lines become:
     ```rust
     let config = config::read_config(&path);
     let state = app.state::<AppState>();
     let interface = config::apply_config(&state, config);
     ```
     The interface block then reads `if let Some(dir) = interface.filter(…)`.
2. **`crates/amb-gui/src/state.rs`** (new). Move from `main.rs`: `AppState`, `LensCell` with its `impl`, `empty_lens`, `save_open_edits`.
3. **The handler list.** In `crates/amb-gui/src/commands/mod.rs`, add:
   ```rust
   /// Every command the web UI can call.
   pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
       tauri::generate_handler![
           // … the list from main.rs …
       ]
   }
   ```
   - Move the list's entries from `main` unchanged, except their paths:
     - `commands::photo::x` becomes `photo::x`;
     - `slideshow::take_slideshow` becomes `crate::slideshow::take_slideshow`;
     - `uitest::…` becomes `crate::uitest::…`.
   - In `main`, the call becomes `.invoke_handler(commands::handler())`.
4. **In `main.rs`:**
   - add `mod config;` and `mod state;`;
   - add `use config::{…};` and `use state::{…};` naming each moved item, so `use crate::*;` in the command files still finds them;
   - start both new files with `use crate::*;`, like the command files.
5. **Docs:**
   - `docs/recipes.md` ("Add a kind of hub item", step 2) and `docs/ARCHITECTURE.md` say commands are registered "in `invoke_handler`" in `main.rs`. Write `handler` in `crates/amb-gui/src/commands/mod.rs` instead.
   - The recipes test checks the file and the name.
6. Check: `main.rs` is under 400 lines. Then the Rust checks, and the UI tests (they start the app).

Commit: `editor: main.rs split into config, state and the handler list`. Why: "main was 460 lines, most of it the command list and the saved settings."

### ☑ F2. amb-gui `session.rs` → `session/` (2026-11-09)

Why: 1,855 lines, the largest file. The open photo's rendering, tools, retouch and AI contexts are separate concerns, and the presets list isn't about the session at all.

1. `git mv crates/amb-gui/src/session.rs crates/amb-gui/src/session/mod.rs`.
2. Move into new files (all `Session` methods, each file with its own `impl Session` block):
   - `session/render.rs`:
     - methods: `develop`, `preview`, `preview_masked`, `effective_settings`, `region`, `region_draft`, `shown_pixel`, `shown_patch`, `render_upright`, `compare_page`;
     - free functions: `to_srgb8`, `display`, `mark_gamut`, `mark_clipping`, `encode_jpeg`.
   - `session/tools.rs`: `mask_frame`, `crop_state`, `level_angle`, `guide_lines`, `crop_commands`, `pick_film_base`, `pick_white`, `white_balance_from_as_shot`, `guided_upright`, `upright_guides`, `negative_analysis`, `negative_frame`, `auto`, `match_look`.
   - `session/retouch.rs`: `find_dust`, `select_object`, `select_words`, `remove_object`, `scratch_spots`, `spot_source`, `stroke_source`.
   - `session/context.rs`: `feedback_context`, `assistant_context`, `plugin_context`.
   - `session/tests.rs`: the tests block.
3. Move to a new **`crates/amb-gui/src/preset_files.rs`**: `PresetEntry`, `save_preset`, `default_preset_dirs`, `scan_presets` (with `walk` and `is_photo_sidecar` inside it).
   - Add `mod preset_files;` in `main.rs`.
   - Change the callers' `session::save_preset` (and the others) to `preset_files::…`, including the `use session::{…, PresetEntry, …}` line in `main.rs` (after F1, also in `config.rs` or `state.rs`).
   - In `session/tests.rs`, add `use crate::preset_files::*;` if the preset tests need it.
4. `mod.rs` keeps the types (`LensSources` … `Preview`), `sidecar_path`, `edit_copy_path`, `ratio_text`, and the methods not listed above (`open`, `info`, `state_info`, `execute`, `save_sidecar`, `export`, …).
5. Check: each file is under 800 lines. Take `session.rs` off `OVER_BUDGET`. Then the Rust checks and the UI tests. Update the mentions of `session.rs` that point at moved functions (`grep -rn "session.rs" docs`).

Commit: `editor: session.rs split by concern`. Why: "1,855 lines: rendering, tools, retouch and AI contexts now have their own files, and the presets list its own module."

### ☑ F3. amb-gui `commands/photo.rs` and `commands/library.rs` (2026-11-09)

Why: 956 and 826 lines of commands for several parts of the editor.

1. New **`commands/select.rs`**, from `photo.rs`: `spot_source`, `find_dust`, `remove_object`, `of`, `select_object`, `select_words`, `selection_shadow`, `selection_apply`, `heal_line`, with the types only they use.
2. New **`commands/tools.rs`**, from `photo.rs`: `crop_state`, `mask_frame`, `mask_sliders`, `crop_commands`, `pick_white`, `pick_film_base`, `guided_upright`, `upright_guides`, `negative_freeze`, `level_angle`, `guide_lines`, `negative_frame`.
3. New **`commands/batch.rs`** (work on many photos), from `library.rs`: `library_find_duplicates`, `library_best_in_bursts`, `negative_sync`, `paste_groups`, `paste_settings`, `export_photos`, `merge_hdr`, `merge_pano`, `auto_tone_merged`, `contact_sheet`, `convert_raws`.
4. Each new file starts like its neighbours:
   - the SPDX line;
   - a `//!` line saying what's in it;
   - `use crate::*;`.
   - Add `pub(crate) mod …;` lines in `commands/mod.rs`, alphabetically.
5. In `handler()` (after F1), change the moved commands' paths, e.g. `photo::select_object` → `select::select_object`. The names the UI calls don't change: Tauri uses the function's name.
6. Update the `//!` line at the top of `photo.rs` and `library.rs` to say what's left. Take `commands/photo.rs` and `commands/library.rs` off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: photo and library commands split by area`. Why: "956 and 826 lines; selecting, the tools and batch work now have their own files."

### ☑ F4. amb-gui `looks.rs` → `looks/` (2026-11-09)

Why: 1,243 lines. Validating a layout or theme is half of it.

1. `git mv crates/amb-gui/src/looks.rs crates/amb-gui/src/looks/mod.rs`. Fix the `include_str!` paths in `BUILTIN_LAYOUTS` and `BUILTIN_THEMES` (one more `../`).
2. `looks/check.rs`:
   - the section from `// ---- validation` to `// ---- storage`: `Catalog`, `catalog`, `is_wheel_section`, `panel_composite`, `Checker` and its `impl`, `check_layout`, `is_hex_colour`, `check_theme`, `parse_hex`, `hex`, `mix`, `luminance`, `contrast`, `complete_theme`, `check`;
   - and `check_files`, from the end of the file.
3. `looks/setups.rs`: the `// ---- setups` section: `SETUP_FORMAT`, `Setup`, `SetupPart`, `setup_path`, `setups`, `save_setup`, `restore_setup`, `delete_setup`.
4. `looks/tests.rs`: the tests block.
5. `mod.rs` keeps the constants, the types, the storage section, `delete`, `import` and `export_text`.
6. Take `looks.rs` off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: looks.rs split into checks, setups and storage`. Why: "1,243 lines, half of it validation."

### ☑ F5. amb-gui `hub.rs` → `hub/` (2026-11-09)

Why: 1,193 lines: the index, each item kind's fetch and install, submission links and the hub's CI check.

1. `git mv crates/amb-gui/src/hub.rs crates/amb-gui/src/hub/mod.rs`. Fix any `include_str!` path.
2. `hub/kinds.rs`: what fetches or installs one kind:
   - collage, border and size items: `SizeProfile`, `ProfileSize`, `SIZES_FORMAT`, `fetch_sizes`, `fetch_collage`, `fetch_border`, `install_collage`, `install_border`, `install_sizes`;
   - presets: `PRESET_FOLDER`, `fetch_preset`, `install_preset`, `BUNDLED_PRESETS`, `LibraryPreset`, `preset_library`, `install_library_preset`, `installed_presets`;
   - plugins and interfaces: `fetch_plugin_manifest`, `fetch_interface_manifest`, `interface_folder_name`, `install_interface`, `installed_plugins`, `install_plugin`;
   - links: `link_candidates`, `install_link`;
   - and the public functions between them.
3. `hub/submit.rs`: `url_encode`, `submit_preset_url`, `submit_url`, and the public item just above `url_encode`.
4. `hub/check.rs`: `check_hub`.
5. `hub/tests.rs`: the tests block.
6. `mod.rs` keeps the constants, `HubItem`, `Index`, `HubEntry`, `known_kind`, `HubSource`, `parse_json`, `find_item`, `source_of`, `kind_of`, the fetch functions (`fetch`, `fetch_large`, `fetch_with`, `fetch_checked`), `item_url`, `read_index`, `fetch_item`, `installed`, `Mine`, `browse` and `install`.
7. Update `docs/recipes.md` ("Add a kind of hub item" names `crates/amb-gui/src/hub.rs` with `known_kind`, `fetch_item` and `browse`; those stay in `hub/mod.rs`). Take `hub.rs` off `OVER_BUDGET`. Then the Rust checks.

Commit: `editor: hub.rs split by kind`. Why: "1,193 lines; each item kind's fetch and install, the submission links and the CI check now have their own files."

### ☑ F6. amb-protocol `state.rs` → `state/` (2026-11-09)

Why: 1,210 lines for `DevelopState`: history, snapshots, sidecar reading and writing, executing, planning and queries.

1. `git mv crates/amb-protocol/src/state.rs crates/amb-protocol/src/state/mod.rs`.
2. Move `DevelopState` methods into new files, each with its own `impl DevelopState` block:
   - `state/snapshots.rs`: `snapshots`, `snapshot`, `manage_snapshot`, `snapshots_for_sidecar`, `restore_snapshots`.
   - `state/files.rs`: `from_xmp_str`, `restore_history`, `history_for_sidecar`, `to_xmp`, `to_json`, `from_json_str`, `load`, `load_or_new`, `save`, `save_merged`.
   - `state/plan.rs`: `plan`.
   - `state/query.rs`: `query`, `info_for`, `param_json`.
3. The free helpers after the `impl` (`known`, `known_group`, `group_of`, `has_extension`, `set_or_remove`, `preset_value`, `apply_amount`, `after_set`, `diff_maps`, `entry_from_json`):
   - put each in the one new file that uses it;
   - if two files use it, keep it in `mod.rs` as `pub(super)`.
4. `mod.rs` keeps the types (`Change` … `DevelopState`), `HISTORY_BUDGET`, `fingerprint`, and `new`, `from_settings`, `settings`, `get`, the undo and redo accessors, `execute`, `adopt`, `execute_all`, `execute_grouped`.
5. `docs/recipes.md` says a protocol command is done "in `execute`" in `crates/amb-protocol/src/state.rs`. Make it `crates/amb-protocol/src/state/mod.rs`, and the planning `crates/amb-protocol/src/state/plan.rs` if the recipe names `plan`. Take `state.rs` off `OVER_BUDGET`. Then the Rust checks: the protocol's tests are in `crates/amb-protocol/tests/`, and every other crate builds on it.

Commit: `protocol: state.rs split by concern`. Why: "1,210 lines; snapshots, sidecar files, planning and queries now have their own files."

### ☑ F7. amb-engine `mask.rs` → `mask/` (2026-11-09)

Why: 1,497 lines: the mask model, the sky, drawing shapes and applying local adjustments. It already has sections.

1. `git mv crates/amb-engine/src/mask.rs crates/amb-engine/src/mask/mod.rs`.
2. Cut along the `// ----` sections:
   - `mask/sky.rs`: the `sky` section;
   - `mask/geometry.rs`: the `geometry` section (`Frame`, `draw_shape`, `sample_colours`, `draw_color_range`, `draw_correction`, …);
   - `mask/apply.rs`: the `applying` section (`draw`, `draw_in`, `reach`, `blend`, the `apply_…` functions);
   - `mask/tests.rs`: the tests block.
3. `mod.rs` keeps everything before the `sky` section.
4. Re-export what other files use, so `crate::mask::Frame` and the others still work: `pub use geometry::Frame;` and so on, as the compiler asks.
5. Take `mask.rs` off `OVER_BUDGET`. Then the Rust checks.

Commit: `engine: mask.rs split along its sections`. Why: "1,497 lines: the model, the sky, geometry and applying now have their own files."

### ☑ F8. amb-engine `negative.rs` → `negative/` (2026-11-09)

Why: 1,254 lines: settings, the analysis of a scan, colour casts, finding the frame and the conversion.

1. `git mv crates/amb-engine/src/negative.rs crates/amb-engine/src/negative/mod.rs`.
2. Move:
   - `negative/analysis.rs`: `stock`, `downsample`, `percentile`, `density`, `log_exposure`, `top_median`, `estimate_base`, `analyze`, `analyze_as`, `samples_grid`, `detailed`, `auto_density_grade`, `levels`, `midtone`, `normalise`;
   - `negative/cast.rs`: `remove_cast`, `crossover`, `uncross`;
   - `negative/frame.rs`: `detect_frame`;
   - `negative/tests.rs`: the tests block.
3. `mod.rs` keeps the types and their defaults (everything before `stock`), `resolve`, and the prepared conversion (its `new` and `apply`).
4. Take `negative.rs` off `OVER_BUDGET`. Then the Rust checks.

Commit: `engine: negative.rs split by concern`. Why: "1,254 lines; the analysis, casts and finding the frame now have their own files."

### ☑ F9. `library.js` (2026-11-09)

Why: 1,488 lines: the library's state, grid, thumbnails, selection, panel, batch actions and wiring.

1. `library-grid.js`: the sections `filter, sort, grid` and `thumbnails`.
2. `library-actions.js`: the section `open, copy and paste, export, collage`.
3. `library.js` keeps the rest: its state, `show, load`, `selection`, `the panel`, `wiring`.
4. In `index.html`, add `library-grid.js` and `library-actions.js`, in that order, right after `library.js`.
5. `docs/recipes.md` names `VIEWER_BUTTONS` in `crates/amb-gui/ui/library.js`; it stays there. Take `library.js` off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: library.js split into grid and actions`. Why: "1,488 lines; the grid and thumbnails, and the actions on many photos, now have their own files."

### ☑ F10. `collage.js` (2026-11-09)

Why: 1,452 lines in ten sections.

1. `collage-canvas.js`: the sections `geometry`, `images`, `canvas`.
2. `collage-free.js`: the section `free arrangement`.
3. `collage-tray.js`: the sections `layouts`, `canvas form`, `tray`.
4. `collage.js` keeps the rest: its state, `export`, `collage files`, `open, close, edit`.
5. In `index.html`, add the three files, in that order, right after `collage.js`. Take `collage.js` off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: collage.js split by part`. Why: "1,452 lines; the canvas, free arrangement and the tray now have their own files."

### ☑ F11. `app.js` (2026-11-09)

Why: 1,188 lines (less after S3): slider rows, History, opening and saving photos, and the start.

1. `rows.js`: from the `panels` section, everything from its marker up to (not including) `function historyLabel`: `TRACKS`, `BAND_COLOURS`, `RAINBOW`, `trackFor` … `makeRow`.
2. `history.js`: from `function historyLabel` to the end of the `panels` section: History and snapshots.
3. `photo.js`: the section `photo` (opening, the folder's photos, the film base, the lens database, saving, before and after).
4. `app.js` keeps the rest: what S3 left of the helpers, `state & commands`, `the selected slider`, `feedback`, `wiring` with `init`.
5. In `index.html`, add `rows.js`, `history.js` and `photo.js`, in that order, **just before** `app.js`. `app.js` must stay the last script: its last line starts the app. Their top-level lines may only use names from earlier files (after S3, `ui` and the helpers are in `core.js`).
6. Update the docs that place these in `app.js` (`grep -rn "app.js" docs`). `docs/recipes.md` says a toolbar button's click handler goes "in `init`" in `crates/amb-gui/ui/app.js`; `init` stays there. Take `app.js` off `OVER_BUDGET` if it's under 700 lines. Then the Rust checks and the UI tests.

Commit: `editor: app.js split into rows, history and photo`. Why: "The slider rows, History and opening photos had grown into the start-up file."

### ☑ F12. `masks.js` (2026-11-09)

Why: 1,082 lines: the masks' state, geometry, drawing, panel, drafts, tools.

1. `mask-draw.js`: the sections `geometry` and `drawing`.
2. `mask-panel.js`: the sections `panel` and `drafts`.
3. `masks.js` keeps the rest: its top, `state`, `tools`, `enter / leave`.
4. In `index.html`, add `mask-draw.js` and `mask-panel.js`, in that order, right after `masks.js`. Take `masks.js` off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: masks.js split into drawing and the panel`. Why: "1,082 lines; drawing and the panel now have their own files."

### ☑ F13. `layout.js` and `editor.js` (2026-11-09)

Why: 1,009 and 989 lines.

1. `columns.js`, from `layout.js`:
   - from the comment `// A side column can be folded away` to the line before the `theme` section;
   - that is, the column widths, folding, top bar, chrome and column tabs.
2. `looks-dialog.js`, from `layout.js`: the section `Appearance dialog`.
3. `editor-keys.js`, from `editor.js`: the section `keyboard`.
4. `editor-inspector.js`, from `editor.js`: the sections `the Add list`, `the inspector`, `the edit bar`.
5. In `index.html`, add:
   - `columns.js` and `looks-dialog.js` right after `layout.js`;
   - `editor-keys.js` and `editor-inspector.js` right after `editor.js`.
6. `docs/recipes.md` names `TOOLBAR_IDS` in `layout.js` and `TOOL_TITLES` in `editor.js`.
   - Check with `grep -n "TOOLBAR_IDS\|TOOL_TITLES" crates/amb-gui/ui/*.js` that they stayed in those files.
   - If not, correct the recipe.
7. Take both off `OVER_BUDGET`. Then the Rust checks and the UI tests.

Commit: `editor: layout.js and editor.js split`. Why: "About 1,000 lines each; the columns, the Appearance dialog, editor mode's keys and its inspector now have their own files."

Later, when the budget asks:
- `amb-ai` `net.rs`: each layer kind's step in its own file;
- `amb-protocol` `masks.rs`: parts and painting;
- `amb-engine` `lens.rs`: the camera-space and output-space steps.

## Phase 3: structure

These change how the code is organised, not only where it sits. The owner says go first. Then a strong model writes each one's exact steps here, in the same form as above, and smaller models carry them out.

- **R1. `amb-ops`, the batch photo operations, out of `amb-library`.**
  - Move `convert`, `replay`, `merge`, `object`, `snap`, `burst`, `dust`, `negative` and `look` into a new crate that depends on `amb-library`.
  - `amb-library` then no longer needs `amb-gpu` or `amb-export` (check `contact`: `search.rs` uses `contact::small`, so `contact` stays, or that helper moves first).
  - The editor and `amb` change their paths, from `amb_library::convert` to `amb_ops::convert`.
- **R2. `Prefs`: one lock for the saved settings.**
  - `AppState`'s saved-settings fields become one `Mutex<Config>`.
  - `save_config` writes it as it is, with no copying.
  - Commands lock `state.prefs` and change a field.
  - Do it one field at a time; the settings file's format stays the same.
- **R3. UI folders.** Group `ui/` into `core/`, `develop/`, `tools/`, `library/`, `dialogs/`, `help/` (the tests already read subfolders, task G1). Then update:
  - the paths in `index.html`;
  - `crates/amb-gui/src/commands/menu.rs`'s test, which reads `menu.js` and `keys.js` by name;
  - `docs/recipes.md`;
  - the docs.
- **R4. Docs with one home per fact** ([design/structure.md](design/structure.md), "Target shape").
  - `NEXT.md`'s "Built" list becomes a link to `FEATURES.md`, after anything only it says moves to `FEATURES.md` or `ARCHITECTURE.md`.
  - "Recently done" in `WHATS-NEXT.md` keeps the last month; older entries go to the release notes.

## Found while cleaning

Bugs or oddities noticed during a task, to look at separately:

- (none yet)
