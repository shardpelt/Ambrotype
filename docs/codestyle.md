# Code style

How code is written here, so the codebase stays lean while features keep landing. Read it before a change; compare a change against it in review. It is short on purpose: the detail of how the code is built is in [ARCHITECTURE.md](ARCHITECTURE.md), the steps of common changes in [recipes.md](recipes.md).

Each rule says what holds it: **(test)** a test fails, **(lint)** `cargo fmt` or `clippy` fails, **(review)** only a reader catches it. A rule that keeps being broken should become a test, not a longer sentence here.

## 1. Where code goes

A feature is written once, in a library crate. The editor and `amb` are thin: they pass arguments in and show what comes out.

| Part | Goes in | Holds | Never holds |
|---|---|---|---|
| Pixel maths, a pipeline stage | `amb-engine` | pure functions on linear pixels | files, printing, AI models |
| Folders, the catalogue, sidecars, search | `amb-library` | walking, reading and writing photos' info | rendering in bulk, the GPU |
| An operation that renders photos | `amb-ops` | `Options` in, a `Report` out, a `progress` callback | printing, Tauri, argument parsing |
| A change to a photo's edit | `amb-protocol` | a `Command`: one undo step | anything a front end decides |
| An AI model | `amb-ai`, reached through a hook the engine offers | loading, tiling, running | pipeline maths |
| A `amb` command | `crates/amb-cli/src/<area>.rs` | flags to `Options`, the call, text or `--json` out | thresholds, algorithms, file formats |
| An editor command | `crates/amb-gui/src/commands/<area>.rs` | arguments, the call, progress events, the report back | the same |
| What the user sees | `crates/amb-gui/ui/<part>/<feature>.js` | one feature's state, drawing and wiring | ranges, labels and defaults of settings (they come from the registry) |

If the editor can do something `amb` can't (or the other way round), the logic is in the wrong crate. **(review)**

**The example to copy** is Clean up: `crates/amb-ops/src/cleanup.rs` (the logic, its thresholds as named constants, its tests), `crates/amb-cli/src/cleanup.rs` (`amb cleanup`), `library_cleanup` in `crates/amb-gui/src/commands/batch.rs` (glue and the `cleanup-progress` event), `crates/amb-gui/ui/library/cleanup.js` (the dialog), a step in `crates/amb-gui/tests/ui/organise.js`.

## 2. The lists a feature is added to

Most bugs in a new feature are a list someone forgot. These are all of them. Where a test holds a list, forgetting fails the build; add a test for any list that bites twice.

| List | Where | Held by |
|---|---|---|
| Editor commands | `handler` in `crates/amb-gui/src/commands/mod.rs` | test: every `#[tauri::command]` is listed, and the UI calls only listed ones |
| The wire call | the part's wire function (`wireLibrary`, …) or `init` in `crates/amb-gui/ui/core/app.js` | test: every `wire…` function is called |
| The script tag | `crates/amb-gui/ui/index.html` | test: every script is loaded once |
| Key action (search, shortcuts, the menu bar) | `KEY_ACTIONS` in `crates/amb-gui/ui/core/keys.js`, `MENU_BAR` in `menu.js` | test (menu ids) |
| `amb` command | the `match` in `crates/amb-cli/src/main.rs`, `USAGE` in `usage.rs`, `docs/man/amb.1` | test: help, dispatch, man page and FEATURES.md agree |
| Develop setting | `crates/amb-presets/data/tags.json` and what [recipes.md](recipes.md) lists | tests (schema, generated references) |
| Built-in plugin | `BUILTIN` and `native.rs` in `amb-plugin`, the workspace members, the UI's `TRIP_PLUGINS` and `PLUGINS_ELSEWHERE` | test for native against wasm; the UI lists by review |
| AI task and model | `TASKS` and `data/models.json` in `amb-ai` | test |
| AI hook into the engine | `crates/amb-ai/src/hooks.rs`, called by both programs | one function, so they can't differ |
| Guide for AI agents | `GUIDES` in `crates/amb-protocol/src/guides.rs` | test |
| Docs | section 8 below | test for the map and links; the rows by review |

## 3. Size and shape

- A source file has at most 800 lines (Rust, before its tests) or 700 (UI script). **(test: `crates/amb-cli/tests/code_size.rs`)** A file about to pass it is split along its sections in the same commit (section 11 says how); the limit is not raised.
- A split module becomes a folder with one file per concern; `mod.rs` re-exports, so public paths don't change.
- A function has at most 150 lines, except tables, dispatch and ported algorithms. **(review; `python3 tools/code-metrics.py` lists the longest)**
- New code goes in the file that owns its concern. If none does, make a file. Don't grow `main.rs`, `app.js`, `index.html`'s scripts or a file near its limit.
- A helper needed by a second file moves to the shared place (`crates/amb-engine/src/math.rs`, `crates/amb-gui/ui/core/core.js`, or the crate that owns the data) in that commit. It is never copied. **(review)**

