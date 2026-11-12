# Resume here (paused 2026-11-12)

A pass to make the codebase and its docs hold their shape without audits. Paused part-way when the session's credits ran out. This folder is working material: delete it when the "Left to do" list below is empty.

To resume, say: **"read docs/audit-2026-11-12/RESUME.md and continue"**.

## Done and pushed

- `docs/README.md`: the map of the docs, one job each, and the rules (one home per fact).
- `docs/codestyle.md`: the standing rules for code, each marked test / lint / review. Section 2 lists every list a feature is added to.
- `docs/delegation.md`: which tier of model does what, the task card format, when a builder stops, how to spend fewer tokens.
- Four audits, in this folder: `audit-hygiene.md` (sizes, seams, small fixes), `audit-architecture.md` (the lists a feature touches, duplication, layering, 11 proposals), `docs-map.md` (overlaps, stale lines, broken paths), `open-items.md` (267 open items from all docs, 21 contradictions).

## Where the three jobs stopped

1. **Rust fixes:** only the first is done: `crates/amb-ai/src/hooks.rs`, `amb_ai::hooks::install`, called by both programs (the two lists were the same five hooks, so no bug there). Not done:
   - drop `serde` from `crates/amb-lens/Cargo.toml` (a grep finds no use; build to confirm);
   - one `srgb_encode` in `crates/amb-engine/src/math.rs` (four identical copies: `detail/util.rs`, `profile.rs`, `local.rs`, `pipeline/mod.rs`) and `dehaze.rs` using `detail::util::lerp_position` (two identical copies);
   - `OVER_BUDGET` limits in `crates/amb-cli/tests/code_size.rs` down to each file's size rounded up to the next 50;
   - the agent primer (`primer` in `crates/amb-protocol/src/guides.rs`): one sentence pointing at `amb docs make-it-yours`, `layouts`, `plugins`, `interface`, and check the shell quoting in `crates/amb-gui/src/commands/connect.rs`.
   - Checked and left alone: the `expect`s at `mcp.rs:444`, `raw.rs:530` and `snapshots.rs:62` are on things the code guarantees.
2. **Docs into one roadmap:** done: `docs/NEXT.md` is now `docs/ROADMAP.md` (110 rows in four tables; eight ideas dropped as already built), links updated, WHATS-NEXT trimmed, `docs/CHANGELOG.md` holds the older done lines. Not done:
   - `docs/GAPS.md`: section 0 still has the old tables (replace with a pointer to ROADMAP.md, keep the heading), "Top 10 to close next" and the two sentences that mention it go; the section 3 Map row still says there is no search for a place by name;
   - `docs/design/future-ideas.md`: remove the sections marked Built, fix its intro;
   - a banner under the title of each dated doc (`docs/SCOPE.md`, `docs/design/structure.md`, `docs/design/agent-navigation.md`, `docs/research/11-…`, `docs/research/08-…`): `> Dated YYYY-MM-DD and kept as written. For what is current, see [X](path).`; structure.md's last section becomes a pointer to `docs/codestyle.md`;
   - the stale lines, crate tables and short paths that `docs-map.md` lists (README's table lacks `amb-ops`; ARCHITECTURE's lacks `amb-plugin` and `amb-plugin-sdk`; "About 104,000 lines…" in ARCHITECTURE and structure);
   - `docs/research/watchlist.md`: say that each recheck also looks for projects we don't track yet;
   - a full check that every relative link and `#anchor` resolves.
3. **`docs/cleanup.md`:** the new frame is in (rules, checks, moving code, structural steps R5 to R8). Two cards are written in full (A1, A2). The rest are one line each under "Cards still to write", with the audit section each comes from.

## Left to do, in order

1. **Checks on what is in the tree:** `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`. If the last commit doesn't say they passed, run them. Then push to `main`.
2. **Finish job 1** above (medium tier, one sitting).
3. **The guard tests** that `docs/codestyle.md` already names (medium tier). They are the point of the pass:
   - `crates/amb-gui/src/checks.rs` (`#[cfg(test)]`): every `#[tauri::command]` is in `handler` (`crates/amb-gui/src/commands/mod.rs`) and the reverse; every `invoke("name"` in `crates/amb-gui/ui` names a registered command (allow the test-only ones); every top-level `function wire…` is called; counts that may only fall: `Object.assign(document.createElement(` (381), `innerHTML` (15), inline `style="` in index.html (17), top-level `let` (7); no `localStorage` outside `core.js`; no `toast(String(e), true)`.
   - `crates/amb-cli/tests/code_health.rs`: every `amb NAME` in `USAGE` has an arm in `main.rs`; every `` `amb x` `` in `docs/FEATURES.md` is in `USAGE`; the count of `#[allow(` (16) and of bare `.unwrap()` outside tests may only fall.
   - `crates/amb-cli/tests/docs_health.rs`: every `docs/**/*.md` (not `research/NN-*`, `release-notes/`, this folder) is linked from `docs/README.md`; relative links resolve; every crate in `crates/` is named in `AGENTS.md`, `README.md` and `docs/ARCHITECTURE.md`.
4. **UI helpers** (medium): `showError(e)` and `plural(n, one, many)` in `crates/amb-gui/ui/core/core.js`; replace all 166 `toast(String(e), true)`; finish the `store` migration (8 raw `localStorage` sites: columns.js, slideshow.js, library.js, tips.js; keep keys and formats); convert the six newest library dialogs to `el()`. Run the UI tests.
5. **Finish job 2** above (medium; docs only).
6. **Write the remaining cards** in `docs/cleanup.md` (medium; one group per sitting, each path checked against the code).
7. **Review the roadmap's Tier and Ready columns** (large or top tier: this is judgement). Most rows say `brief`; the first ten rows should get their briefs written while a top model is at hand.
8. **Small leftovers:** `crates/amb-gui/ui/help/teach.js` line 2 still says `docs/NEXT.md`; `AGENTS.md` should link `docs/codestyle.md`, `docs/README.md` and `docs/delegation.md` and drop what they now say.
9. **Delete this folder,** and add the pass to WHATS-NEXT "Recently done".

## Found on the way

- **Past requests only partly met** (each should be a ROADMAP row; check): zoom in the library's Loupe and Compare; Insights lacks an overall style summary, gear suggestions and "styles next to yours"; "use JPEG if available" does not hold when a raw is opened directly; tips appear in only two empty boxes; the agent started from Connect an AI app was told about photos only (step 2 above); the monthly recheck's "look for projects we don't track" was only in the scheduled task's prompt.
- **Commit trailers named a model** in every commit until now, against AGENTS.md. From this pass on the trailer is `Co-Authored-By: Claude <noreply@anthropic.com>`.
- **The UI's old idioms grew in three days** (element building 316 to 381, error toasts 157 to 166) although `el()` existed: a rule in a doc did not hold, which is why step 2 makes them tests.
- **The two programs install the AI hooks by copy-paste;** any difference between the lists is a "works in the editor, not in `amb`" bug. Checked: the two lists were the same; they are one function now.
- GAPS section 0 listed two built features as ideas (the shareable before/after page, the blank-canvas layout editor).

## Not in the repo

- The owner's Life OS (Notion) was audited and tidied in the same session: an Upkeep page, the index, three saved views, and a Workflow Log entry of 2026-11-12 with ten open decisions for the owner. Nothing there is left in flight.
- Open question for the owner: commit project subagents (`.claude/agents/`, with a cheaper `model:` each) so every session delegates reading and routine work, or keep that in the owner's own setup.
