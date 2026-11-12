# Cleanup cards

Cards for the cleanup that keeps the codebase lean ([design/structure.md](design/structure.md), [codestyle.md](codestyle.md)). Each is written so that any coding agent, a small model included, can do it with no other context, in the card format of [delegation.md](delegation.md).

- One card, one commit. Nothing the app does changes unless the card says so.
- Who may do a card: the model tier on its `Tier:` line, or a higher one.
- A done card is deleted from this file in the commit that does it (git keeps it), so the file only lists open work.
- The 2026-11-09 cleanup (27 tasks: small merges, 16 file splits, the `amb-ops` crate, one settings lock, UI folders) is done. Read it with `git log --grep "cleanup" -- docs/cleanup.md` and in [design/structure.md](design/structure.md).

## Rules for every task

1. **One task, one commit.**
   - Do only what the task says: no fixes, renames or reformatting on the side.
   - If you notice a bug, add one line about it under "Found while working" at the end of this file, in the same commit.
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
8. **Delete the card from this file**, in the same commit.
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

On a machine short of disk, add `--release` to every cargo command and use `target/release` in the UI test lines, keeping one build folder.

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

## Cards

Do the groups in order, or any card whose "Precondition" line is met. Ids are stable: other docs and commit messages name them. A card is ready when a model that has never seen the repo can do it; if one turns out not to be, stop and write the question under "Needs a brief" ([delegation.md](delegation.md), "When to stop and hand up").

### Group 0: finish first

Work that was planned and not finished. Do these before the groups below.

### 0d. The agent primer points at the other guides
Tier: medium        Risk: low
Why: an agent started from Connect an AI app is told about photos only, not about making layouts, themes, plugins or an interface.
Files: `crates/amb-protocol/src/guides.rs` (`primer`), `crates/amb-gui/src/commands/connect.rs` (`agent_line`, `sh_quote`; read only).
Steps:
1. In `primer`, add one sentence to the `format!` string, before `{open}`: `Other guides: amb docs make-it-yours (layouts, themes and the hub, for people), amb docs layouts, amb docs plugins and amb docs interface (making them).` Keep it a plain sentence.
2. The primer reaches the shell through `agent_line` in `connect.rs`: on Windows it removes every `"` and `%`, elsewhere it is wrapped by `sh_quote` (single quotes, `'` escaped). The new sentence has none of `"`, `'`, `%` and no backtick or `$`, so the quoting holds. Keep it that way.
3. Add a test in `guides.rs`'s `tests` module: `primer(Path::new("/g"), None)` contains each of `amb docs make-it-yours`, `amb docs layouts`, `amb docs plugins`, `amb docs interface`, and contains none of `"`, `'`, `%`, `` ` ``, `$`. Each of those four names must be found by `find(...)`: `make-it-yours`, `layouts`, `plugins`, `interface` are guide names in `GUIDES`.
Don't: change `GUIDES`, the guides' text or `agent_line`.
Checks: the three cargo commands; `cargo test -p amb-protocol -p amb-gui`.
Done when: the primer names the four guides and the test passes.
Commit: `ai: the agent primer points at the layout, theme, plugin and interface guides`. Why: "An agent started from Connect was told about photos only."

### 0e. The guard tests the code style names
Tier: medium        Risk: low
Why: `docs/codestyle.md` lists these tests as holding its rules; they do not exist yet, and the UI's old idioms grew in three days although a helper existed. Three files, one commit each.
Files: `crates/amb-gui/src/checks.rs` (new), `crates/amb-gui/src/main.rs`, `crates/amb-cli/tests/code_health.rs` (new), `crates/amb-cli/tests/docs_health.rs` (new).
Steps, first commit (`crates/amb-gui/src/checks.rs`):
1. In `main.rs`, below `mod uitest;`, add `#[cfg(test)] mod checks;`. In `checks.rs`: the SPDX line, a `//!` line ("Guard tests: the lists a feature is added to, and the old idioms whose counts may only fall (docs/codestyle.md)"), and its own helpers: `ui_text(ext)` returning every file under `crates/amb-gui/ui/` (all subfolders) with that extension, as `(path from ui/, text)`, and `src_text()` for every `.rs` under `crates/amb-gui/src/`. The helpers in `main.rs`'s `tests` module are private to it; do not move them.
2. Test `every_tauri_command_is_in_the_handler`: take each name after `#[tauri::command]` (the next `fn` or `async fn`, past other attribute and `///` lines) in `src_text()`; take the entries of `tauri::generate_handler![ ... ]` in `crates/amb-gui/src/commands/mod.rs` (the last `::` part of each, before the comma). The two sets must be equal. They are equal today (240 each).
3. Test `the_ui_calls_only_registered_commands`: every `invoke("name"` in the UI's `.js` files and in `crates/amb-gui/tests/ui/*.js` is in that handler list. True today (232 names).
4. Test `every_wire_function_is_called`: every top-level `function wireX(` (also `async function`) in the UI has its name somewhere else in the UI's scripts, as a call or as `addEventListener("DOMContentLoaded", wireX)`. `wirePreferJpeg` (`crates/amb-gui/ui/develop/filmstrip.js`) and `wireToolbarOverflow` (`crates/amb-gui/ui/core/toolbar.js`) are started that way: they pass.
5. Test `old_idioms_may_only_fall`: count over the UI's `.js` files (and for the last, `index.html`) and assert `count <= LIMIT`, each limit a `const` at the top with a comment "lower it when you remove some": `Object.assign(document.createElement(` 381; `innerHTML` 15; `style="` in `ui/index.html` 17; top-level `let` (lines starting `let `) 7; `toast(String(` 176; `localStorage` outside `crates/amb-gui/ui/core/core.js` 8. The failure message says which idiom and which helper replaces it (`el`, `textContent` or `el` children, `class` in the stylesheet, a `const` state object, `showError`, `store`).
6. Run the UI tests too (`main.rs` is not changed, but the build is).
Steps, second commit (`crates/amb-cli/tests/code_health.rs`):
7. New file with the SPDX line and a `//!` line. Helper `root()` as in `crates/amb-cli/tests/recipes.rs`.
8. Test `every_usage_command_has_an_arm`: take each `amb NAME` that starts a line (after four spaces) in the `USAGE` text of `crates/amb-cli/src/usage.rs`; `crates/amb-cli/src/main.rs` must contain `"NAME"` in a match pattern. True today for every name (`preset`, `render`, `do`, `edit`, ...).
9. Test `every_features_command_is_in_usage`: each `` `amb NAME `` in `docs/FEATURES.md` has NAME among the usage names. True today.
10. Test `counts_may_only_fall`: count `#[allow(` and `#![allow(` in `crates/*/src` and `plugins/*/src` before each file's first `#[cfg(test)]` line (the `code_size.rs` rule; skip `/tests/` and `tests.rs`): at most 16. Count bare `.unwrap()` the same way: at most 1 today (`crates/amb-engine/src/raw.rs`); another card may lower it first, so measure with the same awk or loop and use the number you get.
Steps, third commit (`crates/amb-cli/tests/docs_health.rs`):
11. New file. Test `every_doc_is_in_the_map`: every `docs/**/*.md` is linked from `docs/README.md`, except `docs/research/NN-*.md` (numbered, dated), `docs/release-notes/*`, `docs/audit-2026-11-12/*` while it exists, and `docs/README.md` itself.
12. Test `relative_links_resolve`: every `](path)` or `](path#anchor)` in `README.md`, `AGENTS.md`, `CONTRIBUTING.md` and `docs/**/*.md` that is not a URL names a file that exists (relative to that file). Skip anchors (an anchor check is a later card).
13. Test `every_crate_is_named`: each folder in `crates/` is named (as `` `crates/NAME` `` or `NAME`) in `AGENTS.md`, `README.md` and `docs/ARCHITECTURE.md`. It fails today where the docs lack a crate (`amb-ops` in `README.md`; `amb-plugin` and `amb-plugin-sdk` in `docs/ARCHITECTURE.md`): add the missing rows to those docs in this commit.
Don't: change what a test counts afterwards without lowering; add a dependency (read files with `std::fs`).
Checks: the three cargo commands; `cargo test -p amb-gui checks`, `cargo test -p amb-cli --test code_health --test docs_health`.
Done when: each test fails when you add one violation (try it once, then undo).
Commits: `gui: tests for the handler list, the wire calls and the old idioms`; `cli: tests for usage against dispatch, and the counts that may only fall`; `docs: tests for the map, links and crates`.

### 0f. UI helpers `showError` and `plural`, and the rest of the `store` move
Tier: medium        Risk: low
Why: 176 `toast(String(e), true)` and about 130 hand-made plurals; 8 `localStorage` uses left outside `core.js`.
Files: `crates/amb-gui/ui/core/core.js`, `crates/amb-gui/src/checks.rs` (limits), `crates/amb-gui/ui/looks/columns.js`, `crates/amb-gui/ui/library/slideshow.js`, `crates/amb-gui/ui/library/library.js`, `crates/amb-gui/ui/help/tips.js`, every `.js` that `grep -rln "toast(String(" crates/amb-gui/ui` lists.
Steps:
1. In `core.js`, after `toast`, add:
   ```js
   // An error from a command, as a red toast.
   function showError(e) {
     toast(String(e), true);
   }

   // "1 photo", "3 photos": `many` defaults to `one` + "s".
   function plural(n, one, many = `${one}s`) {
     return `${n} ${n === 1 ? one : many}`;
   }
   ```
2. Replace each `toast(String(e), true)` and `toast(String(err), true)`: a statement `toast(String(e), true);` becomes `showError(e);`; `.catch((e) => toast(String(e), true))` becomes `.catch(showError)`; `return toast(String(e), true);` becomes `return showError(e);`. A `catch` whose parameter is then unused is `catch {` only if it is not used elsewhere. Leave toasts with their own text (a message with the error in it, such as the one in `crates/amb-gui/ui/looks/editor.js` that starts "Not saved").
3. Replace the plurals that match exactly `${N} WORD${N === 1 ? "" : "s"}` (N a name or a short expression like `r.errors.length`, WORD one word) by `${plural(N, "WORD")}`. Leave the ones that do not match this shape. Example: `crates/amb-gui/ui/library/import.js` line 103, `crates/amb-gui/ui/library/library-grid.js` line 195.
4. `store` (it exists in `core.js`: `get`, `set`, `text`, `setText`, `remove`). Keep every key and the JSON shape:
   - `crates/amb-gui/ui/looks/columns.js` `columnTab(side)` and `showColumnTab`: `store.get("amb.tabs", {})` and `store.set("amb.tabs", all)`.
   - `crates/amb-gui/ui/library/slideshow.js` `slideshowSettings`: `{ ...d, ...store.get("amb.slideshow", {}) }`.
   - `crates/amb-gui/ui/library/library.js` `savedMore` and `saveMore` (key `SAVED_MORE`): `store.get(SAVED_MORE, {})` and `store.set(SAVED_MORE, all)`.
   - `crates/amb-gui/ui/help/tips.js` `wireTips`: `store.get("amb.used", [])`.