## 4. Rust

- **Errors:** return the crate's `Error`. No `unwrap()`. `expect("why this can't fail")` only for something the code itself guarantees, never for what a file, a user, a client or the network sent. **(test: the count of bare `unwrap()` outside tests may only fall)**
- **Messages** are for the person at the screen: what happened and what to do, in plain words ("AI Denoise works on raws, not on JPEG, TIFF or other rendered images").
- **Locks:** a lock that a panic elsewhere may have poisoned is taken with `unwrap_or_else(|e| e.into_inner())`. Editor state goes through `AppState`'s methods (`prefs`, `change_prefs`); never hold two of its locks at once.
- **Many arguments** become an `Options` struct with `Default`. `#[allow(...)]` needs a reason in a comment. **(test: the count of `#[allow]` may only fall)**
- **Long work** takes `progress: &dyn Fn(usize, usize)` (done, total) and returns a `Report` that derives `Serialize`, so `--json` and the editor show the same thing.
- **Visibility:** `pub(crate)` unless another crate uses it.
- **Numbers with meaning** are named constants at the top of the file, with their unit and where the value comes from.
- **Every file** starts with the SPDX line and a `//!` header saying what it holds and why. Comments say why, not what.
- **Dependencies:** ask the owner before adding one. Shared ones are declared once, in the workspace.
- **The engine depends on nothing above it.** What it needs from above (AI models, plugins) it gets through a hook that the programs install.

## 5. The UI

Plain scripts in one shared scope, no build step. That only works with discipline:

- **One file per feature,** in its part's folder, starting with a comment that says what it holds. Its state is one object (`const clean = { … }`), not loose `let`s. Its functions carry the feature's name (`cleanMark`, not `mark`). **(test: top-level names are unique; the count of top-level `let` may only fall)**
- **It ends with one `wire…` function** that attaches its listeners. **(test: it is called)**
- **Use the helpers in `core.js`:** `$`, `invoke`, `el` (builds an element), `store` (saved values), `toast`, `showError`, `status`, `askYes`, `plural`, `setInfo` (rating, flag, label), `fileName`, `dirOf`. The older idioms they replace (`Object.assign(document.createElement(…))`, `toast(String(e), true)`, `localStorage` directly, `innerHTML`) are counted, and the counts may only fall. **(test)** Touching old code? Convert what you touch and lower the count. The `el` count left (15) is on purpose: each is where a local `el` would shadow the shared one, or `fx.js`'s `<output>` with a `value`; rename the local first.
- **Shared parts, not copies:** a sideways row of photos is `photoRow` (`crates/amb-gui/ui/core/photo-row.js`); a map is `crates/amb-gui/ui/library/map-touch.js`; the photo's gestures are `crates/amb-gui/ui/core/pinch.js`. Zooming a picture in the library (the viewer, Loupe, Compare) is `makeZoom` (`crates/amb-gui/ui/library/viewer-zoom.js`). Loading pictures as they come near the view is `nearView` and `loadQueue` (`crates/amb-gui/ui/core/lazy.js`). A button that is off while its work runs is `withBusy` (`crates/amb-gui/ui/core/core.js`). Ctrl- and Shift-click selection over a list is `pickPath` (`core/core.js`). A change to a photo's info or edit reaches the views through `photoChanged` (`core/core.js`), never by calling a view's update. Progress from Rust is `throttled` (`crates/amb-gui/src/progress.rs`) in the editor and `line` (`crates/amb-cli/src/progress.rs`) in `amb`. A second copy of a part that exists is a bug: use the part, or extend it. **(review; a test counts two of them: `new IntersectionObserver(` outside `lazy.js` and a sideways-scrolling rule other than `.photo-row`, and the counts may only fall)**
- **Words built in a script go through `t()`** (`t("Moved to the trash")`, `plural(n, "photo")`): the English text is the key, and nothing is added to a language file by hand ([design/language.md](design/language.md)). A data list's label is written `tKey("Square")` and shown with `t(item.label)`; a settings' word (from the registry) is shown with `t(c.label)` too. People's own words (file, preset, folder names) are never looked up: their element gets `translate="no"`. After adding one, run `python3 tools/lang-table.py`, which updates the table of words. **(test: the table is current)**
- **Text from a file or a person** goes in with `textContent` or as a child of `el`, never through `innerHTML`.
- **Settings are drawn from the schema.** No range, default, step or label of a setting is typed into a script. **(review; a test for mask effects)**
- **Every action is a key action,** so search, the shortcut list and the menu all find it. The app teaches by showing: a feature nobody can stumble on is not finished.
- **Markup:** a dialog's ids start with its feature (`clean-dialog`, `clean-find`). No `style="…"`. **(test: ids are unique; inline styles may only fall)**
- **CSS:** colours, radii and spacing come from the theme's variables (`var(--…)`), so every theme works. A rule that two dialogs need is written once, for a shared class.
- **Long work:** the Rust side emits `<feature>-progress` with `[done, total]`; the button is disabled while it runs; an error shows in the dialog, not only as a toast.

