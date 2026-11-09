# Structure review (2026-11-09)

A look at the whole codebase: its size, its shape, what holds up and what has grown too big. The tasks that follow from it, in exact steps for any coding agent, are in [cleanup.md](../cleanup.md).

## Short version

- **No rewrite needed.** The architecture holds:
  - library-first crates in a clean dependency graph;
  - one command protocol for the GUI, the CLI and AI clients;
  - panels generated from the settings registry;
  - 735 Rust tests and 89 UI test steps;
  - no TODOs and few lint exceptions.
- **What hurts is growth at the edges:**
  - a few files that do too much;
  - a UI of 63 scripts sharing 1,085 global names;
  - the editor's state held in 27 separate locks;
  - `amb-library` mixing the library with batch photo operations;
  - small helpers copied from file to file;
  - docs that repeat each other.
- **The fix:**
  - three phases of moves that change no behaviour: guard rails, small merges, then file splits;
  - then four structural steps, agreed with the owner first.
  - Each step is one commit that the tests confirm.

## Measured

Counted with `python3 tools/code-metrics.py`. Production lines leave out `#[cfg(test)]` blocks and test files.

| Language | Files | Lines |
|---|---|---|
| Rust | 235 | 103,928 |
| JavaScript (UI and UI tests) | 67 | 23,259 |
| WGSL shaders | 13 | 2,277 |
| HTML | 3 | 1,915 |
| CSS | 3 | 1,514 |
| Markdown | 70 | 11,825 |

### Crates

| Crate | Lines | Uses | Role |
|---|---|---|---|
| `amb-engine` | 32,872 | color, lens, presets | Raw decode and the develop pipeline |
| `amb-gui` | 17,991 | all but cli | The editor (Tauri 2) |
| `amb-library` | 6,350 | engine, export, lens, presets, protocol | Catalogue, folders, import, Lightroom, files, search |
| `amb-ops` | 3,221 | engine, export, gpu, lens, library, presets, protocol | Batch operations that render photos (since R1) |
| `amb-protocol` | 8,246 | presets | Undoable edit commands, sidecars, history |
| `amb-export` | 7,067 | color, engine | Files out |
| `amb-cli` | 6,859 | all but gui | The `amb` tool and the MCP server |
| `amb-lens` | 3,828 | none | Lens profiles |
| `amb-gpu` | 3,745 | engine | The pipeline and AI layers as compute shaders |
| `amb-ai` | 3,560 | engine, gpu | Local AI models |
| `amb-presets` | 3,002 | none | Settings, XMP, the tag registry |
| `amb-plugin` | 2,586 | engine, export, presets, protocol | Sandboxed plugins |
| `amb-testkit` | 1,155 | color, presets | Synthetic photos, the Lightroom kit (dev-only for engine, library, gui) |
| `amb-color` | 1,134 | none | Colour science |
| `amb-plugin-sdk` | 584 | none | What plugins are written with |

The graph has no cycles. AI models reach the engine through registration (`amb_engine::subject::set_finder`, `retouch::set_inpainter`), so the engine doesn't depend on `amb-ai`:

```
presets ─┬─ protocol ───────────────────────────┐
color ───┼─ engine ─┬─ export ──┬─ library ─────┼─ gui
lens ────┘          ├─ gpu ─ ai │              └─ cli
                    └───────────┴─ plugin
```

### Big files

Rust, production lines over 900:

| File | Lines | What's in it |
|---|---|---|
| `amb-gui/src/session.rs` | 1,855 | The open photo: edits, previews, regions, tools, retouch, contexts, export, and the presets list |
| `amb-engine/src/mask.rs` | 1,497 | The mask model, sky, drawing shapes, applying local adjustments |
| `amb-engine/src/negative.rs` | 1,254 | Film negatives: analysis, cast, frame finding, conversion |
| `amb-gui/src/looks.rs` | 1,243 | Layouts and themes: built-ins, validation, storage, setups |
| `amb-protocol/src/state.rs` | 1,210 | `DevelopState`: history, snapshots, sidecar I/O, execute, plan, query |
| `amb-gui/src/hub.rs` | 1,193 | The hub: index, each item kind's fetch and install, submit links, CI check |
| `amb-protocol/src/masks.rs` | 1,141 | Mask commands |
| `amb-ai/src/net.rs` | 1,122 | Running a model's graph layer by layer on the GPU |
| `amb-engine/src/lens.rs` | 1,013 | Lens corrections |
| `amb-engine/src/retouch.rs` | 978 | Spots: heal, clone, AI Remove |
| `amb-gui/src/commands/photo.rs` | 956 | 37 Tauri commands about the open photo |
| `amb-protocol/src/sidecar.rs` | 937 | Sidecar merge, info |
| `amb-export/src/embed.rs` | 932 | Metadata into finished files |
| `amb-ops/src/object.rs` | 919 | Select object, select in words, move, remove |
| `amb-engine/src/matching.rs` | 908 | Match look |
| `amb-engine/src/pipeline/settings.rs` | 906 | `DevelopSettings` read from the settings map |

