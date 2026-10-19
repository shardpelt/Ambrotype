# Helping agents find their way: a plan

Status: rounds A1 and B1 done (2026-10-16), results below. B1 passes all 12 tasks. The test branches (`base/*`, `work/*`) were deleted before the repository went public; the task texts and hidden checks are on `checks/a1`.

The goal is to measure what helps an agent that has never seen this code make a correct change here, cheaply. We keep what works, write it down, and then clean up the code to match.

## What others found

- **Context files:** an ETH Zurich study (ICLR 2026, [arXiv 2602.11988](https://arxiv.org/abs/2602.11988)) found that repository context files like AGENTS.md didn't raise success rates.
  - Generated ones lowered them slightly and hand-written ones raised them slightly; neither result was significant.
  - Both cost over 20% more. The authors advise keeping these files minimal.
- **Speed:** another 2026 study (Lulla et al.) found that context files cut run time and output tokens, with the same success rate.
- **Maps:** Aider doesn't keep a stored map. For each request it builds a ranked list of signatures, using tree-sitter and PageRank. RepoMapper is a stand-alone CLI and MCP server built on the same idea.
- **Callers and callees:** Claude Code's LSP tool (since 2.0.74) can list a function's callers, callees and references. It needs a language-server plugin; for Rust, that's rust-analyzer from a community marketplace. Reports say it is still rough.

So keep the text that loads every session short, and offer maps as tools an agent calls when it needs them.

## What we compare

- **A:** today's AGENTS.md and docs.
- **B:** A, plus:
  - recipes for changes that cross layers (for example "add a setting": registry, protocol, engine, panel, generated references);
  - a short summary at the top of each file;
  - a trimmed AGENTS.md.

  All of B is written from where A's runs got stuck.
- **C1:** B plus RepoMapper, for a ranked map on demand.
- **C2:** B plus the LSP tool with rust-analyzer, for callers and callees on demand.

Aider itself isn't used. It is a different agent, so it would measure the agent, not the help.

## Tasks

About eight tasks. Each has a check the agent can't see, kept outside the repo.

1. **Add a develop setting end to end.** Check: a render changes, and the generated references list the setting.
2. **Fix a seeded engine bug** from a user-style report ("the vignette brightens the corners"). Check: a hidden test.
3. **Add a toolbar button.** Check: the looks and docs tests, plus a look by hand.
4. **Add a CLI subcommand.** Check: run it on a test file.
5. **Add a protocol command.** Check: `amb do` on a test file.
6. **Add a hub item kind.** Check: a hidden install test, like the collage one.
7. **Fix a seeded GUI bug that crosses JS and Rust.** Check: a grading agent with a rubric, plus a run by hand.
8. **Answer a "where is…" question.** Check: an answer key.

Seeded bugs live on a task branch with squashed history, so `git log` doesn't give them away.

## How we run it

- **Fresh sessions:** each run is a new cloud session with its own container and disk, and no prior context. All runs use the same model, and the task text is the only prompt. Separate containers also stop the runs' build folders filling one disk.
- **What we record:**
  - pass or fail;
  - tokens, time and tool calls;
  - files read before the first edit;
  - dead ends.
- **Size:**
  - A pilot comes first: 3 tasks, condition A, one run each. It checks the setup and the cost per run.
  - The full round is 8 tasks, 2 runs each, per condition: about 50–65 runs.
- **Limits:** it is a small sample on one model and one repo. Treat the results as signals, not proof.

## Results: round A1 (today's docs, Sonnet 5.5, one run per task)

Twelve fresh cloud sessions, each from a one-commit snapshot of the code (branch `base/a1`, planted bugs on `base/a1-t2` and `base/a1-t7`), pushing to `work/a1-t1`…`work/a1-t12`. Hidden checks and the task texts are on branch `checks/a1`.

| Task | Result | Cost | Time |
|---|---|---|---|
| t1 Fade setting end to end | Partial: renders to spec (black to L* 21), panel and export work; `amb do "amb:Fade=40"` from the task fails and wasn't tried | $0.42 | 19 min |
| t2 Planted bug: vignette in the 1:1 view | Pass: the one-character fix, plus a test | $0.38 | 4 min |
| t3 Auto toolbar button | Partial: works, but edited the bundled hub copy of the layouts, which breaks their checksum test | $0.41 | 3 min |
| t4 `amb rating` | Pass | $0.39 | 7 min |
| t5 `reset_all` command | Pass | $0.55 | 25 min |
| t6 LUTs in the hub | Pass | $0.47 | 10 min |
| t7 Planted bug: rating undoes unsaved edits | Pass, with a better fix than the original | $0.24 | 2 min |
| t8 Trace the Exposure slider | Pass: all 8 steps of the key, with lines | $0.53 | 2 min |
| t9 Make a theme | Pass | $0.27 | 2 min |
| t10 Make a plugin | Pass, one flaw: from As Shot it drops the as-shot Tint | $0.39 | 5 min |
| t11 Make a layout | Pass | $0.27 | 8 min |
| t12 Edit a photo | Pass: darker, less colour, blue shadows (hue 220), grain | $0.24 | 4 min |

Total about $4.60; times are from start to the last update.

What it shows:
- **Finding the code isn't the problem.** Both planted bugs were found and fixed in minutes, and the trace was complete. Even with no map, Sonnet navigates this repo well for focused tasks.
- **Failures come from not checking the result.** t1 never ran the command the task named; t3 couldn't run the editor's tests. The test sessions can't build `amb-gui` (the environment's setup script doesn't install the GTK and WebKit libraries), so GUI work goes untested there.
- **One rule in AGENTS.md was missed:** built-in layouts are changed in the hub repo, not in `data/hub/` (t3).
- **Two product gaps showed up:** the compact syntax can't set `amb:` settings (`amb do "amb:LUT=…"` fails too), and setting Temperature from As Shot resets Tint to 0 unless it is set as well.

For round B: recipes for "add a setting" (including the compact syntax) and "add a toolbar button" (don't touch `data/hub/`), and a line telling agents to run the exact commands a task names. Fix the environment first, so every round can build the editor.

## Results: round B1 (recipes and session hook, Sonnet 5.5, one run per task)

The same 12 tasks on `base/b1`: the A1 snapshot plus `docs/recipes.md`, its test, the "Common changes" lines in AGENTS.md and the session-start hook. The two product gaps A1 found were left unfixed, so t1 and t10 face the same code. Work is on `work/b1-t1`…`work/b1-t12`.

| Task | A1 | B1 | B1 cost |
|---|---|---|---|
| t1 Fade setting end to end | Partial | Pass: ran `amb do "amb:Fade=40"`, found the compact-syntax gap and fixed it, with a test | $1.23* |
| t2 Planted bug: vignette | Pass | Pass | $0.38 |
| t3 Auto toolbar button | Partial | Pass: left `data/hub/` alone and said the built-in layouts need the hub repo | $0.31 |
| t4 `amb rating` | Pass | Pass | $0.35 |
| t5 `reset_all` command | Pass | Pass | $0.62* |
| t6 LUTs in the hub | Pass | Pass | $0.54 |
| t7 Planted bug: rating undoes edits | Pass | Pass | $0.24 |
| t8 Trace the Exposure slider | Pass (8/8) | Pass (8/8, plus the bonus) | $0.56 |
| t9 Make a theme | Pass | Pass | $0.24 |
| t10 Make a plugin | Pass, one flaw | Pass | $0.48 |
| t11 Make a layout | Pass | Pass | $0.23 |
| t12 Edit a photo | Pass | Pass | $0.22 |

\* t1 and t5 hit the 5-hour usage limit and resumed later; the restart re-read their context, so their cost is high and their time isn't comparable.

Total about $5.40. Without t1 and t5 the other ten cost $3.55, against $3.63 for the same ten in A1.

What it shows:
- **The two A1 misses are gone.** Both came from rules the recipes now state: run the commands the task names, and don't edit `data/hub/`.
- **The same cost.** The recipes add a file to read but no measurable cost.
- **The hook works:** GUI tasks could build `amb-gui` and run its tests.
- **Limits:** one run per task, and A1 left little room to improve. A harder task set would show more.

Next: C1 and C2 (RepoMapper, LSP) are unlikely to help where B1 already passes everything. Run them only with harder tasks, such as larger cross-crate changes or bugs with vaguer reports.

## Models

API prices per million tokens, input and output, from Anthropic's model table:

| Model | Input | Output |
|---|---|---|
| Fable 5.1 | $10 | $50 |
| Opus 5.5 | $4 | $20 |
| Sonnet 5.5 | $2 | $10 |
| Haiku 5.5 | $0.10 | $0.50 |

Haiku's price applies to prompts up to 100K tokens. Above that it costs $0.50 and $2.50. Plan limits may not follow these ratios exactly.

- **Main round on Sonnet 5.5, half Opus's price.**
  - Help should matter most to a model that finds its way less easily. A strong model may pass every task in every setup, which would show nothing.
  - If the help makes Sonnet reliable here, routine changes can run on Sonnet for good.
- **An Opus 5.5 check:** A against the winning setup, one run per task. Opus is what this repo is mostly worked on with, and help that suits one model can hurt another.
- **A Haiku 5.5 probe** on three or four easy tasks with the winning setup. It is nearly free. If Haiku passes with the recipes, the recipes carry the knowledge.
- **No Fable:** it costs 2.5 times Opus, and nobody would use it for routine work here.
- **Graders:** Sonnet or Haiku. Most checks are tests anyway.
- **Fixed settings:** the same model and effort level for every run in a comparison. Compare setups within one model, never across models.
- **Cost per completed task, not per token:** a cheap run that fails or wanders isn't cheap. A later arm could try Opus at a lower effort against Sonnet, measured the same way.

## Phases

0. Run the pilot.
1. Write the task set and the hidden checks.
2. Run A, then read the transcripts.
3. Write B from what the transcripts show.
4. Run B, C1 and C2, then compare.
5. Keep what helped:
   - write the findings in `docs/ai/agent-practices.md`;
   - add tests that keep the recipes and file summaries current, as the settings reference has.
6. Clean up the code, starting where agents struggled:
   - split `amb-gui/src/main.rs` and `ui/app.js` by concern;
   - use names that are easy to search for;
   - remove dead code.

   Then run A's tasks again to measure the cleanup.

## Before we start (owner)

- **Third-party tools:** RepoMapper and a rust-analyzer LSP plugin may be installed in the test sessions (owner, 2026-10-16). They stay out of the repo. RepoMapper's licence still needs checking.
- **Budget:** the pilot shows what a full round costs.
