# The docs, and what each one is for

One home per fact. If something is written in two places, one of them goes out of date. So each doc below has one job; the others link to it.

## Where to look

| You want | Read | It changes when |
|---|---|---|
| To start work on the code | [AGENTS.md](../AGENTS.md), then [WHATS-NEXT.md](WHATS-NEXT.md) | rarely; every session |
| What to do now, and what waits on the owner | [WHATS-NEXT.md](WHATS-NEXT.md) | every session |
| Everything planned, in order, with who can do it | [ROADMAP.md](ROADMAP.md) | an item is added, readied or done |
| What is missing or approximate in what exists | [GAPS.md](GAPS.md) | a gap closes or is found |
| What the app can do, where it is, its `amb` command | [FEATURES.md](FEATURES.md) | a feature people use lands |
| How the code is built: crates, the pipeline, each part's files | [ARCHITECTURE.md](ARCHITECTURE.md) | what exists changes |
| How code is written here, and the checks that hold it | [codestyle.md](codestyle.md) | a rule changes |
| The steps of a common change (a setting, a command, a button) | [recipes.md](recipes.md) | the places a change touches move (a test checks) |
| Small cleanup tasks, ready for any model | [cleanup.md](cleanup.md) | a task is added or done |
| Which model does which work; how to write a task | [delegation.md](delegation.md) | the models or the loop change |
| How one feature is designed, and why | `design/<feature>.md` | that feature's design changes |
| What is a tab, a mode, a view, a tool, a dialog | [design/tabs.md](design/tabs.md) | a tab is proposed |
| Why the project exists and what was decided at the start | [SCOPE.md](SCOPE.md) | never: it is dated |
| Legal and licence questions | [legal.md](legal.md) | a question is settled |
| Building, releasing, setting up a desktop | [desktop-setup.md](desktop-setup.md), [releasing.md](releasing.md), [release-notes/](release-notes/) | a release |
| Measuring Lightroom during a trial | [lightroom-trial-plan.md](lightroom-trial-plan.md), [design/calibration.md](design/calibration.md) | the plan changes |
| Other projects, and when we last looked | [research/watchlist.md](research/watchlist.md); the numbered files in [research/](research/) are dated and not updated | the monthly recheck |

**For people using the app:** [make-it-yours.md](make-it-yours.md) (layouts, themes, the hub), [own-models.md](own-models.md) (your own AI models).

**For an AI driving the app** (not changing its code): [ai/README.md](ai/README.md) first. These are built into the program (`amb docs`); a new one is listed in `crates/amb-protocol/src/guides.rs`. `ai/core.md` and `ai/settings-reference.md` are generated: never edit them by hand.

## Rules

1. **One home per fact.** Before writing something down, find its home in the table. Elsewhere, link.
2. **Open work has three homes, by time:** now is WHATS-NEXT, later is ROADMAP, missing-in-what-exists is GAPS. A design doc may say what its own feature doesn't do yet; the roadmap or GAPS row links to it and doesn't repeat it.
3. **Done work leaves the planning docs.** It becomes a line in WHATS-NEXT's "Recently done" (the last 15 stay), a row in FEATURES and, if it changed how the code is built, a line in ARCHITECTURE. Git has the rest.
4. **No counts by hand.** Line counts, test counts and file sizes go stale in a week. Say "run `python3 tools/code-metrics.py`", or date the number.
5. **A dated doc stays as it was.** SCOPE, the numbered research files and `design/structure.md` are snapshots. They start with a line saying so, and what is current instead.
6. **Paths are written in full** from the repository's root (`crates/amb-gui/ui/tools/crop.js`, not `tools/crop.js`), so a search and a test can find them.
7. **Plain, short sentences.** No model names. A new doc gets a row in the table above (a test checks).