UI scripts over 900 lines: `library.js` 1,488, `collage.js` 1,452, `app.js` 1,188, `masks.js` 1,082, `layout.js` 1,009, `editor.js` 989. And `index.html` 1,781 (every dialog's markup), `style.css` 1,423: both stay whole, since splitting them needs a build step or a change in what the cascade sees.

### Long functions

259 of 2,557 Rust functions are over 50 lines, 29 over 150. The long ones that matter:
- `amb-gui` `main` (460 lines): 200 of startup, then the 232-command handler list.
- `amb-cli` `main` (414): one `match` over the commands. It reads well; leave it.
- `amb-engine` `xtrans::run` (343) and `rcd::band` (236): demosaicing ports. Leave them.
- `plan::plan_with` (310), `pipeline::develop` (265), `amb-gpu` `develop::render` (273): the pipeline's order, written out once each. Leave them; their order is the point.
- `amb-export` `export_one` (275), `amb-gui` `schema::panels` (311), `amb-protocol` `state::plan` (237), `amb-ai` `net::step` (250): these could be split by case.

JS: 30 of 964 functions are over 80 lines. The longest: `crop.js` `create` (408), `curve.js` `create` (364), `layout.js` `looksFlow` (288), `keys.js` `settingsFlow` (264), `wheels.js` `makeWheel` (225), `masks.js` `wireMasks` (209).

### Copies

- `smoothstep`: 8 copies in the engine (two with a guard for equal edges, which is the safe form; every call passes ordered edges). One since 2026-11-09 (S1).
- `percentile`: 3 in the engine, but they differ (nearest rank or interpolated; a fraction or a percent), so they aren't copies.
- `file_stamp`: `amb-presets` `linked.rs` and `amb-library` `catalog.rs` (the same function). One since 2026-11-09 (S2).
- `luma`: 3 in the plugins (fine: plugins are separate crates).
- JS: a file's name from its path, written out 9 times in 6 files (and `fileName` in `collage.js`, `dirOf` in `filmstrip.js`); an element builder (`el`) only in `masks.js`.
- JS idioms:
  - 316 × `Object.assign(document.createElement(…), …)`;
  - 69 `localStorage` calls, most wrapped in their own `try`/`JSON.parse`;
  - 157 × `toast(String(e), true)`.

### Coupling

- `$`, `invoke`, `ui` and the toast lived in `app.js`, which loads last, so 56 other files used its names. They are now in `core.js`, which loads first.
- The files whose names the most others use: `masks.js` (used by 32 files), `library.js` (24), `preview.js` (22), `collage.js` (17).
- The saved settings are one `Config` behind `AppState::prefs` (since 2026-11-09, R2); `AppState`'s other fields are the open photo, the GPU, watchers and the like.
- `amb-library` has two jobs:
  - the library: catalogue, folders, import, collections, files, Lightroom, search index;
  - photo operations that render: `convert`, `replay`, `merge`, `object`/`snap`, `burst`, `dust`, `negative`, `look`, `contact`.
  - Only the operations need `amb-gpu` and `amb-export`.
  - Since 2026-11-09 (R1) the operations are the `amb-ops` crate, except `look` and `contact`, which search needs; `amb-library` no longer uses `amb-gpu`.

### Hygiene

- No TODOs.
- 15 `#[allow]`s, 11 of them `too_many_arguments`.
- 68 `unwrap`/`expect` outside tests, mostly on locks and fixed data.
- Every source file has its SPDX line.
- Two tests already guard the UI's shared scope: element ids are unique, and so are top-level names. They read `ui/*.js` only, not subfolders.

## What is good (keep it)

- **Library-first.** The editor and `amb` are thin clients. A feature written once works in both, and over MCP.
- **One code path for edits.** Every edit is an `amb-protocol` command, so undo, history and the sidecar can't drift between front ends.
- **Generated, checked docs.** The settings reference, the core card and the recipes are checked by tests.
- **Tests at every level:**
  - engine stages against references;
  - each GPU kernel against its CPU stage;
  - the protocol's schema against serde;
  - the editor driven by UI scripts.
- **Plain UI.** No build step and no npm. Any agent can read and run it.
- **Files already have sections** (`// ---- name` lines). Splitting along them is mechanical.

## Problems, by what they cost

1. **Files that do too much.**
   - An agent reading `session.rs` or `library.js` loads 1,500 to 1,900 lines to change ten.
   - Merge conflicts gather there.
   - Each splits cleanly along its existing sections.
2. **The UI's one global scope.**
   - 1,085 names in one namespace, and the load order matters in ways nothing states.
   - The core helpers live in the file that loads last.
3. **The editor's state.**
   - Saved settings are a lock each, plus a copy-in, copy-out `Config`.
   - Adding a setting touches four places.
4. **`amb-library` is two crates.**
   - The library parts rebuild whenever the GPU or export code changes.
   - Where a new operation goes is unclear.
5. **The GUI's `main`.** Its 250-line handler list belongs with the commands it lists.
6. **Copies of helpers.** Small, but each copy can drift (two `smoothstep`s already differ at equal edges).
7. **Docs repeat each other:**
   - `NEXT.md`'s "Built" list repeats `FEATURES.md` and `ARCHITECTURE.md`;
   - `ARCHITECTURE.md` had table cells of up to 4,100 characters (restructured on 2026-11-09).

## Target shape

- **Size budget:** a production file of at most about 800 lines (Rust) or 700 (JS), and a function of at most 150, except tables, dispatch and ported algorithms. A test holds it (task G2), with today's big files listed until they are split.
- **Rust:** a module past the budget becomes a folder with one file per concern. Its public paths stay the same (`mod.rs` re-exports).
- **Crates:** `amb-library` keeps the library; `amb-ops` has the batch operations (task R1, done 2026-11-09).
- **Editor state:** one `Prefs` lock for the saved settings, which is the `Config` that's saved (task R2).
- **UI:**
  - `core.js` loads first, with `$`, `invoke`, `toast`, `askYes`, a `store` for localStorage and the path helpers;
  - files stay under the budget, each starting with a header that says what it holds;
  - files grouped in folders once the tests read subfolders (task R3).
- **Docs:** one home per fact (task R4):
  - what the app does: `FEATURES.md`;
  - how the code is built: `ARCHITECTURE.md`;
  - the design of one feature: its design doc;
  - what's next: `WHATS-NEXT.md`, then `NEXT.md` for the tracks;
  - what's missing: `GAPS.md`.

## The plan

The phases and their tasks are in [cleanup.md](../cleanup.md), each with its files, steps, checks and commit message.

| Phase | What | Who | Risk |
|---|---|---|---|
| 0. Guard rails (done) | UI tests read subfolders and check every script loads; a file-size test; the metrics script in `tools/` | | none |
| 1. Small merges (done) | `math.rs` (smoothstep), one `file_stamp`, `core.js`, a `store` helper | any model | low |
| 2. Split big files (done) | 9 Rust files and 6 UI scripts, along their sections | any model, one task each | low: tests and the UI scripts catch mistakes |
| 3. Structure | `amb-ops`; `Prefs`; UI folders; docs with one home per fact | planned by a strong model, owner says go | medium |

**Not now:**
- **ES modules for the UI:** the webviews support them without a build step. But the UI tests and the menu call globals, and 1,085 names would need imports. Do it only if the global scope keeps causing bugs after `core.js` and the splits.
- **A JS framework:** it would bring a build step, against the "no npm" rule.
- **A rewrite of `Session` or the protocol:** both are sound; they only need splitting.

## Rules that keep it lean

- New code goes in the file that owns its concern. If none does, make one; don't grow `app.js`, `library.js`, `session.rs` or `main.rs`.
- A helper used by two files goes in `core.js` (UI), `math.rs` (engine) or the crate that owns the data. Never copy it.
- A new top-level JS name is specific (`maskDrawRadial`, not `draw`), so it's easy to search for and can't clash.
- Re-measure with `tools/code-metrics.py` after a large feature. If a file passes the budget, split it along its sections in the next commit.
