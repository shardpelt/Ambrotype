# Resume here (paused 2026-11-12)

A pass to make the codebase and its docs hold their shape without audits. Paused part-way when the session's credits ran out. This folder is working material: delete it when the "Left to do" list below is empty.

To resume, say: **"read docs/audit-2026-11-12/RESUME.md and continue"**.

## Done and pushed

- `docs/README.md`: the map of the docs, one job each, and the rules (one home per fact).
- `docs/codestyle.md`: the standing rules for code, each marked test / lint / review. Section 2 lists every list a feature is added to.
- `docs/delegation.md`: which tier of model does what, the task card format, when a builder stops, how to spend fewer tokens.
- Four audits, in this folder: `audit-hygiene.md` (sizes, seams, small fixes), `audit-architecture.md` (the lists a feature touches, duplication, layering, 11 proposals), `docs-map.md` (overlaps, stale lines, broken paths), `open-items.md` (267 open items from all docs, 21 contradictions).

## In flight when paused (check `git log` and `git status` first)

Three medium-tier agents were told to wrap up. What each was doing:

1. **Rust fixes** (`crates/` only): one function that installs the AI hooks for both programs (`crates/amb-ai/src/hooks.rs`), the unused `serde` in amb-lens, three panics on outside input (mcp.rs, raw.rs, snapshots.rs), `srgb_encode` and `lerp_position` copies, tighter `OVER_BUDGET` limits, the agent primer also pointing at the layout and plugin guides.
2. **Docs into one roadmap:** `docs/NEXT.md` renamed `docs/ROADMAP.md` and rewritten as one ordered table (Id, What, Size, Tier, Ready); GAPS section 0 and "Top 10" folded in; WHATS-NEXT trimmed (older "Recently done" lines to `docs/CHANGELOG.md`); built ideas out of `design/future-ideas.md`; banners on dated docs; stale lines and crate lists fixed.
3. **`docs/cleanup.md` rewritten** as open task cards (groups A small, B medium, "Needs a brief", structural steps R5 to R8).

If a commit named "wip" is the last one on the work branch, the tree was saved mid-change: run the checks before trusting it.

## Left to do, in order

1. **Verify what the agents left:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, and the UI tests (docs/cleanup.md, "Checks"). Review the diffs against `docs/codestyle.md`. Fix or revert, then push to `main`.
2. **The guard tests** that `docs/codestyle.md` already names (medium tier). They are the point of the pass:
   - `crates/amb-gui/src/checks.rs` (`#[cfg(test)]`): every `#[tauri::command]` is in `handler` (`crates/amb-gui/src/commands/mod.rs`) and the reverse; every `invoke("name"` in `crates/amb-gui/ui` names a registered command (allow the test-only ones); every top-level `function wire…` is called; counts that may only fall: `Object.assign(document.createElement(` (381), `innerHTML` (15), inline `style="` in index.html (17), top-level `let` (7); no `localStorage` outside `core.js`; no `toast(String(e), true)`.
   - `crates/amb-cli/tests/code_health.rs`: every `amb NAME` in `USAGE` has an arm in `main.rs`; every `` `amb x` `` in `docs/FEATURES.md` is in `USAGE`; the count of `#[allow(` (16) and of bare `.unwrap()` outside tests may only fall.
   - `crates/amb-cli/tests/docs_health.rs`: every `docs/**/*.md` (not `research/NN-*`, `release-notes/`, this folder) is linked from `docs/README.md`; relative links resolve; every crate in `crates/` is named in `AGENTS.md`, `README.md` and `docs/ARCHITECTURE.md`.
3. **UI helpers** (medium): `showError(e)` and `plural(n, one, many)` in `crates/amb-gui/ui/core/core.js`; replace all 166 `toast(String(e), true)`; finish the `store` migration (8 raw `localStorage` sites: columns.js, slideshow.js, library.js, tips.js; keep keys and formats); convert the six newest library dialogs to `el()`.
4. **Review the roadmap's Tier and Ready columns** (large or top tier: this is judgement), then cut `docs/WHATS-NEXT.md` "Up next" to its first five rows.
5. **Finish `docs/cleanup.md`** if it still has a "Cards still to write" section.
6. **Small leftovers:** `docs/README.md` needs a row for `docs/CHANGELOG.md`; `crates/amb-gui/ui/help/teach.js` line 2 still says `docs/NEXT.md`; `AGENTS.md` should link `docs/codestyle.md`, `docs/README.md` and `docs/delegation.md` and drop what they now say.
7. **Delete this folder,** and add the pass to WHATS-NEXT "Recently done".

## Found on the way

- **Past requests only partly met** (each should be a ROADMAP row; check): zoom in the library's Loupe and Compare; Insights lacks an overall style summary, gear suggestions and "styles next to yours"; "use JPEG if available" does not hold when a raw is opened directly; tips appear in only two empty boxes; the agent started from Connect an AI app was told about photos only (being fixed in 1 above); the monthly recheck's "look for projects we don't track" was only in the scheduled task's prompt.
- **Commit trailers named a model** in every commit until now, against AGENTS.md. From this pass on the trailer is `Co-Authored-By: Claude <noreply@anthropic.com>`.
- **The UI's old idioms grew in three days** (element building 316 to 381, error toasts 157 to 166) although `el()` existed: a rule in a doc did not hold, which is why step 2 makes them tests.
- **The two programs install the AI hooks by copy-paste;** any difference between the lists is a "works in the editor, not in `amb`" bug. See the Rust agent's report in the commit message.
- GAPS section 0 listed two built features as ideas (the shareable before/after page, the blank-canvas layout editor).

## Not in the repo

- The owner's Life OS (Notion) was audited and tidied in the same session: an Upkeep page, the index, three saved views, and a Workflow Log entry of 2026-11-12 with ten open decisions for the owner. Nothing there is left in flight.
- Open question for the owner: commit project subagents (`.claude/agents/`, with a cheaper `model:` each) so every session delegates reading and routine work, or keep that in the owner's own setup.