## 6. Data, not code

A list that will grow is a data file that code reads, not a `match` or an `if` chain: settings (`tags.json`), their texts (`descriptions.json`), AI models (`models.json`), AI apps (`ai-apps.json`), layouts, themes and presets (the hub), tips and the features list (`FEATURES.md`). A new entry should need no new code. No maker, camera or product gets its own branch in the code when a table row will do.

## 7. Tests

- A new module has tests in the same file. A new `amb` command has a test that runs the built binary (`crates/amb-cli/tests/`). **(review)**
- A UI feature or fix gets a step in `crates/amb-gui/tests/ui/`. A new feature gets its own script there rather than a longer `smoke.js`.
- A rule worth keeping gets a guard: a test that reads the source or the docs and fails when the rule is broken. Counts that "may only fall" are in `crates/amb-gui/src/checks.rs` and `crates/amb-cli/tests/code_health.rs`; lower them in the commit that removes some.
- "Done" means the commands the request names were run, as written ([recipes.md](recipes.md)).

## 8. Docs with the change

| The change | Also update |
|---|---|
| A feature people use | a row in `docs/FEATURES.md`; one line in `docs/WHATS-NEXT.md` "Recently done"; delete its row from `docs/ROADMAP.md` |
| What exists in the code changed | `docs/ARCHITECTURE.md` |
| A gap closed or found | `docs/GAPS.md` |
| `amb`'s help text | `amb man > docs/man/amb.1` |
| A setting's range, default or text | the two generated references ([AGENTS.md](../AGENTS.md)) |
| A new doc | a row in [docs/README.md](README.md) **(test)** |

One home per fact ([docs/README.md](README.md)). Plain, short sentences. No model names in code or commits.

## 9. Words

- **Setting:** one named value of an edit (`Exposure2012`, `amb:LUTAmount`). **Registry:** the table that defines them (`crates/amb-presets/data/tags.json`).
- **Command:** an `amb_protocol::Command`, one undo step. An editor command (a Tauri command) is something else: say "editor command".
- **Sidecar:** the `.xmp` beside a photo, holding its edit and its info (rating, flag, place).
- **Operation:** a function in `amb-ops` that renders photos and reports.
- **Stage:** one step of the develop pipeline. **Hook:** a function a program installs into the engine.
- **Tab, mode, view, tool, dialog:** [design/tabs.md](design/tabs.md).

## 10. Before you say it's done

1. Is the logic in a library crate, with the editor and `amb` as glue?
2. Is every list in section 2 updated?
3. Did any file pass its size, or any "may only fall" count rise?
4. Are there tests where section 7 asks for them, and did the request's own commands run?
5. Are the docs in section 8 updated, each fact in one place?
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## 11. Splitting a file

When a file passes its size (section 3), split it in its own commit, and change nothing the app does.

- **Move, don't rewrite.** Cut and paste whole items unchanged, with their doc comments, attributes and the comment lines above them. The only edits: `mod` and `use` lines; visibility (`pub(super)` or `pub(crate)` where the compiler asks, never plain `pub` unless the item already was); `include_str!` and `include_bytes!` paths; the Tauri handler list; `<script>` tags in `index.html`.
- **Names stay:** public items, Tauri commands, setting names, localStorage keys, CSS classes, element ids, event names and file formats. Sidecars, saved settings, plugins, hub items and tests depend on them.
- **Fix what names the moved code:** `grep -rn "old/path.rs" docs AGENTS.md README.md crates tools` and correct each mention (`docs/recipes.md` is checked by a test). Take a split file off `OVER_BUDGET` in `crates/amb-cli/tests/code_size.rs`; the test says when.
- **Tests unchanged in count:** compare `cargo test -p CRATE`'s "N passed" before and after.

### Rust

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

### UI

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
