# Handing work to models

Which model does which work, and how a task is written so a cheaper model can do it with no other context. The aim: the most capable model's time goes into decisions that many cheap sessions then follow.

Model names change, so this page uses four tiers. Today: **small** is Haiku, **medium** is Sonnet, **large** is Opus, **top** is the newest model above Opus. Commits and code never name a model (AGENTS.md).

## Who does what

| Tier | Give it | Don't give it |
|---|---|---|
| **Small** | Reading and summarising; collecting todos; checking links and paths; a docs row; regenerating generated docs; a cleanup task marked `small`; running the checks and reporting | Anything with a choice to make |
| **Medium** | A change that follows a recipe ([recipes.md](recipes.md)); a feature that has a brief; splitting a file; tests; a bug with a failing test; reviewing a small model's diff | A new algorithm, a new crate, a change to a file format |
| **Large** | Writing a brief; a new pipeline stage or model; a refactor across crates; a bug with no way to reproduce it yet; reviewing a medium model's diff in the engine, the protocol or sidecars; the monthly research recheck | Routine work a recipe covers |
| **Top** | An audit of the whole codebase; a decision that changes the architecture; briefs for many items at once; the hardest algorithm and speed problems | Anything a lower tier can do from a brief |

Each item in [ROADMAP.md](ROADMAP.md) carries its tier and whether it is ready.

## The loop

1. **Brief** (large or top): a task card, below. For a feature, its design doc in `docs/design/` first.
2. **Build** (the tier on the card): one task, one session, one commit.
3. **Check:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and the commands the card names. The guard-rail tests ([codestyle.md](codestyle.md)) catch what a review would.
4. **Review** (one tier above the builder): reads the diff only, against the card and the code style. Not needed for a small task whose checks pass.
5. **Docs:** the rows the code style lists. Then the next card.

## A task card

Written so the reader needs nothing else. [cleanup.md](cleanup.md) is a list of these.

```
### ID. Title
Tier: small | medium | large        Risk: low | medium | high
Why: one or two sentences.
Files: every file to read or change, by path.
Steps: numbered, exact. Names of functions and where they go.
Don't: what must not change (names, formats, behaviour).
Checks: the commands to run, and what they must print.
Done when: what is true afterwards.
Commit: `area: summary`
```

A card is ready when a model that has never seen the repo could do it. If a step says "decide" or "choose", it isn't ready: decide it in the card.

## When to stop and hand up

A builder stops, writes down the question (in the card, or under "Found while working" in cleanup.md) and leaves the tree clean when:

- the card doesn't say which of two ways to go;
- a check fails and two tries don't fix it;
- the change would rename a public item, a setting, a command, an element id or a file format;
- the change spreads to files the card doesn't list;
- a file would pass the size budget.

A wrong guess costs more than the question.

## Spending fewer tokens

- **One task, one session.** Start a new session (or `/clear`) for the next task. A long session re-reads everything it has seen, and compacting is itself a large request.
- **Hand off through the repo, not the chat:** WHATS-NEXT.md, the card and the commit message carry what the next session needs.
- **Let small models read.** Send a subagent to read long files, logs or docs and report the few lines that matter. Its reading stays out of the main context.
- **Search before reading:** grep for the name, then read the part of the file around it.
- **Don't switch model mid-task:** each model has its own cache, so a switch re-reads the conversation at full price.
- **Keep AGENTS.md short.** It is read every session. Detail belongs in the doc it links to.

## Setting it up in Claude Code

From the Claude Code docs (2026-11); check them, since this changes.

- The session's model: `/model sonnet` (or `opus`, `haiku`). `opusplan` plans with the large model and builds with the medium one.
- A subagent's model: the `model` given when it is started, else `model:` in its definition (`.claude/agents/NAME.md`), else the `CLAUDE_CODE_SUBAGENT_MODEL` environment variable, else the session's model. Without one of these, subagents run on the session's model.
- The built-in Explore agent uses the session's model, not a small one.
- A scheduled task has its own model, chosen where it is made.
- `/usage` shows what is left and what used it.