5. In `checks.rs` lower the limits for `toast(String(` and `localStorage` outside `crates/amb-gui/ui/core/core.js` to what remains (0 if you did all).
Don't: rename a key; change a message; touch the plural sites that differ in shape.
Checks: the UI checks of "Checks" (`cargo build -p amb-gui -p amb-cli`, all three UI scripts); `cargo test -p amb-gui`.
Done when: `grep -rn "localStorage" crates/amb-gui/ui` shows only `crates/amb-gui/ui/core/core.js`, and `grep -rn "toast(String(" crates/amb-gui/ui` prints nothing.
Commit: `ui: showError and plural helpers; localStorage only in store`. Why: "The old idioms were growing."

### Group A: small

### A1. Shared dependencies declared once
Tier: small        Risk: low
Why: six dependencies are declared with their own version in several crates, so a bump takes 3 to 6 edits.
Files: `Cargo.toml` (root), `crates/amb-ai/Cargo.toml`, `crates/amb-engine/Cargo.toml`, `crates/amb-export/Cargo.toml`, `crates/amb-gui/Cargo.toml`, `crates/amb-library/Cargo.toml`, `crates/amb-ops/Cargo.toml`, `crates/amb-plugin/Cargo.toml`.
Steps:
1. In root `Cargo.toml`, under `[workspace.dependencies]`, add:
   ```toml
   sha2 = "0.10"
   ureq = { version = "2", features = ["native-certs"] }
   rayon = "1"
   flate2 = "1.1"
   jpeg-encoder = "0.6"
   image = { version = "0.25", default-features = false }
   ```
2. `sha2`: in amb-ai, amb-gui, amb-library, amb-plugin replace `sha2 = "0.10"` by `sha2.workspace = true`.
3. `ureq`: in amb-ai, amb-gui, amb-library replace the `ureq = { version = "2", features = ["native-certs"] }` line by `ureq.workspace = true`.
4. `rayon`: in amb-engine, amb-export, amb-gui, amb-library, amb-ops, amb-plugin replace `rayon = "1"` by `rayon.workspace = true`.
5. `flate2`: in amb-ai and amb-export replace `flate2 = "1.1"` by `flate2.workspace = true`.
6. `jpeg-encoder`: in amb-export and amb-gui replace `jpeg-encoder = "0.6"` by `jpeg-encoder.workspace = true`.
7. `image` keeps its features at the use site, with the same feature lists as today:
   - amb-engine: `image = { workspace = true, features = ["png", "tiff", "jpeg", "webp"] }`
   - amb-export: `image = { workspace = true, features = ["png", "tiff", "webp", "avif", "exr", "rayon"] }`
   - amb-library and amb-ops: `image = { workspace = true, features = ["jpeg"] }`
   - amb-gui (dev-dependencies): `image = { workspace = true, features = ["png"] }`
8. amb-ai: replace `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` by `serde.workspace = true` and `serde_json.workspace = true`.
9. Look at `git diff Cargo.lock`.
Don't: change any feature list; add or remove a dependency; run `cargo update`. amb-export asked for `image` 0.25.10: the lock already holds it, so it must not change.
Checks: the three cargo commands of "Checks". `git diff Cargo.lock` is empty (or only reorders). `cargo tree -d` shows no new duplicate.
Done when: `grep -n 'sha2 = \|ureq = \|rayon = \|flate2 = \|jpeg-encoder = ' crates/*/Cargo.toml` prints nothing.
Commit: `build: declare shared dependencies once in the workspace`. Why: "A version bump was 3 to 6 edits."

### A2. Named constants for bare numbers in recent code
Tier: small        Risk: low
Why: thresholds typed inline can't be told apart or tuned, and the kaleidoscope's limits repeat its manifest.
Files: `crates/amb-ops/src/cleanup.rs`, `crates/amb-ai/src/denoise.rs`, `plugins/kaleidoscope/src/lib.rs` (and its `plugin.wasm`).
Steps:
1. `crates/amb-ops/src/cleanup.rs`, next to the other tuning constants (`BLURRY`, `SOFT_SHARE`, `SHOOT`), add, each with a one-line comment saying what it is:
   - `const DARK_LEVEL: u8 = 16;` a channel value below this counts as black. Used in `see`: `v < 16` becomes `v < DARK_LEVEL`.
   - `const BRIGHT_LEVEL: u8 = 245;` above this counts as blown out. `v > 245` becomes `v > BRIGHT_LEVEL`.
   - `const DARK_SHARE: f32 = 0.9;` share of black pixels that makes a frame dark; `const BRIGHT_SHARE: f32 = 0.6;` same for blown out (lower because a blown sky still has some detail). In the line `let (dark, bright) = (s.black > 0.9, s.white > 0.6);` use them.
   - `const BURST_SECONDS: f64 = 2.0;` and use it in `impl Default for Options` for `burst_seconds`.
2. `crates/amb-ai/src/denoise.rs`: add `const MIN_WEIGHT: f32 = 1e-3;` (floor of the tile feather weight) for `w.clamp(1e-3, 1.0)`, and `const FLAT_MEAN: f64 = 1e-12;` for `mean.abs() > 1e-12`.
3. `plugins/kaleidoscope/src/lib.rs`: add `SLICES: (f64, f64) = (2.0, 32.0)`, `ZOOM_MIN: f64 = 0.05`, `DEPTH: (f64, f64) = (1.0, 8.0)` for the `clamp(2.0, 32.0)`, `.max(0.05)` and `clamp(1.0, 8.0)` calls in `kaleidoscope`, and `FOLD_SCALE: f32 = 1.7`, `FOLD_SHIFT: f32 = 0.55` for `p[0] * 1.7 - 0.55` and `p[1] * 1.7`. Comment that the first three repeat the manifest's ranges.
4. Rebuild the plugin: `cargo run -p amb-cli -- plugin build plugins/kaleidoscope`, and stage its `plugin.wasm` only if it changed.
Don't: change a value; move code; touch other files.
Checks: the three cargo commands; `cargo test -p amb-ops -p amb-plugin -p amb-ai`.
Done when: none of the literals above remain in those lines.
Commit: `ops: name the cleanup, denoise and kaleidoscope thresholds`. Why: "Tuning values were bare numbers; the plugin's limits copy its manifest."


### A3. UI test scripts per feature
Tier: small        Risk: low
Why: `crates/amb-gui/tests/ui/smoke.js` is 1,100 lines and `organise.js` grows with every library feature, so most feature commits touch them and agents working at once conflict. CI runs every script in the folder (`for t in crates/amb-gui/tests/ui/*.js`, `.github/workflows/ci.yml`), each in a fresh editor on a fresh photo, so a new script needs no other change.
Files: `crates/amb-gui/tests/ui/smoke.js`, `crates/amb-gui/tests/ui/organise.js`, new scripts in the same folder. The helpers are in `crates/amb-gui/ui/core/uitest.js` (`t.step`, `t.until`, `t.click`, `t.key`, `t.expect`, `t.historyLength`, `t.setting`).
Steps:
1. Each new script starts with the SPDX line, two comment lines (what it tests; `// Run: ambrotype --ui-test crates/amb-gui/tests/ui/NAME.js photo.dng`), then the steps moved from the old script, unchanged, in their old order. A moved step that depends on state an earlier smoke step made fails in the new script: add the line it needs at the top of its script (a `t.until` for the photo, `await toggleLibrary(true)`, `await switchTask("develop")`) and nothing else. Run each new script alone.
2. From `smoke.js` into `trippy.js`: the step "Trippy, a task plugin: its tools put effects on and take them off". Begin the script with the first step of `smoke.js` ("the photo opens and a preview shows"), copied. In the moved step, replace the end that returns to the Exposure step (the line `for (let i = 0; i < 7; i++) await run([{ cmd: "undo" }]);` and the `t.until("back at the Exposure step", ...)` below it) by:
   ```js
   while (ui.state.can_undo) await run([{ cmd: "undo" }]);
   await t.until("History is empty", () => t.historyLength() === 0);
   ```
   This is the "hard-coded undo count": the 7 was the number of steps the trip tiles made.
3. From `smoke.js` into `retouch.js` (same first step): "Spot panel: Find dust finds none on a clean chart; Shift-drag heals a line as one step" and "Remove object: Shift+Q opens the tool; a box sends its corners, a click its point; Esc cancels".
4. From `smoke.js` into `negative.js` (same first step): "Negative: Auto Density and Auto Grade are switches beside Auto" and "Pick base: a click on the photo stores the film base; Alt-click forgets it".
5. From `organise.js` into `cleanup.js`: "Clean up finds a rejected photo; K keeps it, D lets it go". Its start is the first two steps of `organise.js` (the photo, "the library shows the photo"), copied.
6. From `organise.js` into `connect.js`: "Connect an AI app has the built-in guides and starts an agent" (first step: the photo only).
7. From `organise.js` into `denoise.js`: "AI Denoise offers its model, then a new DNG per raw" (it begins with `await toggleLibrary(true); select(first, "only");`, so give the script `const first = await t.until("the photo", () => ui.info?.path, 60000);` first).
8. Update the two old files' header comments so they no longer list what moved.
Don't: change a step's checks; rename a step; leave a step in both files.
Checks: the UI checks of "Checks" (all scripts, each prints its steps and how many passed, no `FAILED` line); the new scripts' step counts plus the old ones' equal the old total.
Done when: `smoke.js` has no `< 7` loop of undos; each moved step passes in its new script.
Commit: `tests: UI steps per feature`. Why: "Two files held every feature's steps, and agents conflicted in them."

