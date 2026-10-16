# Helping agents find their way: a plan

Status: a plan, not started (2026-10-16).

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

- **Third-party tools:** may RepoMapper and a rust-analyzer LSP plugin be installed in the test sessions? They are third-party and stay out of the repo. RepoMapper's licence still needs checking.
- **Budget:** the pilot shows what a full round costs.
