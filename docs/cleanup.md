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

### Group A: small

### A4. `el()` instead of `Object.assign(document.createElement(...))`
Done 2026-11-19 for every site it can be (card in git history). 15 sites remain on purpose: where a local `el` (a parameter, a loop variable or slideshow.js's own `el`) would shadow the shared one, and `fx.js`'s `<output>` with a `value`. Converting one of those means renaming the local first. The limit in `crates/amb-gui/src/checks.rs` is the count left.

### Group B: medium

How the splits below work. Each follows "Moving Rust code" and the rules above (move whole items with their doc comments, change only `mod` and `use` lines and visibility, keep every public path by re-exporting). The line numbers are from 2026-11-12: find an item by its name (`grep -n "fn NAME" FILE`), not by the number. Common steps for B1 to B8 and B9a to c:
- `git mv OLD.rs OLD/mod.rs` first, unless the card says the new files sit beside the old one.
- The file's `#[cfg(test)] mod tests { ... }` block moves to `OLD/tests.rs` without its wrapper; `mod.rs` gets `#[cfg(test)] mod tests;`. If the tests then need private items from a new file, add `use` lines in `tests.rs`, not new `pub`.
- Every item that was `pub` and is used outside the folder is re-exported from `mod.rs` with `pub use <part>::{Name, ...};` (list them with `grep -n "^pub " NEWFILE`). An item that was private and is now used by a sibling file becomes `pub(super)`.
- Take the file off `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs` (the test says so when it must go); read its number from the file, it may have changed.
- Fix mentions of the old path (rule 7 above). `docs/ARCHITECTURE.md` and `docs/design/structure.md` name several of these files.
Checks for all of them: the three cargo commands, with the crate's own tests unchanged in count (`cargo test -p CRATE`, compare the "N passed" before and after).

### Needs a brief

A large model answers each question and writes the cards, then moves them above. Each is here because a builder would have to choose.

- **The editor's small preview, shared with `amb` (was B12d).** `thumbnail` in `crates/amb-gui/src/folder.rs` returns the camera's embedded preview, or when the file has none renders a small picture through the editor's `Session`, and caches the JPEG in the app's cache folder. `amb cleanup`, `amb duplicates` and `amb convert` (`crates/amb-cli/src/cleanup.rs`, `organise.rs`, `convert.rs`) read the embedded preview only, so on JPEG, PNG and TIFF photos (no embedded preview) they skip the tests the editor runs: a behaviour difference to check first by running both on a folder of JPEGs. Questions: (1) Which render does the fallback use, given that `amb-library` cannot depend on the editor's `Session`: a new small-preview function in `amb-ops` or `amb-engine`, which is also the first step of R7? (2) Where does `amb` keep its cache (the editor uses Tauri's app cache folder), and with what key (the editor's key is path, size, mtime and orientation)? (3) Do `amb cleanup` and `amb duplicates` start rendering previews for photos that have none (slower, and they would then find things they did not before), or does the editor stop doing it?
- **Tests for the largest files that have none.** `crates/amb-cli/src/organise.rs` (710 lines), `crates/amb-cli/src/render.rs` (542), `crates/amb-gui/src/commands/{presets,catalog,batch,export}.rs` (about 600 each) have no unit tests; the UI scripts cover only some of the commands. Questions: (1) In which order, and which behaviour of each is worth a test (the ones that lost data or looked wrong before: renaming and sorting in `organise.rs`, the export naming in `export.rs`)? (2) How is a photo faked: `amb_testkit::dng::write` with `amb_testkit::patterns::generate("ramp")` is what `crates/amb-library` and `crates/amb-gui/src/library.rs` tests use; is that enough, or does a test need a raw with a capture time or an embedded preview? (3) How is the editor's state faked: can `AppState` be built in a test, and can a command that takes a `tauri::AppHandle` be run with Tauri's mock runtime (`tauri::test`, which may need a feature and a dev-dependency: ask the owner), or must the logic move out of the command first (as B12a does)?
- **Splitting the long functions.** `export_one` (275 lines, `crates/amb-export/src/lib.rs`), `schema::panels` (311, `crates/amb-gui/src/schema.rs`), `state::plan` (237, `crates/amb-protocol/src/state/plan.rs`), `net::step` (250, `crates/amb-ai/src/net/plan.rs`), and the UI factories `create` in `crates/amb-gui/ui/tools/crop.js` (408) and in `crates/amb-gui/ui/develop/curve.js` (364), then `looksFlow`, `settingsFlow`, `makeWheel`, `wireMasks`. These are long matches whose order matters (a mistake changes output silently) or closures that share local state. Questions: for each, where the case boundaries go (per format, per panel group, per command, per op; for a UI factory, which state becomes an explicit object), whether helper signatures stay private, and which test proves the output is byte-identical before and after (export tests compare bytes; the UI factories have no UI test for dragging a crop or a curve: write that first?).
- **Batch export through `amb_ops::export_batch` (was B12c, stopped 2026-11-19).** The editor's `export_many` (`crates/amb-gui/src/library.rs`) and `amb render`'s `render_many` (`crates/amb-cli/src/render.rs`) each name and loop. `export_batch::plan` has neither a name template (both take one: the editor's `name_template`, `amb render --name`) nor the editor's rule of skipping names that already exist on disk (it only numbers names taken within the batch), and `run` exports three at a time where `render_many` prints `[i/n]` lines in order, which `crates/amb-cli/tests/` check. Questions: (1) Does `export_batch::Options` gain `name: Option<String>` (an `import::file_name` template) and `unique_on_disk: bool`, or do the two front ends keep their loops and share only a `unique_name` helper? (2) May `amb render` print its lines as photos finish (three at a time), or must it keep one at a time?
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
- Fixed 2026-11-12: `amb-engine`'s test `mask::tests::ai_masks_select_nothing_and_are_reported` failed once in a full run and passed on two reruns. Cause: `model_masks_ask_for_their_kind_and_background_is_the_subject_inverted` installs a stand-in with `subject::set_finder`, which is process-wide and stayed set, so when it ran first the other test found a model and no note. Both tests now hold a lock, and the finder is removed when the installing test ends.
- Fixed 2026-11-13: `batch-export.js` failed now and then at "Start is on once there is a folder" because the step read the button before the plan call finished; the step now waits on the `planned` counter. `browse.js` could not be made to fail again (6 of 6 runs clean on a quiet machine), so its 3 failures in 8 were load on the machine, not a slowdown from the lazy loader or the dialog helpers.