### A4. `el()` instead of `Object.assign(document.createElement(...))`
Tier: small        Risk: low
Why: 381 sites build elements the old way although `el` is in `crates/amb-gui/ui/core/core.js`; the count grows (316 to 381 in three days). Repeat this card once per file.
Files: one UI script per commit, in this order (sites on 2026-11-12): `crates/amb-gui/ui/library/insights.js` 39, `crates/amb-gui/ui/library/browse.js` 32, `crates/amb-gui/ui/library/organise.js` 22, `crates/amb-gui/ui/dialogs/hub.js` 21, `crates/amb-gui/ui/library/library-grid.js` 15, `crates/amb-gui/ui/dialogs/plugins.js` 14, `crates/amb-gui/ui/core/search.js` 13, `crates/amb-gui/ui/core/tasks.js` 12, `crates/amb-gui/ui/collage/collage-tray.js` 11, `crates/amb-gui/ui/library/duplicates.js` 10, `crates/amb-gui/ui/library/cull.js` 10, `crates/amb-gui/ui/help/basics.js` 10, `crates/amb-gui/ui/develop/fx.js` 10, `crates/amb-gui/ui/develop/frame.js` 9, `crates/amb-gui/ui/core/keys.js` 9. Then the rest, by `grep -rc "Object.assign(document.createElement(" crates/amb-gui/ui | sort -t: -k2 -nr`. Also `crates/amb-gui/src/checks.rs` (the limit; card 0e must be done first).
How `el` works: `el(tag, attrs = {}, ...children)`. `class` sets the class; a key starting with `on` adds that listener (`onclick`, `onchange`, `oninput`); `true` sets the attribute empty; `false`, `null` and `undefined` skip it; anything else is set with `setAttribute`; children are nodes or strings (`null` skipped).
Conversion rules, for each `Object.assign(document.createElement("tag"), { ... })`:
1. `className: X` becomes `class: X`.
2. `textContent: X` is removed from the object and `X` becomes a child: `el("span", { class: "a" }, X)`.
3. Properties that are attributes stay as keys: `type`, `title`, `value` (on a new element), `alt`, `role`, `id`, `min`, `max`, `step`, `label`, `src`, `loading`, `hidden`, `disabled`, `checked`, `selected`.
4. Rename to the attribute's name: `tabIndex` becomes `tabindex`, `ariaLabel` becomes `"aria-label"`.
5. `draggable: true` becomes `draggable: "true"` and `spellcheck: false` becomes `spellcheck: "false"` (an empty `draggable` attribute does not make an element draggable; an omitted `spellcheck` does not turn it off).
6. Event properties (`onclick = fn`, `{ onclick: fn }`) become `onclick: fn` keys (the listener is added, not assigned; code that assigns twice to replace a handler must keep its plain assignment).
7. These stay plain statements after the `el(...)`: `dataset.x = ...`, `style.x = ...`, `.value = ...` on an input that is already in the page, `.checked = ...` set later, `.innerHTML` (not to be added), `.append` of more children later.
8. A site with a computed tag (`document.createElement(ps ? "button" : "span")`) becomes `el(ps ? "button" : "span", {...})`.
9. `Object.assign(document.createElement(tag), attrs)` with attrs a variable (`crates/amb-gui/ui/library/slideshow.js`, its local `el` at line 302): leave it, and leave the local `el` there: it shadows the shared one inside its function. Move on.
10. Do not change what the element is, its order among siblings, or its text.
Steps for each file:
1. Convert every site in the file by the rules. Also convert the `document.createElement("x")` + separate property lines next to them only if they are the same element built in the same statement block; leave other code.
2. Count what is left: `grep -c "Object.assign(document.createElement(" FILE`.
3. In `crates/amb-gui/src/checks.rs`, lower the limit for `Object.assign(document.createElement(` by the number of sites you converted.
Don't: do two files in one commit; rename a CSS class or id; change a message.
Checks: the UI checks of "Checks" (build, all UI scripts, no `FAILED`); `cargo test -p amb-gui`; then open the dialog or panel you converted once by hand (`cargo run --release -p amb-gui`, docs/recipes.md for Xvfb) when no UI script covers it.
Done when: the file has no `Object.assign(document.createElement(` (except rule 9) and the limit equals the new total.
Commit: `ui: FILE uses el()`, with the file's name. Why: "One way to build an element."

### Group B: medium

How the splits below work. Each follows "Moving Rust code" and the rules above (move whole items with their doc comments, change only `mod` and `use` lines and visibility, keep every public path by re-exporting). The line numbers are from 2026-11-12: find an item by its name (`grep -n "fn NAME" FILE`), not by the number. Common steps for B1 to B8 and B9a to c:
- `git mv OLD.rs OLD/mod.rs` first, unless the card says the new files sit beside the old one.
- The file's `#[cfg(test)] mod tests { ... }` block moves to `OLD/tests.rs` without its wrapper; `mod.rs` gets `#[cfg(test)] mod tests;`. If the tests then need private items from a new file, add `use` lines in `tests.rs`, not new `pub`.
- Every item that was `pub` and is used outside the folder is re-exported from `mod.rs` with `pub use <part>::{Name, ...};` (list them with `grep -n "^pub " NEWFILE`). An item that was private and is now used by a sibling file becomes `pub(super)`.
- Take the file off `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs` (the test says so when it must go); a card 0c done earlier may have changed its number.
- Fix mentions of the old path (rule 7 above). `docs/ARCHITECTURE.md` and `docs/design/structure.md` name several of these files.
Checks for all of them: the three cargo commands, with the crate's own tests unchanged in count (`cargo test -p CRATE`, compare the "N passed" before and after).

### B1. Split `crates/amb-engine/src/retouch.rs`
Tier: medium        Risk: medium
Why: 979 production lines, over the budget of 800; its parts (heal, AI fill, donor search) do not share state.
Files: `crates/amb-engine/src/retouch.rs` to `crates/amb-engine/src/retouch/{mod,heal,inpaint,source,tests}.rs`; `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps: the module doc and `use` lines; `SpotShape`, `Spot`, `Spots`, `DEFAULT_FEATHER`, `impl Spots`, `Placed`, `place`, `sample`, `apply`, `apply_in`. It declares `mod heal; mod inpaint; mod source;` and re-exports `pub use inpaint::{INPAINT_SIZE, Inpainter, set_inpainter};` and `pub use source::{find_source, find_spot_source, footprints};`.
2. `heal.rs` (the clone and heal maths): `search_offset`, `heal_one`, `correction`, `push_pull`. `search_offset` and `heal_one` are used by `apply_in` and `source.rs`: `pub(super)`.
3. `inpaint.rs` (AI fill): `INPAINT_SIZE`, `Inpainter`, `INPAINTER`, `set_inpainter`, `inpainter`, `FILL_CONTEXT`, `FILL_MIN_HALF`, `fill_one`, `bilinear`, `ring_grain`, `gaussian`. `inpainter` and `fill_one` become `pub(super)`; `FILL_MIN_HALF` stays `pub(crate)` and is re-exported `pub(crate) use inpaint::FILL_MIN_HALF;` if the compiler says it is used outside.
4. `source.rs` (the donor search for new spots and the footprints of a region render): `footprints`, `find_source`, `find_spot_source`.
5. Tests to `tests.rs`.
Don't: change any signature or any number.
Checks: as above; `cargo test -p amb-engine`; and the UI tests (spots are used there).
Done when: no file in `retouch/` has more than 800 production lines and `retouch.rs` is gone.
Commit: `engine: split retouch.rs into heal, inpaint and source`. Why: "It was past the size budget."

### B2. Split `crates/amb-protocol/src/sidecar.rs`
Tier: medium        Risk: medium
Why: 960 production lines; reading a sidecar's info and writing it back are two jobs.
Files: `crates/amb-protocol/src/sidecar.rs` to `crates/amb-protocol/src/sidecar/{mod,read,write,tests}.rs`; `docs/design/structure.md` (names it); `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps the module doc, `use` lines, the namespace constants (`XMP_NS`, `DC_NS`, `LR_NS`, `EXIF_NS`, `LIBRARY_NS`), and the types and helpers both sides use: `Flag` with its `impl`, `PhotoInfo`, `StackRef`, `DC_TEXTS`, `impl PhotoInfo` (the first one, with `place`), `parse_place`, `parse_xmp_coordinate`, `xmp_coordinate`, `parse_rating`. It declares `mod read; mod write;` and re-exports `pub use read::{read_history, read_info, read_snapshots};` and `pub use write::{Stored, merge, merge_with_info, merge_with_stored, with_info, write_atomic};`.
2. `read.rs`: `read_history`, `read_snapshots`, `read_library_text`, `read_info`, and the second `impl PhotoInfo` (with `cleaned_keeping_keywords`).
3. `write.rs`: `escape_text`, `resolve`, `element_name`, `element_ns`, `list_items`, `apply_info`, `prefix_for`, `apply_library_text`, `Stored`, `Foreign`, `escape_attr`, `xml_err`, `collect_foreign`, `merge`, `merge_with_info`, `merge_with_stored`, `merge_parts`, `with_info`, `write_atomic`.
4. Tests to `tests.rs`; where they name a private helper that moved, add a `use super::write::NAME;` there.
Don't: change the XMP the code writes (the round-trip tests show it).
Checks: as above; `cargo test -p amb-protocol` and `cargo test -p amb-cli --test library` (they read sidecars).
Done when: `sidecar.rs` is gone and no part has more than 800 production lines.
Commit: `protocol: split sidecar.rs into read and write`. Why: "It was past the size budget."

### B3. Split `crates/amb-export/src/embed.rs`
Tier: medium        Risk: medium
Why: 932 production lines; the AVIF and JPEG XL containers are 600 lines of box handling.
Files: `crates/amb-export/src/embed.rs` to `crates/amb-export/src/embed/{mod,bmff,avif,jxl,tests}.rs`; `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps the module doc and `use crate::Format; use crate::exif::Tags;`, then `escape`, `xmp_packet`, `iptc_app13`, `jpeg_segment`, `embed_jpeg`, `crc32`, `embed_png`, `embed_tiff`, `tiff_with_entries`, `embed_webp` and `embed` (the dispatcher). Declares `mod bmff; mod avif; mod jxl;`.
2. `bmff.rs` (the box format AVIF and JPEG XL share): `Bmff`, `boxes`, `read_be`, `push_be`, `make_box`. All `pub(super)`.
3. `avif.rs`: `grown_iloc`, `Meta` and its `impl`, `embed_avif`, `Bits` and its `impl`, `av1_colour_bits`, `av1_set_colour`, `item_extents`, `avif_set_colour`, `avif_colour_box`. `embed_avif` is `pub(super)`; the others private to the file (the compiler says if not).
4. `jxl.rs`: `JXL_SIGNATURE`, `jxl_with_boxes` (`pub(super)`).
5. Tests to `tests.rs`.
Don't: change bytes written.
Checks: as above; `cargo test -p amb-export` (every container has a test).
Done when: `embed.rs` is gone and no part passes 800.
Commit: `export: split embed.rs by container`. Why: "It was past the size budget."

### B4. Split `crates/amb-ops/src/object.rs`
Tier: medium        Risk: medium
Why: 925 production lines: selecting an object, selecting by words, and turning a selection into a Remove spot.
Files: `crates/amb-ops/src/object.rs` to `crates/amb-ops/src/object/{mod,select,words,geometry,remove,tests}.rs`; `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps the module doc, `use` lines, the constants `EDGE`, `CONTEXT`, `WIDEN`, `MIN_WIDEN`, `SNAP_RADIUS`, `SNAP_EPS`, the types `Target`, `Selection`, `Object`, `Like`, and `find`, `frame`. It declares the four parts and re-exports `pub use select::select;` and `pub use words::select_words;` (the editor calls `object::select`, `object::select_words`, `object::find`).
2. `select.rs`: `select`.
3. `words.rs`: `GRID`, `NEAR`, `ALSO`, `select_words`.
4. `geometry.rs` (helpers used by selecting and by removing): `overlap`, `bounds`, `shadow_of`, `distance_inside`, `fill`, `fill_snapped`, `map_at`, `cover`.
5. `remove.rs`: `impl Selection` (the block with `WIDEN` and `MIN_WIDEN`, turning a selection into a Remove spot) and `dabs_inside`.
6. Tests to `tests.rs`.
Don't: change a threshold or an item's name.
Checks: as above; `cargo test -p amb-ops`.
Done when: `object.rs` is gone and no part passes 800.
Commit: `ops: split object.rs into select, words, geometry and remove`. Why: "It was past the size budget."

