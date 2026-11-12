# Resume here (paused 2026-11-12, second time)

Start a **new session** (not the old one: its long history costs a full re-read) and say:
**"read docs/audit-2026-11-12/RESUME.md and continue"**. A medium model is enough for everything below; every step is a card or a short list. Read only this file, then the card you are doing.

## State

- `main` is green and holds everything finished: the code style (`docs/codestyle.md`), the docs map (`docs/README.md`), delegation (`docs/delegation.md`), the roadmap by tier (`docs/ROADMAP.md`), every cleanup task as a card (`docs/cleanup.md`), the docs leftovers.
- The work branch `claude/lightroom-alternative-scope-xlnf0p` is `main` plus one commit, `wip: library Back and Map gestures` (the same as `back-map.patch` below; it fails a test). **Never push that commit to `main`.** Simplest: start from `main` and use the patches.
- Push finished work with: `git push origin HEAD:claude/lightroom-alternative-scope-xlnf0p && git push origin HEAD:main`.
- Commit trailer: `Co-Authored-By: Claude <noreply@anthropic.com>` and the session line. No model names.
- Cargo: always `--release` (there is no debug cache). One cargo owner per checkout.

## Unfinished: three features the owner asked for

Each is saved as a patch in `docs/audit-2026-11-12/wip/`, made against the commit named. None is on `main`. Do one at a time, each on a fresh checkout of `main`: `git apply --3way docs/audit-2026-11-12/wip/NAME.patch`, finish it, run every check in `docs/cleanup.md` "Checks" (all three cargo commands and every UI script), commit, push. Then delete its patch. The later ones will conflict a little in `crates/amb-gui/ui/core/keys.js`, `crates/amb-gui/ui/index.html`, `crates/amb-gui/ui/style.css` and the docs lines: keep both sides.

Every feature also owes its docs: a row in `docs/FEATURES.md`, a line dated 2026-11-12 at the top of "Recently done" in `docs/WHATS-NEXT.md` (15 stay; the oldest moves to the top of `docs/CHANGELOG.md`).

### 1. `preset-hide.patch` (against `63ee28b`): nearly done
The owner asked: "a way to remove presets, and/or hide them." Right-click a preset: Hide, or Delete… for your own files (to the trash). Right-click a group: Hide this group. "N hidden · Show · Unhide all" at the foot of the Presets tab. New `crates/amb-gui/ui/develop/preset-hide.js`; `preset_deletable` in `crates/amb-gui/src/preset_files.rs`. Docs are done.
- Passed: fmt, clippy, all three UI scripts (two new steps in `smoke.js`).
- Left: `cargo test --workspace --release`; fix what fails; commit as `presets: hide a preset or a group, and delete your own`.
- Review before merging: Delete… shows for every scanned preset file, so also for files in Lightroom's own folder. They go to the trash, not gone, but say so in the question or limit it to the app's folder.

### 2. `back-map.patch` (against `67420d0`): Back is broken, Map works
The owner asked: "back button while in library view still goes back to develop view, instead of folder back" and "in map view: side scroll is acting weird, can we add the same drag system and also scroll wheel". This patch is also the commit `wip: library Back and Map gestures` on the work branch.
- Map: drag to pan, wheel and pinch zoom at the pointer, no sideways page scroll (new `crates/amb-gui/ui/library/map-touch.js`, shared with the place picker). Its step in `crates/amb-gui/tests/ui/browse.js` passed once. The side scroll was: one zoom level per wheel event, so a sideways swipe zoomed out again and again.
- Back: `navGo`/`navStep` in `crates/amb-gui/ui/core/tasks.js`. **Failing:** `browse.js` step "Back in the library returns to the folder before" times out; `smoke.js`'s last Back step failed before a fix (`nav.chain`, calls run one after another) that is built but not re-run.
- Left: run the UI scripts; fix the browse step (look at `navStep`/`navShowSource` in `tasks.js`, the `navNote()` call at the top of `renderLibrary` in `crates/amb-gui/ui/library/library-grid.js`, and nav state left by earlier steps); take the debug text out of that step's label; re-run fmt, clippy, tests; docs.
- If Back stays hard, split it: land the Map part alone first.

### 3. `batch-export.patch` (against `0f9b928`): logic done, editor part never run
The owner asked: "batch exports/resizes. i want to make a lot of images in subfolders smaller". `amb render DIR... --out-dir D [--recursive] [--flat] [--skip-existing] [--dry-run] [--json]` and a **Batch export…** dialog in the library.
- Done and tested: `crates/amb-ops/src/export_batch.rs` (plan, run, report; 9 tests) and the `amb` side (`crates/amb-cli/tests/export_batch.rs` passes: tree kept, long edge, skip existing). Outputs are written as hidden `.partial` files and renamed, so a stopped run can be run again.
- Written, never compiled into the editor or run: the editor commands in `crates/amb-gui/src/commands/batch.rs`, `crates/amb-gui/ui/library/batch-export.js`, `crates/amb-gui/tests/ui/batch-export.js`.
- Left: build; fmt, clippy, tests (expect the man page, FEATURES and size tests to ask for updates: `cargo run --release -p amb-cli -- man > docs/man/amb.1`; `commands/batch.rs` must stay under 800 lines); run the UI scripts and fix the new one; try it by hand (6 test photos in 3 nested folders, `--long-edge 256`); docs, plus a line in `docs/ARCHITECTURE.md` where `amb-ops`'s modules are listed.
- Then card B12c in `docs/cleanup.md` (the editor's old export-many onto this) can go ahead.

## Then, in order

1. **Group 0 of `docs/cleanup.md`** (cards 0a to 0f): the small Rust fixes, the guard tests `docs/codestyle.md` already names, the `showError` and `plural` helpers. Do 0e and 0f after the three features are merged: they sweep the same UI files.
2. **Delete this folder** (`docs/audit-2026-11-12/`) and add one line for the pass to `docs/WHATS-NEXT.md` "Recently done".
3. Then `docs/cleanup.md` group A, group B; `docs/ROADMAP.md` from the top. The first roadmap rows (Film lab) need a brief from a large model first.

## For the owner

- **Open question:** commit project subagents (`.claude/agents/`, each with a cheaper `model:`) so every session sends reading and routine work to a small model? Or keep that in your own setup (`CLAUDE_CODE_SUBAGENT_MODEL` on the cloud environment)?
- **Life OS** (Notion): tidied in this pass; an Upkeep page says how to keep it. Ten decisions wait for you in the Workflow Log entry of 2026-11-12.
- **Past requests only partly met** are roadmap rows now: `library-loupe-zoom`, `insights-*`, `jpeg-twin-on-open`, `tips-more-places`, `agent-editor-primer`, `library-view-centred`.
- README screenshots still wait for your Fujifilm samples.
