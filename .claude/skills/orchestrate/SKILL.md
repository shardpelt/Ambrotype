---
name: orchestrate
description: Run a feature, fix or refactor as lead: decide, brief cheaper agents, verify, land. Use when the user asks to build, add, fix or refactor something larger than a few lines, says "orchestrate", or asks for the most token-efficient way to get a change done.
---

# Orchestrate

You are the lead. Your tokens go into decisions; cheaper models do the reading and typing; tests judge. The user pays for every token and has a 5-hour limit, so waste is the main failure.

## 0. Before anything: do I delegate at all?

Do it yourself, no agents, when any of these holds:
- The change is under ~30 lines in files you have already read.
- It is a question, an explanation or a one-file fix.
- You would spend longer writing the brief than making the change.

Delegate when:
- The work means reading more than ~3 files or ~500 lines you have not read.
- It is mechanical and wide (a sweep, a rename, a split, docs rows).
- Two or more parts are independent and can run at once.
- It needs a build-and-test loop of minutes (the waiting costs you nothing if an agent does it).

Never delegate the decision itself. If a brief would contain "decide", "choose" or "figure out the best way", you are not done thinking.

## 1. Understand the ask (cheap, yours)

- Restate the request in one line. Quote the user's words in the brief later, verbatim.
- If the words can mean two things, look for which: read the screen/feature they mention before choosing. A wrong reading costs a whole build. If still unclear and the two readings lead to different work, ask one question.
- Find what exists: grep names, read the 20 lines around them. Do not read whole files; an agent will.
- Check the repo's own rules first (AGENTS.md, a code style doc, a tasks/cards doc). They outrank this skill on conventions.

## 2. Decide the design (yours)

Write down, for yourself, in a few lines:
- Where each part goes (which crate/module/file), and what it is called.
- The interface between parts (function signatures, command names, data shapes).
- What must not change (names, formats, behaviour other code relies on).
- What "done" is, as something a command can check.
- The smaller option whenever two would do.

## 3. Pick the model per piece

| Piece | Model |
|---|---|
| Reading, summarising, collecting, link checks, docs rows, regenerating generated files | small (haiku) |
| Building from a brief, tests, splits, sweeps, a bug with a failing test | medium (sonnet) |
| New algorithm, cross-cutting refactor without a brief, a bug nobody can reproduce | large (opus), or you |
| Design, the brief, the review of risky diffs | you |

Always pass the model explicitly when spawning. Never let a subagent inherit the top model by default.

## 4. Write the brief

One brief = one outcome = one commit (or a short ordered list of them). A good brief lets a model that has never seen the repo finish without asking. Include, in this order:

1. Where to work (path, branch), what not to touch, "do not push".
2. What to read first (2 to 5 files, by path).
3. The user's request, verbatim.
4. What exists and what is wrong, with file paths.
5. The design, marked as decided: names, signatures, where things go.
6. Rules that bite here (helpers to use, patterns banned, size limits).
7. Tests to add.
8. Docs to update.
9. Checks to run, the exact commands, and what a clean result prints.
10. The commit message.
11. When to stop and report instead of guessing: a step that does not match the code; a check that fails twice; a change spreading beyond the listed files.
12. The report: at most N lines, what to include (commit hash, each check's result, choices the brief did not make, what looked wrong but was left).

Keep facts exact. A wrong line number is fine ("near"), a wrong name is not: grep it first.

## 5. Run it

- **Parallel only when files do not overlap.** Overlap means merge work for you. Same files: run in order.
- One build tool owner per checkout. A second parallel builder gets its own worktree and build folder.
- At most 2 or 3 agents at once: they share the user's limit and the machine.
- Start agents in the background. Do not poll them; you are told when they finish. Do not read their transcripts.
- While they run, do only work that does not touch their files. Otherwise stop talking: every turn of yours re-reads your whole context.
- Do not commit an agent's half-done files. Explain once if a hook asks.

## 6. Verify (trust, then check)

Tests judge first; you read only what tests cannot see.

**The report**
- Was the full check run on the *final* commit? If the log names an older commit, run it yourself in the background.
- "Failed once, passed alone" is not passed. It is fixed, or written down as open and kept off the protected branch.
- Did it do what the user asked, in the user's words? Re-read the verbatim request against the report.
- Choices the brief did not make: agree with each, or send it back.

**The diff: skim only the risky parts**
- Anything that deletes, moves or overwrites files; writes outside its own folder; reads input from outside (paths, files, network); changes a file format or a saved setting.
- New public names, commands, element ids: are they registered wherever the repo lists them?

**Design patterns: five questions per change**
1. **A second copy?** Did it write something that already exists (a helper, a component, a loader, a selection, a progress bar, a dialog shell)? Grep for the shape, not the name. If yes: use the part, or extend it.
2. **Right layer?** Is the logic where the repo says logic goes (a library, not the UI or the command-line glue)? Can every front end reach it?
3. **Data or code?** Is a growing list written as a table or file that code reads, not as an if-chain?
4. **One home?** Is each fact, text and constant in one place? Any number or label typed twice?
5. **State and wiring?** One state object per feature, no loose globals; listeners attached in one place; nothing left running when the view closes.

Then:
- A rule that was broken and has no test: add the guard (a count that may only fall, a list checked against the code) in the same landing, and name the shared part in the code style doc. A rule only in prose gets broken again.
- A pattern you see three times with no shared part: write a card for it; do not widen this change.

**Combined tree**
- Merge, run the checks once on the result, then push. Several agents' work that passed alone can fail together.

## 7. Land and hand off

- State lives in the repo (the project's next-steps or tasks doc), not the chat. If work stops unfinished, write there: what is done, what failed, the exact next step.
- Report to the user: what changed, where to find it, what was not verified, what needs them. Short.
- New task: tell the user to start a new chat. Long chats re-read everything each turn; after an hour idle the cache is gone and the first message pays for all of it.

## Costs to watch (each has happened)

| Waste | Avoid by |
|---|---|
| Building the wrong reading of the ask | step 1: look before choosing; quote the user |
| Vague brief, agent explores and guesses | step 4: decided design, stop rules |
| Agent re-does a finished doc or plan | say what exists, by path |
| Lead reads files an agent will read anyway | grep and 20 lines, not the file |
| Reviewer re-reads the codebase | review the diff and the report only |
| Parallel agents all hit the limit mid-work | 2 or 3 at most; each leaves commits, not loose edits |
| Resuming a dead agent or a day-old chat | a fresh agent with a tight brief is usually cheaper |
| A finished agent keeps reporting | stop it |
| Narrating while waiting | say it once, then wait |

## When the limit is near

Tell running agents: finish the command in hand, start nothing new, commit what is checked, undo what is not, report in 8 lines. Write the open state into the repo. Push. Then stop.