### B5. Split `crates/amb-engine/src/matching.rs`
Tier: medium        Risk: medium
Why: 908 production lines: measuring a look, solving for it, and the one-click Auto.
Files: `crates/amb-engine/src/matching.rs` to `crates/amb-engine/src/matching/{mod,stats,solve,auto,tests}.rs`; `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps the module doc, `use` lines, `Settings`, `Weights` with `impl Default`, `Part` with its `impl`, `MatchOptions` with `impl Default`, `MatchResult`, `develop_plain`. Declares `mod stats; mod solve; mod auto;` and re-exports every `pub` item of the three (`LookStats`, `look_stats`, `average`, `solve`, `auto`, ...: `grep -n "^pub " matching/*.rs`).
2. `stats.rs`: `L_PERCENTILES`, `LookStats` with its `impl`, `NEUTRAL_CHROMA`, `MIN_NEUTRAL_SHARE`, `zone_weights`, `percentile`, `look_stats`, `average`, `MIN_BAND_SHARE`, `residuals`, `distance`, `distance_with`, `weighted_distance`.
3. `solve.rs`: `Param`, `param`, `params`, `num`, `clamp_to_registry`, `Start`, `STAND_INS`, `impl Start`, `solve_linear`, `solve`, `solve_stage`, `MIN_MIXER_MOVE`, `PENALTY`.
4. `auto.rs`: `AUTO_ANCHORS`, `AUTO_TONE_PULL`, `AUTO_WHITE_PULL`, `AUTO_WHITE_SHARE`, `auto_target`, `auto`.
5. Tests to `tests.rs`.
Don't: change a weight or a constant.
Checks: as above; `cargo test -p amb-engine matching`; `cargo test -p amb-cli`.
Done when: `matching.rs` is gone and no part passes 800.
Commit: `engine: split matching.rs into stats, solve and auto`. Why: "It was past the size budget."

### B6. Split `crates/amb-engine/src/pipeline/settings.rs`
Tier: medium        Risk: medium
Why: 905 lines, of which the table of rendered tags is 180 and `from_settings` is 220.
Files: `crates/amb-engine/src/pipeline/settings.rs` to `crates/amb-engine/src/pipeline/settings/{mod,tags,from_settings}.rs`; `docs/recipes.md` (line 35 names it, and a test checks that); `crates/amb-cli/tests/code_size.rs`. Its tests are already in `pipeline/tests.rs`.
Steps:
1. `mod.rs` keeps the module doc and `use super::*;`, `WhiteBalance`, `UnsetDetail` with `impl Default`, `RENDERED_DEFAULTS`, `IncrementalWhiteBalance`, `BaseLook` with `impl`, `Base` with `impl`, `LookLut`, `DevelopSettings` with `impl Default`, the helpers `num`, `wheel`, `spline_of`, and the small `impl DevelopSettings` that holds `profile_note`. It declares `mod tags; mod from_settings;` and re-exports `pub use tags::{RENDERED_TAGS, is_rendered};` (as they were `pub`).
2. `tags.rs`: `RENDERED_TAGS`, `is_rendered`, `PANEL_SWITCHES`, `without_switched_off`, `LEGACY_TONE`, `with_legacy_tone`.
3. `from_settings.rs`: the large `impl DevelopSettings` (with `from_settings`), `flag`, `text`, `lens_corrections`, `transform_settings`, `calibration_settings`, `crop_settings`, `rgb_triple`, `crossover_tags`, `negative_settings`.
4. `num`, `wheel` and `spline_of` are used by the new files: they are `pub(super)` already, which now means "in `settings`": the new files see them through `use super::*;`. Items that `pipeline/mod.rs` or `camera.rs` used with `pub(super)` need `pub(super) use from_settings::NAME;` in `settings/mod.rs` (the compiler lists them).
5. `docs/recipes.md` line 35: the path becomes `crates/amb-engine/src/pipeline/settings/mod.rs` and the sentence says the field is read in `from_settings` (in `settings/from_settings.rs`); run `cargo test -p amb-cli --test recipes`.
Don't: change what `from_settings` reads.
Checks: as above; `cargo test -p amb-engine`; `cargo test -p amb-cli --test recipes`.
Done when: `pipeline/settings.rs` is gone and no part passes 800.
Commit: `engine: split settings.rs into tags and from_settings`. Why: "It was past the size budget."

### B7. Split `crates/amb-ai/src/lib.rs`
Tier: medium        Risk: medium
Why: 886 production lines: what a model is, which are installed and chosen, and running one.
Files: `crates/amb-ai/src/lib.rs` and new `crates/amb-ai/src/spec.rs`, `store.rs`, `model.rs`, `tests.rs`; `crates/amb-cli/tests/code_size.rs`. The three files sit beside `lib.rs` (it is the crate root: no folder).
Steps:
1. `lib.rs` keeps the crate doc, `mod` lines (`archive`, `net`, the public ones, and the new three), `TASKS`, `AUTO_INSTALL_BYTES`, `default_dir`, and re-exports `pub use spec::*; pub use store::*; pub use model::*;` (every item that was `pub` in `lib.rs` keeps its path `amb_ai::NAME`).
2. `spec.rs`: `Input`, `Pixels` with `impl`, `Layout` with `impl`, `Activation`, `Output`, `ModelSpec`, `ModelFile`, `impl ModelSpec`, `ModelState`, `CustomModel`, `imagenet_mean`, `imagenet_std`, `Catalogue`, `catalogue`. The `include_str!("../data/models.json")` in `catalogue` keeps its path (the file is still in `src/`).
3. `store.rs` (installed models and the user's choice): `known_task`, `file_of`, `is_installed`, `custom_specs`, `all_specs`, `find_spec`, `choices`, `write_choices`, `chosen`, `list`, `choose`, `sha256_file`, `install`, `download`, `remove`, `add_custom`, `ensure_model`.
4. `model.rs` (loading and running): `Model`, `load`, `run`, `fingerprint`, `finder`, `try_model`.
5. Tests to `tests.rs` (`#[cfg(test)] mod tests;` in `lib.rs`).
Don't: change a public name or the models list.
Checks: as above; `cargo test -p amb-ai`; `cargo clippy -p amb-cli -p amb-gui --all-targets -- -D warnings` (they use the crate).
Done when: `lib.rs` is under 200 lines and no part passes 800.
Commit: `ai: split lib.rs into spec, store and model`. Why: "It was past the size budget."

### B8. Split `crates/amb-gpu/src/nn.rs`
Tier: medium        Risk: medium
Why: 847 production lines: tensor types, the kernel table, and 440 lines of one method per op.
Files: `crates/amb-gpu/src/nn.rs` to `crates/amb-gpu/src/nn/{mod,ops,kernels,session,tests}.rs`; `docs/design/gpu.md` (names `src/nn.rs`); `crates/amb-cli/tests/code_size.rs`.
Steps:
1. `mod.rs` keeps the module doc and the `use` lines. It declares `mod ops; mod kernels; mod session;` and re-exports `pub use ops::*;` (`RANK`, `Tensor`, `Binary`, `Unary`, `Reduce`, `Pad`, `View`, `Operand`, `MatMul`, `Conv`, `contiguous`, `broadcast`), `pub use session::Session;` and `pub(crate) use kernels::Kernels;` (`device.rs` names `crate::nn::Kernels`).
2. `ops.rs`: `RANK`, `Tensor` with `impl`, `Binary`, `Unary` with `impl`, `Reduce`, `Pad`, `View` with `impl`, `Operand`, `MatMul`, `Conv` with `impl`, `contiguous`, `broadcast`.
3. `kernels.rs`: `Kernels`, `ENTRIES`, `impl Kernels`. The `include_str!("shaders/nn.wgsl")` in `Kernels::new` becomes `include_str!("../shaders/nn.wgsl")`. The fields of `Kernels` that `session.rs` reads become `pub(super)`.
4. `session.rs`: `BATCH`, `Session`, `impl Gpu` (`nn`, `nn_max_floats`, `nn_kernels`), `size_class`, `impl Session<'_>`, `impl Drop for Session<'_>`.
5. Tests to `tests.rs`.
Don't: touch the WGSL.
Checks: as above; `cargo test -p amb-gpu` (the GPU tests skip without an adapter: say so) and `cargo test -p amb-ai`.
Done when: `nn.rs` is gone and no part passes 800.
Commit: `gpu: split nn.rs into ops, kernels and session`. Why: "It was past the size budget."

### B9a. Split `crates/amb-engine/src/raw.rs` before it passes 800
Tier: medium        Risk: low
Why: 798 production lines, 2 under the budget: the next feature trips the size test. Its metadata reading and its embedded previews are separate jobs.
Files: `crates/amb-engine/src/raw.rs` to `crates/amb-engine/src/raw/{mod,metadata,preview}.rs`.
Steps:
1. `git mv crates/amb-engine/src/raw.rs crates/amb-engine/src/raw/mod.rs`. Its two test modules (`thumbnail_tests`, `matrix_tests`, after the production code) stay in `mod.rs` as they are; do not make a `tests.rs`.
2. `metadata.rs`: `Ratio`, `Gps`, `PhotoMetadata`, `calibration_matrices`, `color_matrix`, `read_shot`, `read_metadata`. `mod.rs` declares `mod metadata;` and re-exports `pub use metadata::{Gps, PhotoMetadata, Ratio, read_metadata, read_shot};` (add `calibration_matrices`, `color_matrix` as `pub(super)` or `pub(crate)` as the compiler asks: `load_file` uses them).
3. `preview.rs`: `embedded_thumbnail`, `raw_size`, `embedded_preview`, `embedded_jpeg`, `upright`. `mod.rs` declares `mod preview;` and re-exports `pub use preview::{embedded_preview, embedded_thumbnail, raw_size};`. `upright` is also used by `impl CfaExt for rawler::RawImage` (which stays in `mod.rs`): `pub(super)`.
4. The rest (`CameraImage`, `RAW_EXTENSIONS`, `is_raw_path`, `load`, `load_file`, the demosaic helpers, `downscale`, `clip_levels`, `orientation_code`, both `impl CameraImage`, `CfaExt`) stays.
Don't: change a public path (`amb_engine::raw::load`, `raw::PhotoMetadata`, `raw::embedded_thumbnail` are used in 7 crates).
Checks: as the common checks of Group B; `cargo test -p amb-engine`; `cargo clippy --workspace --all-targets -- -D warnings`.
Done when: `mod.rs` is under 650 production lines.
Commit: `engine: split raw.rs into metadata and previews`. Why: "It was 2 lines under the size budget."

### B9b. Split `crates/amb-library/src/catalog.rs` before it passes 800
Tier: medium        Risk: low
Why: 791 production lines, 9 under the budget.
Files: `crates/amb-library/src/catalog.rs` to `crates/amb-library/src/catalog/{mod,query,prints,tests}.rs`.
Steps:
1. `git mv crates/amb-library/src/catalog.rs crates/amb-library/src/catalog/mod.rs`; its `mod tests` block to `tests.rs`.
2. `query.rs`: `Entry`, `Query`, `impl Query`. `mod.rs` declares `mod query;` and `pub use query::{Entry, Query};`.
3. `prints.rs`: `Print`, and the two `impl Catalog` blocks after the main one (the one with `store_prints` and the one with `store_vectors`). `mod.rs`: `mod prints; pub use prints::Print;`. The methods that `mod.rs`'s code or other files call become `pub(super)` or stay `pub` as they were. `key` (the free function after them) stays in `mod.rs`; the children see it.
4. Everything else (`VERSION`, `Shot`, `Record`, `Stamp`, `stamp`, `read`, `shot_of`, `impl Record`, `FolderCount`, `Indexed`, `INDEX_CHUNK`, `Stats`, `Catalog`, `default_path`, the main `impl Catalog`, `folder_and_prefix`, `reachable`) stays.
Don't: change SQL or the schema `VERSION`.
Checks: as the common checks of Group B; `cargo test -p amb-library`.
Done when: `mod.rs` is under 700 production lines.
Commit: `library: split catalog.rs into query and prints`. Why: "It was 9 lines under the size budget."

### B9c. Split `crates/amb-engine/src/dehaze.rs` before it passes 800
Tier: medium        Risk: low
Why: 788 production lines, 12 under the budget; 200 of them are filter helpers.
Files: `crates/amb-engine/src/dehaze.rs` to `crates/amb-engine/src/dehaze/{mod,filters,tests}.rs`.
Precondition: card 0a is done (then `lerp_position` is in `math.rs` and not here).
Steps:
1. `git mv crates/amb-engine/src/dehaze.rs crates/amb-engine/src/dehaze/mod.rs`; its `mod tests` block to `tests.rs`.
2. `filters.rs`: `downscale`, `resample`, `lerp_position_f`, `transpose`, `min_filter`, `min_filter_rows`, `min_row`, `guided_coefficients`, `deterministic_mean`. They are used by `mod.rs`: `pub(super)`. Add `use super::*;` and `use rayon::prelude::*;`.
3. Everything else (`DehazeStrengths`, `apply`, `Haze`, `apply_haze`, `sanitise`, `apply_with`, `Estimate` and its `impl`, `estimate_factor`, `ApplyNumbers`, `apply_numbers`, `Recovery` and its `impl`, `fog_colour`, `dot`, `guide_value`, `airlight`, `cap_channels`) stays.
Don't: change a constant in `DehazeStrengths`.
Checks: as the common checks of Group B; `cargo test -p amb-engine dehaze`; `cargo test -p amb-gpu`.
Done when: `mod.rs` is under 650 production lines.
Commit: `engine: split dehaze.rs, filters out`. Why: "It was 12 lines under the size budget."

### B10. Narrow `pub` to `pub(crate)` where only the crate uses it
Tier: medium        Risk: low
Why: `docs/codestyle.md` says `pub(crate)` unless another crate uses it; 39 items in three crates are `pub` and used nowhere outside.
Files: `crates/amb-ops/src`, `crates/amb-ai/src`, `crates/amb-library/src`. Find each item by name: other cards move some of them (B7, B9b).
Steps:
1. Narrow these functions, constants and methods (verified on 2026-11-12: the word appears in no other crate under `crates/` or `plugins/`, and in no crate's own `tests/`):
   - `amb-ops`: `dust::spots_for`, `dust::spot_command`, `merge::default_output`, `merge::pano_output`, `denoise::default_output`, `snap::box_mean`, `burst::openness`, `convert::CAMERA_JPEG_EDGE`, `convert::AT_ONCE`, `negative::analyze_file`, `negative::SCENE_GAP`, `negative::ODD_GAP`.
   - `amb-ai`: `search::similarity`, `try_model`.
   - `amb-library`: `gpx::MAX_GAP`, `Track::locate` (in `gpx.rs`), `folder::is_raw`, `duplicates::dhash`, `places::LOOK_ALIKE`, `lensdb::SERVER`, `lensdb::VERSION`, `Catalog::records_counted`, `Catalog::store_prints`, `Catalog::store_vectors`, `lrcat::plan_contents`, `insights::bucket_of`.
2. Build after each crate (`cargo check -p CRATE --all-targets`). Re-grep an item first if the build says it is used elsewhere; put it back to `pub` then and say so in the commit.
3. Then try the types: `cleanup::Reason`, `cleanup::Seen`, `burst::Ranked`, `negative::RollFrame`, `search::SearchSpec`, `ModelSpec`, `ModelFile`, `gpx::Track`, `lrcat::LrCollection`, `lrcat::Contents`, `lrcat::PlannedPhoto`, `lrcat::PlannedCollection`, `insights::Tally`. A type that appears in the signature of a `pub` item must stay `pub` (the compiler warns "private type in public interface", and clippy with `-D warnings` fails): put it back. Narrow the rest.
Don't: change anything else; make an item `pub` that was not.
Checks: the three cargo commands (unused-item warnings catch an item that was only `pub` to hide that nothing used it: if so delete nothing, make it `pub(crate)` and add `#[cfg(test)]` only when tests alone use it, and say so).
Done when: `grep -rn "^pub \(fn\|const\) \(spots_for\|dhash\|is_raw\)" crates` prints nothing.
Commit: `crates: pub(crate) for what only the crate uses`. Why: "Public items that nobody outside uses are API we must keep."

### B11. One error type for editor commands
Tier: medium        Risk: medium
Why: 306 `.map_err(|e| e.to_string())` in `crates/amb-gui/src` (267 followed by `?`); with one error type `?` does it.
Files: `crates/amb-gui/src/error.rs` (new), `crates/amb-gui/src/main.rs` (the alias `type Res<T> = Result<T, String>;`, line 60, and `mod error;`), and every file the compiler then flags in `crates/amb-gui/src`. Do it in one commit, when no other work is in flight in `amb-gui`.
Steps:
1. New `error.rs` (SPDX line, a `//!` line: "The error of an editor command: one type, so `?` converts what a library returns"):
   ```rust
   /// What an editor command returns on failure. The page receives it as the plain text.
   pub(crate) struct CmdError(String);

   // Not `Display` or `Error` itself: that is what lets every other error convert.
   impl<E: std::fmt::Display> From<E> for CmdError {
       fn from(e: E) -> Self {
           CmdError(e.to_string())
       }
   }

   impl serde::Serialize for CmdError {
       fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
           s.serialize_str(&self.0)
       }
   }

   // For the functions that still return `Result<_, String>`.
   impl From<CmdError> for String {
       fn from(e: CmdError) -> String {
           e.0
       }
   }

   impl CmdError {
       pub(crate) fn text(&self) -> &str {
           &self.0
       }
   }

   pub(crate) type Res<T> = Result<T, CmdError>;
   ```
2. In `main.rs`: add `mod error;`, delete the old `type Res<T> = Result<T, String>;` and write `pub(crate) use error::{CmdError, Res};` (the files import `crate::Res` or get it from `use crate::*;`: they keep working).
3. `cargo check -p amb-gui --all-targets`. The compiler flags, and you fix:
   - `Err(msg)` where `msg: String`, `Err(format!(...))` and `Err("...".into())` in a function returning `Res`: they compile with `?`-style conversion only through `.into()`: write `Err(format!(...).into())`; for a `String` variable `Err(msg.into())`; a `&str` literal `Err("...".into())` works as it is.
   - places that read the error's text (match arms, `format!("{e}")`, `e.contains(...)`, a `Res` value passed where `Result<_, String>` is wanted): use `e.text()`, or `.map_err(String::from)`.
4. Delete the `.map_err(|e| e.to_string())` that is followed by `?`, in functions that return `Res`: `grep -rn "map_err(|e| e.to_string())?" crates/amb-gui/src`. Do it with one search-and-replace over the folder (`.map_err(|e| e.to_string())?` to `?`), then build. Where the compiler then says "the trait `From<X>` is not implemented for `String`" or similar, the function returns `Result<_, String>` explicitly (88 of them): put the `.map_err(|e| e.to_string())` back there. Functions that return `Result<_, String>` are not changed.
5. `spawn_blocking(...).await.map_err(|e| e.to_string())??` loses its `map_err` the same way (`.await??`), when the function returns `Res`.
Don't: change a message's text (the page still shows the same strings); change `Result<_, String>` functions' signatures; rename `Res`.
Checks: the three cargo commands (and `grep -rc "map_err(|e| e.to_string())" crates/amb-gui/src` falls from 306); the UI checks of "Checks" (the page must still get plain strings: a UI step that triggers an error, such as opening a missing path, shows the same text).
Done when: `grep -rn "map_err(|e| e.to_string())?" crates/amb-gui/src` is limited to functions that return `Result<_, String>`.
Commit: `gui: one error type for editor commands`. Why: "Every command repeated map_err to turn an error into text."

### B12a. Best in bursts moves to `amb-ops`
Tier: medium        Risk: low
Why: the editor's `library_best_in_bursts` (`crates/amb-gui/src/commands/batch.rs`) groups the bursts, looks at each photo and ranks it: logic that `amb` cannot reach (`amb duplicates` lists bursts unranked), so the same feature differs by front end.
Files: `crates/amb-ops/src/burst.rs`, `crates/amb-gui/src/commands/batch.rs`.
Steps:
1. In `burst.rs` add (with `use amb_library::catalog::Catalog; use amb_library::duplicates::{self, Kind}; use rayon::prelude::*; use std::path::{Path, PathBuf}; use std::sync::atomic::{AtomicUsize, Ordering};`):
   - `#[derive(Debug, Clone, Serialize)] pub struct BurstPhoto { pub path: PathBuf, pub score: f32, pub note: String }` and `#[derive(Debug, Clone, Serialize)] pub struct Bursts { pub bursts: Vec<Vec<BurstPhoto>>, pub faces: bool }`: the same fields as the editor's `BurstPhoto` and `Bursts`, so the JSON the page gets does not change.
   - `pub fn best_in_bursts(catalog: &Catalog, photos: &[PathBuf], burst_seconds: f64, thumb: &(dyn Fn(&Path, u32) -> Option<image::RgbImage> + Sync), progress: &(dyn Fn(usize, usize) + Sync)) -> Bursts`. Its body is the closure body of `library_best_in_bursts`, unchanged except: the grouping call uses `&|p| thumb(p, 256)` as its `Thumb`; the options are `duplicates::Options { burst_seconds: burst_seconds.max(0.1), max_bits: 0 }`; the groups are those whose `kind == Kind::Burst`; each photo is looked at from `thumb(p, 1600)` (a missing preview gives `Look { sharp: 0.0, eyes: None, faces: 0 }`); the `faces` flag is `amb_engine::subject::faces(&[0.0; 8 * 8 * 3], 8, 8).is_ok()`; where the editor emitted `bursts-progress`, call `progress(n, total)`.
2. In `batch.rs`, `library_best_in_bursts` keeps its signature and doc: it builds `cache`, the `thumb` closure (embedded preview, then `folder::thumbnail`, then `decode_jpeg`: as now), then calls `amb_ops::burst::best_in_bursts(folder::catalog(), &photos, burst_seconds, &thumb, &|done, total| { let _ = app.emit("bursts-progress", [done, total]); })` and returns the result. Delete the editor's `BurstPhoto` and `Bursts` structs and use the ops ones as the return type.
3. A test in `burst.rs`: `best_in_bursts` over no photos (a `Catalog::in_memory()`, `&[]`, a `thumb` that returns `None`) gives no bursts. The behaviour with photos is covered by the UI test that calls `library_best_in_bursts` (`crates/amb-gui/tests/ui/browse.js`).
Don't: change the event name `bursts-progress` or the JSON fields (`bursts`, `faces`, `path`, `score`, `note`).
Checks: the three cargo commands; `cargo test -p amb-ops -p amb-gui`; the UI tests (`browse.js` calls `library_best_in_bursts`).
Done when: `library_best_in_bursts` has no ranking code of its own.
Commit: `ops: best in bursts moves out of the editor`. Why: "The logic belongs where `amb` can use it too."

### B12b. One read-change-write of a photo's info
Tier: medium        Risk: medium
Why: reading a sidecar's info, changing it and writing it back is written four times (`write_info` in `crates/amb-cli/src/organise.rs`, `change_info` in `crates/amb-gui/src/library.rs`, `set_info` in `crates/amb-cli/src/mcp.rs`, `Session::set_info` in `crates/amb-gui/src/session/mod.rs`), each with its own handling of a Lightroom catalogue opened in place.
Files: `crates/amb-library/src/info.rs` (new), `crates/amb-library/src/lib.rs`, `crates/amb-gui/src/library.rs`, `crates/amb-cli/src/organise.rs`, `crates/amb-cli/src/mcp.rs`.
Steps:
1. New `crates/amb-library/src/info.rs` (`pub mod info;` in `lib.rs`), with a `//!` line "A photo's rating, label, flag, keywords and text, read from and written to its sidecar". Move into it, unchanged and with their doc comments, from `crates/amb-gui/src/library.rs`: `InfoChange` (with its `Deserialize` derive and `impl`: `turns`, `base`, `apply`) and `change_info`. Add:
   ```rust
   /// A photo's info as its sidecar says (the default when it has none).
   pub fn read_info(photo: &Path) -> PhotoInfo { ... }  // the body of organise.rs read_info
   /// Write `info` into the photo's sidecar, keeping everything else in it, and return what was written.
   pub fn write_info(photo: &Path, info: &PhotoInfo) -> Result<PhotoInfo, String> { ... }  // the body of organise.rs write_info
   ```
   and make `change_info` call `read_info`/`write_info` instead of repeating their lines.
2. `crates/amb-gui/src/library.rs`: delete the moved items and write `pub use amb_library::info::{InfoChange, change_info};` so `library::InfoChange` and `library::change_info` (used by `commands/library.rs`, `commands/places.rs`, `commands/files.rs`) keep working.
3. `crates/amb-cli/src/organise.rs`: `read_info` and `write_info` become `pub(crate) use amb_library::info::{read_info, write_info};` (they are imported by `cleanup.rs`, `places.rs` and others from `organise`).
4. `crates/amb-cli/src/mcp.rs` `set_info`: keep its argument parsing (it reports a bad flag, which `InfoChange` does not); replace its last lines (`sidecar::with_info`, `write_atomic`, `read_info` of the written text) by `let back = amb_library::info::write_info(p, &info)?;` and the `sidecar` path in the answer by `amb_engine::sidecar_path(p)`. Read the start of `set_info` the same way: `amb_library::info::read_info(p)`.
5. `Session::set_info` (editor, the open photo) stays: it writes the session's own sidecar and keeps the session's state.
6. A test in `info.rs`: write a rating and a keyword to a test photo's sidecar through `change_info`, read them back with `read_info`; a second `change_info` with `remove_keywords` takes the keyword out.
Don't: change `InfoChange`'s fields or serde names (the page sends them); change what `amb tag` or `amb cleanup` write.
Checks: the three cargo commands; `cargo test -p amb-library -p amb-gui -p amb-cli`; the UI tests (the library rates and flags).
Done when: `with_info(` appears in `crates/amb-library/src/info.rs` and `crates/amb-gui/src/session/mod.rs` only (and tests).
Commit: `library: one read and write of a photo's info`. Why: "Four copies of the same three lines, each handling catalogues its own way."

### B12c. Batch export through `amb_ops::export_batch`
Tier: medium        Risk: medium
Precondition: `crates/amb-ops/src/export_batch.rs` exists (another change adds it, used by `amb render`). If it does not, stop: do not write it here. Read its public function and its options and report type first.
Why: the editor (`export_many` and `unique_path` in `crates/amb-gui/src/library.rs`, called from `commands/batch.rs`) and `amb render` (`render_many` in `crates/amb-cli/src/render.rs`) each name and loop; the naming rules differ (the editor skips names that exist on disk, `amb` only those taken in the batch).
Files: `crates/amb-ops/src/export_batch.rs` (read), `crates/amb-gui/src/library.rs`, `crates/amb-gui/src/commands/batch.rs`, `crates/amb-cli/src/render.rs`, `crates/amb-cli/src/convert.rs` and `crates/amb-cli/src/share.rs` only if they loop over many photos the same way.
Steps:
1. If `render_many` already calls `export_batch`, do not touch it. If not, replace its naming loop (`template`, `taken`, `-2` names) with the call and print its report as the old code printed (`[i/n] ...`, "failed: ...", the final "N of M failed" error). Its text output is checked by the tests in `crates/amb-cli/tests/`.
2. Editor: `export_many` keeps its signature (its caller in `commands/batch.rs` and the page do not change); its body calls `export_batch`, giving it the per-photo render as a closure that opens the `Session`, sets `settings.tags` from `s.info` when `include_tags` is set, and calls `s.export(&dest, &settings, lens)`, exactly as the loop body does now. Delete `unique_path` when nothing else uses it (`grep -rn unique_path crates`).
3. The editor must keep avoiding names that already exist on disk. If `export_batch` has an option for that, pass it. If it has none, stop and hand up (the card does not choose a new rule for either front end).
4. `ExportedMany` (`written`, `errors`, serialised to the page): keep it as the editor's return type, filled from the batch report, or use the report type if its serialised fields are the same two names.
Don't: change a file name the editor or `amb` would have produced.
Checks: the three cargo commands; `cargo test -p amb-ops -p amb-gui -p amb-cli`; the UI tests (`browse.js` exports a selection).
Done when: no naming loop of `-2` names remains in `crates/amb-gui/src` or `crates/amb-cli/src/render.rs`.
Commit: `ops: the editor and amb render export many photos through export_batch`. Why: "Two batch exporters named files by two rules."

### B13. Mask sliders from the schema
Tier: medium        Risk: medium
Why: the ranges of the mask panel's sliders are typed in `crates/amb-gui/ui/tools/mask-panel.js` and again, as `clamp` literals, in `crates/amb-protocol/src/masks/fields.rs` and `crates/amb-protocol/src/masks/mod.rs`: two copies free to drift (`docs/codestyle.md`: no range or label typed in a script).
Files: `crates/amb-protocol/src/masks/fields.rs`, `crates/amb-protocol/src/masks/mod.rs`, `crates/amb-gui/src/commands/tools.rs` (where `mask_sliders` is), `crates/amb-gui/src/commands/mod.rs` (the handler list), `crates/amb-gui/ui/tools/mask-panel.js`, `crates/amb-gui/ui/tools/masks.js` (loads `mk.sliders` from `mask_sliders`, near line 452), `crates/amb-gui/src/checks.rs` (if card 0e is done).
Steps:
1. In `fields.rs` add `pub struct FieldRange { pub key: &'static str, pub label: &'static str, pub min: f64, pub max: f64, pub step: f64, pub default: f64 }` (`Serialize`) and `pub const FIELD_RANGES: &[FieldRange]` with exactly these rows, which are the ranges in use today:
   - `Feather` 0 to 100, step 1, default 50 (a radial part and a brush: `fields.rs` clamps `Feather` to 0..100; `paint_mask` clamps `feather` to 0..100);
   - `Angle` -180 to 180, step 1, default 0;
   - `ColorAmount`, label `Refine`, 0 to 100, step 1, default 50;
   - `Flow` 1 to 100, step 1, default 100;
   - `Density` 1 to 100, step 1, default 100;
   - `amount`, label `Amount`, 0 to 100, step 1, default 100.
2. Use the same table in the clamps: replace the literals in the `"Angle"`, `"Feather"`, `"ColorAmount"` and `"amount"` arms of `fields.rs` and the `flow`, `density`, `feather` clamps in `masks/mod.rs` by `range("Angle")` and so on, a small function that returns `(min, max)` from `FIELD_RANGES`. Values must come out the same: the existing tests of `masks/` show it.
3. In `commands/tools.rs` add `#[tauri::command] pub(crate) fn mask_field_ranges() -> Vec<amb_protocol::masks::fields::FieldRange>` (re-export `FieldRange` and `FIELD_RANGES` from `masks/mod.rs` if `fields` is private), list it in `handler` in `commands/mod.rs`.
4. In `masks.js` load it where `mk.sliders` is loaded (`mk.ranges = await invoke("mask_field_ranges")`, a new field of `mk`), and in `mask-panel.js` build these `sliderRow`s from it with a small `maskRange(key)` that returns `{ min, max, step }` and the default: the radial `Feather` and `Angle` (lines 188 and 189), the color `Refine` (212), the brush `Feather`, `Flow`, `Density` (318 to 320) and the mask's `Amount` (228). Labels and `title`s that are not in the table stay in the script.
5. These stay typed in `mask-panel.js` because they are not Rust field ranges: the luminance `From`, `To` (0 to 100, a percent view of `LumRange`), `Smoothness` (0 to 50), and the brush `Size` (0.5 to 25, narrower than the 0.1 to 50 that Rust accepts). Add a comment saying so above each.
6. A test in `checks.rs` (or `main.rs`'s tests): every `FIELD_RANGES` key that `mask-panel.js` builds comes through `maskRange(`: `mask-panel.js` has no `min: 0, max: 100` for `Feather` (grep for `label: "Feather", min`).
Don't: change a range or a default; change a sidecar value (the values written stay the same).
Checks: the three cargo commands; `cargo test -p amb-protocol -p amb-gui`; the UI tests (`smoke.js` uses masks); then try the mask panel by hand: radial, luminance, color, brush.
Done when: `grep -n 'label: "Feather", min' crates/amb-gui/ui/tools/mask-panel.js` prints nothing.
Commit: `masks: the sliders' ranges come from the protocol`. Why: "The ranges were typed in two places."

### B14. Built-in plugin lists from the manifests (UI half)
Tier: medium        Risk: low
Why: `PLUGINS_ELSEWHERE` (`crates/amb-gui/ui/dialogs/plugins.js`, 10 ids) and `TRIP_PLUGINS` (`crates/amb-gui/ui/tools/trip.js`, 7 ids) are hand-copied; a new built-in plugin must be added to both or it shows twice or not as part of a trip.
Files: `crates/amb-plugin/src/manifest.rs`, the `plugin.json` of the 10 plugins below, `docs/ai/plugins.md`, `crates/amb-gui/ui/dialogs/plugins.js`, `crates/amb-gui/ui/tools/trip.js`.
Steps:
1. `manifest.rs`: the struct `Manifest` denies unknown fields. Add `#[serde(default, skip_serializing_if = "String::is_empty")] pub home: String,` with the doc comment: where the editor already shows this plugin: "editor" (its own place: Surprise me, a task tab) or "trip" (part of a trip in the Trippy task, also in the Effects panel). Empty: only the Plugins dialog. `validate` accepts the empty string, `editor` and `trip` and rejects any other value with a message that lists them.
2. Add `"home": "trip"` to `plugins/hue-trip/plugin.json`, `plugins/holographic/plugin.json`, `plugins/light-leak/plugin.json`, `plugins/duotone/plugin.json`, `plugins/paint/plugin.json`, `plugins/kaleidoscope/plugin.json`, `plugins/heat-camera/plugin.json` (the ids in `TRIP_PLUGINS`), and `"home": "editor"` to `plugins/random-look/plugin.json`, `plugins/tilt-shift/plugin.json`, `plugins/trippy/plugin.json` (the other three ids in `PLUGINS_ELSEWHERE`).
3. `docs/ai/plugins.md`: add `home` to the manifest's field list, next to `recipes` (line about "`recipes`: optional ready-made parameter sets").
4. `plugins.js`: delete `PLUGINS_ELSEWHERE`; the filter becomes `.filter((p) => !(p.builtin && p.manifest?.home))`.
5. `trip.js`: delete `TRIP_PLUGINS`; add `const isTripPlugin = (id) => fx.plugins.find((p) => p.id === id)?.manifest?.home === "trip";` and use it where the list was used (`!TRIP_PLUGINS.includes(e.plugin)` becomes `!isTripPlugin(e.plugin)`). `fx.plugins` is the effect plugins from `plugins_list` (`crates/amb-gui/ui/develop/fx.js`).
6. Run `cargo test -p amb-plugin` (the manifests are built in with `include_str!`; the native-against-wasm test must pass; the `plugin.wasm` files do not change).
Don't: change the Rust table `BUILTIN` or `native.rs` (that is under "Needs a brief"); change a plugin's id.
Checks: the three cargo commands; the UI checks (`smoke.js` has the Trippy step: card A3 moves it to `trippy.js`).
Done when: `grep -rn "PLUGINS_ELSEWHERE\|TRIP_PLUGINS" crates` prints nothing.
Commit: `plugins: the editor reads where a built-in plugin lives from its manifest`. Why: "Two lists of ids were copied by hand."

### B15. Dialog helpers: one width rule and `withBusy`
Tier: medium        Risk: low
Why: `max-width: calc(100vw - 32px)` is typed on 10 dialog rules, and four library handlers repeat "disable the button, run, re-enable, show the error in the dialog's status line".
Files: `crates/amb-gui/ui/style.css`, `crates/amb-gui/ui/core/core.js`, `crates/amb-gui/ui/library/denoise.js`, `crates/amb-gui/ui/library/cleanup.js`, `crates/amb-gui/ui/library/duplicates.js`.
Steps:
1. `style.css`: the base rule `dialog { ... width: 560px; }` (line 256) gets `max-width: calc(100vw - 32px);`. Delete ` max-width: calc(100vw - 32px);` from the ten rules that repeat it: `#looks-dialog`, `#hub-dialog`, `#export-dialog`, `dialog.plugins-dialog`, `#import-dialog, #lrcat-dialog`, `.search-dialog`, `.guide-dialog`, `.clean-dialog`, `.dupes-dialog`, `.files-dialog` (their `width` stays). `.connect-dialog` uses `min(560px, calc(100vw - 32px))`: leave it.
2. `core.js`, after `showError` (card 0f; if it is not done yet, after `toast`), add:
   ```js
   // Run `fn` with `button` disabled; an error shows in `line` (the dialog's status line).
   // Gives true when `fn` finished, false when it threw.
   async function withBusy(button, line, fn) {
     button.disabled = true;
     try {
       await fn();
       return true;
     } catch (e) {
       line.textContent = String(e);
       return false;
     } finally {
       button.disabled = false;
     }
   }
   ```
3. Convert the four handlers that have exactly this shape; each keeps its own "Starting…" or "Looking…" text set before the call:
   - `installDenoiseModel` and `runDenoise` in `crates/amb-gui/ui/library/denoise.js` (`$("denoise-install")` and `$("denoise-go")` with `$("denoise-status")`; `renderDenoise()` stays after the call, as the old `finally` ran it).
   - `findCleanup` in `crates/amb-gui/ui/library/cleanup.js` (`$("clean-find")`, `$("clean-status")`; a `false` result returns, as the old `catch` did).
   - `findDuplicates` in `crates/amb-gui/ui/library/duplicates.js` (`$("dupes-find")`, `$("dupes-status")`; `renderDupes()` stays after).
4. `crates/amb-gui/ui/library/guess-places.js` (`roll` keeps its button disabled on an error), `crates/amb-gui/ui/library/bursts.js` and `crates/amb-gui/ui/library/place-picker.js` have other shapes: leave them.
Don't: change a message, an id or the order of what happens on screen.
Checks: the UI checks of "Checks" (`organise.js` or the scripts card A3 makes cover Clean up, duplicates and AI Denoise); open each of the three dialogs by hand and trigger an error if you can (Denoise without its model installed).
Done when: `grep -c "max-width: calc(100vw - 32px)" crates/amb-gui/ui/style.css` prints 1 (the base rule; `.connect-dialog`'s `min(...)` line is not `max-width:`).
Commit: `ui: one dialog width rule and a withBusy helper`. Why: "Ten copies of a width and four copies of a try-finally."

### B16. One progress helper for the editor and one for `amb`
Tier: medium        Risk: low
Why: the editor throttles progress events by hand three times, and `amb` prints progress with six hand-rolled `\r` loops.
Files: `crates/amb-gui/src/progress.rs` (new), `crates/amb-gui/src/main.rs`, `crates/amb-gui/src/commands/batch.rs`, `crates/amb-gui/src/commands/export.rs`, `crates/amb-cli/src/progress.rs` (new), `crates/amb-cli/src/main.rs`, `crates/amb-cli/src/cleanup.rs`, `crates/amb-cli/src/convert.rs`, `crates/amb-cli/src/library.rs`, `crates/amb-cli/src/search.rs`.
Steps, editor (one commit):
1. New `crates/amb-gui/src/progress.rs` (`mod progress;` in `main.rs`):
   ```rust
   /// A progress callback that sends `[done, total]` to the page as `event`, at most every 200 ms (and always at the end).
   pub(crate) fn throttled(app: &tauri::AppHandle, event: &'static str) -> impl Fn(usize, usize) + Send + Sync + 'static {
       use tauri::Emitter;
       let app = app.clone();
       let last = std::sync::Mutex::new(std::time::Instant::now());
       move |done, total| {
           let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
           if done == total || last.elapsed().as_millis() > 200 {
               *last = std::time::Instant::now();
               let _ = app.emit(event, [done, total]);
           }
       }
   }
   ```
2. Replace the three copies: in `commands/batch.rs` `library_find_duplicates` (event `duplicates-progress`) and `library_cleanup` (`cleanup-progress`), and in `commands/export.rs` the replay export (`replay-progress`): delete the `let last = ...; let progress = |done, total| { ... };` lines and write `let progress = crate::progress::throttled(&app, "duplicates-progress");` (the matching event). Pass `&progress` as before.
3. Leave `bursts-progress`, `denoise-progress` and `convert-progress` as they are: their events have another shape or are not throttled (bursts and convert emit every step; denoise sends four numbers).
Steps, `amb` (one commit):
4. New `crates/amb-cli/src/progress.rs` (`mod progress;` in `main.rs`):
   ```rust
   /// A progress callback that rewrites one line on stderr: `text(done, total)` is the line. Silent with `--json` (`quiet`). A newline at the end.
   pub(crate) fn line(quiet: bool, text: impl Fn(usize, usize) -> String + Sync) -> impl Fn(usize, usize) + Sync {
       move |done, total| {
           if quiet {
               return;
           }
           eprint!("\r{}", text(done, total));
           if done == total {
               eprintln!();
           }
       }
   }
   ```
5. Replace the loops, keeping each line's text: `cleanup.rs` (`"Looking… {done} of {total}"`, `quiet` is `json`; keep its `done % 50 == 0` as `if done % 50 == 0 || done == total` inside the text closure's caller: simplest is to keep that test in `cleanup.rs` around the call), `convert.rs` (`"{n}/{total}"`), `library.rs` (`"{}: {done}/{total}"`), `main.rs` twice (`"{raw}: {done} of {total} tiles"` and `"{n}/{total} frames"`), `search.rs` (`"indexing {done}/{total} with {}"`). A site whose old code did not print the newline at the end keeps working with it. A site that printed with `print!` to stdout keeps stdout: leave it and say so.
Don't: change a message; make a progress line appear with `--json` where it did not.
Checks: the three cargo commands; `cargo test -p amb-gui -p amb-cli`; the UI tests (Clean up and duplicates show progress).
Done when: `grep -rn "elapsed().as_millis() > 200" crates/amb-gui/src/commands` prints nothing and `grep -rn 'eprint!("\\r' crates/amb-cli/src` prints only `progress.rs`.
Commits: `gui: one throttled progress emitter`; `cli: one progress line helper`.

### B17. UI test steps for the dialogs no script opens
Tier: medium        Risk: low
Why: 18 of the 43 dialogs in `crates/amb-gui/ui/index.html` have no id in any script of `crates/amb-gui/tests/ui/` (checked 2026-11-12), so a broken open goes unseen. A step that opens each one and closes it catches that.
Files: `crates/amb-gui/tests/ui/dialogs.js` (new). Read: `crates/amb-gui/ui/core/uitest.js` (the `t` helpers) and the files in the table.
Steps:
1. `dialogs.js` starts like the other scripts (SPDX line; comment: "Every dialog opens, shows its main button, and closes. Run: ambrotype --ui-test crates/amb-gui/tests/ui/dialogs.js photo.dng"), then `await t.until("the photo", () => ui.info?.path, 60000);` and, for the library ones, `await toggleLibrary(true);`.
2. A helper at the top: `const opens = async (id, open, button) => t.step(`${id} opens and closes`, async () => { const p = open(); await t.until(id, () => $(id).open, 10000); if (button) t.expect(document.querySelector(button) && !document.querySelector(button).hidden, `${button} is there`); $(id).close(); await Promise.race([p, t.sleep(1000)]); t.expect(!$(id).open, `${id} closed`); });` (`open()` returns the flow's promise, which may wait for the dialog to close).
3. One step per dialog, with these openers (all are global functions of the UI scripts; the file is where the function lives):
   | Dialog | Open with | Main button | Needs |
   |---|---|---|---|
   | `paste-dialog` | `applyPresetPartly(ui.presets[0])` (`crates/amb-gui/ui/develop/presets.js`) | `#paste-dialog button.primary` | a preset: `ui.presets = await invoke("presets")` first |
   | `surprise-dialog` | `surpriseFlow()` | `#surprise-more` | a photo |
   | `match-dialog` | none: `matchFlow()` starts with `invoke("pick_references")`, a native file picker | | skip, and say so in the commit message |
   | `shortcuts-dialog` | `showShortcuts()` (`crates/amb-gui/ui/develop/frame.js`) | `#shortcuts-dialog button.primary` | none |
   | `settings-dialog` | `settingsFlow()` (`crates/amb-gui/ui/core/keys.js`) | `#settings-dialog [data-tab="keys"]` | none |
   | `sizes-dialog` | `sizesFlow()` (`crates/amb-gui/ui/tools/croptool.js`) | `#sizes-new` | none |
   | `basics-dialog` | `basicsFlow()` (`crates/amb-gui/ui/help/basics.js`) | none | none |
   | `learn-dialog` | `learnFlow()` (`crates/amb-gui/ui/help/learn.js`) | none | none |
   | `assistant-dialog` | `assistantFlow()` (`crates/amb-gui/ui/dialogs/assistant.js`) | `#assistant-send` | a photo |
   | `plugins-dialog` | `pluginsFlow()` (`crates/amb-gui/ui/dialogs/plugins.js`) | `#plugin-run` | none |
   | `hub-dialog` | `hubFlow()` (`crates/amb-gui/ui/dialogs/hub.js`) | `#hub-dialog [data-kind="all"]` | none; it asks the network: the step checks only that the dialog opens |
   | `name-dialog` | `askName("Name")` (`crates/amb-gui/ui/library/collections.js`) | `#name-dialog button.primary` | none |
   | `smart-dialog` | `editSmartCollection(null)` (`crates/amb-gui/ui/library/collections.js`) | `#smart-save` | none |
   | `meta-dialog` | `metaTemplatesFlow()` (`crates/amb-gui/ui/library/organise.js`) | `#meta-apply` | library on, a photo selected (`select(first, "only")`) |
   | `import-dialog` | `importFlow()` (`crates/amb-gui/ui/library/import.js`) | none | it may ask for a folder with a native picker first: if so skip the step and say so |
   | `lrcat-dialog` | `lrcatFlow()` (`crates/amb-gui/ui/library/lrcat.js`) | `#lrcat-pick` | none |
   | `hdr-dialog` | `libraryMergeHdr()` (`crates/amb-gui/ui/library/library-actions.js`) | `#hdr-dialog button.primary` | library on, two photos selected: the CI folder has `test.dng` only, so this step checks the toast "Select two or more raws" and that the dialog stays closed; say so in the step's name |
   | `pano-dialog` | `libraryMergePano()` (same file) | `#pano-dialog button.primary` | same as `hdr-dialog` |
4. For each, if the opener turns out to need something this table does not say, put the one line of set-up in the step and add the reason as a comment; if it needs a native file dialog, skip that dialog and list it in the commit message. Do not add a test hook to the app.
Don't: change any dialog or flow function.
Checks: the UI checks of "Checks" (the new script prints its steps and no `FAILED`).
Done when: the script has a step for each row not skipped; the number skipped is in the commit message.
Commit: `tests: every dialog opens and closes`. Why: "18 dialogs had no UI step."

### B18. More guard tests
Tier: medium        Risk: low
Why: three lists are held by review only: the buttons that `docs/FEATURES.md` names, the MCP server's tool list against its tool calls, and the built-in plugins.
Files: `crates/amb-gui/src/checks.rs` (card 0e must be done), `crates/amb-cli/src/mcp.rs` (its `mod tests`), `crates/amb-plugin/tests/builtin.rs` (new), `docs/FEATURES.md`.
Steps (three commits):
1. FEATURES names a real label. In `checks.rs` add `features_name_things_the_app_has`: for each line of `docs/FEATURES.md` that starts with `- **` and holds a `→`, take the text right of the first `→` up to the first `` `amb `` and every `**bold**` in it; lower-case it and drop a trailing `…`, `.` or `:`; it must occur (lower-cased) in the text of the files under `crates/amb-gui/ui/` (`.js`, `.html`, `.json`), `crates/amb-gui/src/**/*.rs`, `crates/amb-presets/data/*.json`, `crates/amb-gui/data/**/*.json` or `plugins/*/plugin.json`. 215 names are checked and 9 fail today: `Neon`, `Mandala`, `Heat camera`, `Contours`, `Acid highlights`, `Foil shadows` (task tiles in `plugins/trippy/plugin.json`: include that file's text so they pass), `Add to your presets`, `Lightroom +`, `Start … here`. For each that still fails, correct the name in `docs/FEATURES.md` to the label the app shows (look it up in the UI), not the other way round; list any you cannot match in an `ALLOWED` array with a comment saying why.
2. MCP. In the `tests` module of `crates/amb-cli/src/mcp.rs` add `every_listed_tool_has_a_call_and_back`: the names in `tools()` (each element's `"name"`) equal the string patterns that start a line with eight spaces in `fn call` (lines like `        "describe_photo" =>` up to the next top-level `fn`: read the file with `include_str!("mcp.rs")` and cut it from `fn call(` to the next `\nfn `). Both sets have 11 names today.
3. Built-in plugins. New `crates/amb-plugin/tests/builtin.rs`: for each folder in `plugins/` (path from `CARGO_MANIFEST_DIR/../../plugins`) that has a `plugin.json`, its `"id"` is one of the ids in `amb_plugin::BUILTIN` (the second field of each tuple), and every id in `BUILTIN` has such a folder. Both are 12 today. A plugin kept as an example and not built in goes in an `EXAMPLES` const at the top of the test (empty today).
Don't: change a label in the UI to match the doc.
Checks: the three cargo commands; `cargo test -p amb-gui checks`, `cargo test -p amb-cli mcp`, `cargo test -p amb-plugin --test builtin`.
Done when: each test fails when you add one violation (try it once, then undo).
Commits: `gui: a test that FEATURES names real buttons`; `cli: a test that the MCP tool list matches its calls`; `plugins: a test that the built-in list matches the plugins folder`.

### Needs a brief

A large model answers each question and writes the cards, then moves them above. Each is here because a builder would have to choose.

- **The editor's small preview, shared with `amb` (was B12d).** `thumbnail` in `crates/amb-gui/src/folder.rs` returns the camera's embedded preview, or when the file has none renders a small picture through the editor's `Session`, and caches the JPEG in the app's cache folder. `amb cleanup`, `amb duplicates` and `amb convert` (`crates/amb-cli/src/cleanup.rs`, `organise.rs`, `convert.rs`) read the embedded preview only, so on JPEG, PNG and TIFF photos (no embedded preview) they skip the tests the editor runs: a behaviour difference to check first by running both on a folder of JPEGs. Questions: (1) Which render does the fallback use, given that `amb-library` cannot depend on the editor's `Session`: a new small-preview function in `amb-ops` or `amb-engine`, which is also the first step of R7? (2) Where does `amb` keep its cache (the editor uses Tauri's app cache folder), and with what key (the editor's key is path, size, mtime and orientation)? (3) Do `amb cleanup` and `amb duplicates` start rendering previews for photos that have none (slower, and they would then find things they did not before), or does the editor stop doing it?
- **Tests for the largest files that have none.** `crates/amb-cli/src/organise.rs` (710 lines), `crates/amb-cli/src/render.rs` (542), `crates/amb-gui/src/commands/{presets,catalog,batch,export}.rs` (about 600 each) have no unit tests; the UI scripts cover only some of the commands. Questions: (1) In which order, and which behaviour of each is worth a test (the ones that lost data or looked wrong before: renaming and sorting in `organise.rs`, the export naming in `export.rs`)? (2) How is a photo faked: `amb_testkit::dng::write` with `amb_testkit::patterns::generate("ramp")` is what `crates/amb-library` and `crates/amb-gui/src/library.rs` tests use; is that enough, or does a test need a raw with a capture time or an embedded preview? (3) How is the editor's state faked: can `AppState` be built in a test, and can a command that takes a `tauri::AppHandle` be run with Tauri's mock runtime (`tauri::test`, which may need a feature and a dev-dependency: ask the owner), or must the logic move out of the command first (as B12a does)?
- **Splitting the long functions.** `export_one` (275 lines, `crates/amb-export/src/lib.rs`), `schema::panels` (311, `crates/amb-gui/src/schema.rs`), `state::plan` (237, `crates/amb-protocol/src/state/plan.rs`), `net::step` (250, `crates/amb-ai/src/net/plan.rs`), and the UI factories `create` in `crates/amb-gui/ui/tools/crop.js` (408) and in `crates/amb-gui/ui/develop/curve.js` (364), then `looksFlow`, `settingsFlow`, `makeWheel`, `wireMasks`. These are long matches whose order matters (a mistake changes output silently) or closures that share local state. Questions: for each, where the case boundaries go (per format, per panel group, per command, per op; for a UI factory, which state becomes an explicit object), whether helper signatures stay private, and which test proves the output is byte-identical before and after (export tests compare bytes; the UI factories have no UI test for dragging a crop or a curve: write that first?).
- **The Rust half of the built-in plugin table.** A built-in plugin is added in `BUILTIN` and `native.rs` (`crates/amb-plugin/src`), the workspace members in the root `Cargo.toml`, and the `amb-plugin` dependencies: eight edit points. Question: one macro table (`builtin_plugins! { "kaleidoscope" => amb_plugin_kaleidoscope::kaleidoscope, ... }`) that makes `BUILTIN` and the native dispatch, or keep them separate and rely on card B18's test that checks `BUILTIN` against the `plugins/` folder; and whether a test should check that each `plugin.wasm` is newer than its `src/`.

## Structural steps (the owner says go first)

### R5. The CLI as a table
`amb`'s dispatch `match`, its usage text, its ~140 flags (parsed globally, so any command accepts any flag) and the man page come from one `COMMANDS` table, with flags per command. Why: four hand-synced lists, and a flag is silently accepted by the wrong command. Decided: two steps, first the table driving the existing `match`, then flags per command. Open: the shape of the per-command context. Trigger: the next time a flag is silently accepted by the wrong command, or `crates/amb-cli/src/main.rs` passes its size limit again. Source: architecture proposal 1.

### R6. Dialog markup and CSS per feature
`crates/amb-gui/ui/index.html` and `style.css` are touched by more than half of all feature commits. Decided: not now; each stays one file while that is workable. Trigger: `index.html` passes 3,000 lines, or two agents working at once conflict in either file more than once a week. Design for then: a fragment `ui/<part>/<feature>.html` per dialog, fetched and attached on first open by a `dialog()` helper in `core.js`, the id-uniqueness test reading the fragments too; CSS in `ui/<part>/<feature>.css` linked in `index.html`. No build step, no modules.

### R7. The editor's Session as the batch renderer
Ops-like code in the editor drives a `Session` to render many photos, and `amb` has its own `render()`: two renderers of "a photo with its sidecar to a file". Open: one render function in amb-ops that both use. Needs a brief. Source: architecture C2.

### R8. ES modules or a framework for the UI
Still not now ([design/structure.md](design/structure.md) says why). Trigger: the shared scope causes a bug that the unique-name test did not catch.

## Found while working

Bugs or oddities noticed during a task, to look at separately:

- Fixed 2026-11-09: two quick rating or flag keys in the viewer could show the older values (each answer carries the photo's whole info); every info write now goes through `setInfo` in `core.js`, one at a time.
- Fixed 2026-11-09: tests that failed now and then: two export tests wrote the same JPEG XL files at once; two UI steps waited less than the work before them (Watch an edit's ghost cursor, the photo's fit after a resize).
- Fixed 2026-11-09: "Save to…" for a preset held `preset_dirs` while reading the settings path, the opposite order to `save_config`; two threads could each wait for the other. It reads the path first now.
