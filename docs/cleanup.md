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

## Cards still to write

Each of these is a card id with its one line. The audits are in [audit-2026-11-12/](audit-2026-11-12/): `audit-hygiene.md` (hygiene) and `audit-architecture.md` (architecture). A model of tier large writes the exact steps (verify every path and item name first), then moves the line into "Cards" above.

Group A (small):
- **A3.** UI test scripts per feature: move the newest features' steps out of `crates/amb-gui/tests/ui/organise.js` and `smoke.js` into their own scripts (CI runs `tests/ui/*.js`); `smoke.js` undoes until History is empty instead of a fixed count. Source: architecture proposal 9, D1.
- **A4.** `el()` instead of `Object.assign(document.createElement(...))`: one commit per file, the 15 files with most sites first, each lowering the count in `crates/amb-gui/src/checks.rs`. Needs conversion rules (`className` to `class`, `textContent` to a child, `onclick` keys, non-attribute properties stay plain assignments). Source: hygiene C6, architecture B3.

Group B (medium):
- **B1 to B8.** Split each file on `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs` by its sections: B1 `crates/amb-engine/src/retouch.rs` (heal, inpaint, source), B2 `crates/amb-protocol/src/sidecar.rs` (read, write), B3 `crates/amb-export/src/embed.rs` (avif, jxl), B4 `crates/amb-ops/src/object.rs` (select, words, remove), B5 `crates/amb-engine/src/matching.rs` (stats, solve), B6 `crates/amb-engine/src/pipeline/settings.rs` (tags table, `from_settings`), B7 `crates/amb-ai/src/lib.rs` (spec, install), B8 `crates/amb-gpu/src/nn.rs` (ops, kernels, session). Source: hygiene table "Rust files >= 680" and Fix tasks 5 to 8.
- **B9a to c.** Split `crates/amb-engine/src/raw.rs` (metadata, preview), `crates/amb-library/src/catalog.rs` (prints and vectors out) and `crates/amb-engine/src/dehaze.rs` (guided-filter helpers) before they pass 800. Source: hygiene Fix task 9.
- **B10.** `pub` to `pub(crate)` for functions and constants used only in their crate in amb-ops, amb-ai and amb-library; keep types in public signatures `pub`; re-grep `crates/` and `plugins/` for each. Source: hygiene C3, Fix task 4.
- **B11.** One error type for editor commands: `crates/amb-gui/src/error.rs` with `CmdError(String)`, `From<E: Display>`, `Serialize` as a plain string, `Res<T>`; then drop the ~300 `.map_err(|e| e.to_string())`. One commit, when no other work is in flight in amb-gui. Source: architecture B2, proposal 10.
- **B12a to d.** Editor logic into library crates: (a) `library_best_in_bursts` (`crates/amb-gui/src/commands/batch.rs`) into amb-ops' burst module; (b) the read-change-write of a photo's info (`change_info` and its 4 copies) into one `amb-library` function, also used by `amb tag`; (c) `export_many` and `render_many` into one amb-ops function; (d) the thumbnail core of `crates/amb-gui/src/folder.rs` into amb-library. (c) and (d) touch the editor's Session: may need a brief. Source: architecture C2, B1.5, B1.6, proposal 4.
- **B13.** Mask sliders from the schema instead of `crates/amb-gui/ui/tools/mask-panel.js` literals; ranges live in `crates/amb-protocol/src/masks/fields.rs`. Source: architecture C3, proposal 8.
- **B14.** UI half: `PLUGINS_ELSEWHERE` and `TRIP_PLUGINS` read from each plugin's manifest. Source: architecture proposal 7.
- **B15.** One dialog width rule in `crates/amb-gui/ui/style.css` (10 copies of `max-width: calc(100vw - 32px)`) and a `busy(button, fn)` helper in `core.js`; convert the six newest library dialogs. Source: architecture B3, B5, proposal 6.
- **B16.** One throttled progress emitter for editor commands (copies in `crates/amb-gui/src/commands/batch.rs` twice and `export.rs`) and one text progress printer for `amb` (six `\r` loops: `cleanup.rs`, `convert.rs`, `library.rs`, `main.rs` twice, `search.rs`). Source: architecture B1.3, B2.
- **B17.** UI test steps for the dialogs no script opens (19 ids, re-check each first). Source: hygiene C9, Fix task 14.
- **B18.** Guard tests: every `**Button**` name in `docs/FEATURES.md` exists in the UI; the MCP server's `tools/list` equals its `tools/call` arms; every built-in folder in `plugins/` is in `BUILTIN`. Source: architecture proposal 11.

Needs a brief (a large model designs first):
- Tests for the largest untested command and CLI files (`crates/amb-cli/src/organise.rs`, `crates/amb-gui/src/commands/{presets,catalog,batch,export}.rs`, `crates/amb-cli/src/render.rs`): which first, how to fake a photo and a GUI state. Source: hygiene C9.
- Splitting the long functions `export_one`, `schema::panels`, `state::plan`, `net::step`, and the UI factories `create` in `crates/amb-gui/ui/tools/crop.js` and `ui/develop/curve.js`. Source: hygiene C4, Fix tasks 15 and 16.
- The Rust half of the plugin table (one macro for `BUILTIN` and `native.rs`). Source: architecture proposal 7.

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
