---
name: brief
description: Turn a rough ask into a brief or task card that a cheaper model can build without questions. Use when the user says "brief", "write a card", "plan this for sonnet", or describes a feature or fix they want handed to another session.
---

# Brief

Your output is one brief (or a few cards). You do not build. A good brief makes the build cheap; a vague one makes it expensive twice.

## 1. Pin the ask down

- Write the user's request down verbatim. It goes into the brief unchanged.
- Name the exact thing: which screen, which part, which command. If their words fit two things, look at the code or the feature first; if both still fit and lead to different work, ask one question. Never brief a guess.
- Say what "good" means as something checkable (a command's output, a test, a number).

## 2. Look, briefly

- Check the repo's rules: AGENTS.md, a code style doc, a docs map, a tasks/cards doc. Follow their card format if they have one.
- Find what exists: grep the names, read ~20 lines around each hit. List files by full path.
- Find the nearest finished example of the same kind of change. Name it in the brief as "the example to copy".
- Check for a shared part before designing a new one (a helper, a component, a table). If one exists, the brief says "use X". If two copies exist already, the brief says which becomes the one.
- Send a small model to read anything over ~300 lines and report the few facts you need.

## 3. Decide

Everything a builder could hesitate over is decided here:
- Where each piece goes and what it is called.
- Signatures, command names, data shapes, element ids.
- What must not change.
- Order of steps, and where commits split.
- The smaller option, unless the user asked for the larger.

If you cannot decide something, it is a question for the user, not a line in the brief.

## 4. Write it

```
### ID. Title
Tier: small | medium | large        Risk: low | medium | high
Asked (verbatim): "…"
Why: one or two sentences.
Read first: 2–5 files, by path. The example to copy: path.
Files: every file to change or create, by path.
Design (decided): names, signatures, where things go.
Steps: numbered, exact.
Use: the shared parts and helpers that apply. Don't add: the patterns banned here.
Don't: what must not change (names, formats, behaviour).
Tests: what to add, where.
Docs: which rows or lines, in which files.
Checks: exact commands, and what a clean result prints.
Done when: what is true afterwards, checkable.
Stop and report if: a step doesn't match the code; a check fails twice; the change spreads past the files listed.
Report: at most N lines: commit, each check's result, choices the brief didn't make, what looked wrong but was left.
Commit: `area: summary`
```

Rules:
- One brief, one outcome, one commit (or a short ordered list).
- Exact names are grepped before they are written. A wrong name costs more than no name.
- No "decide", "choose", "consider", "as appropriate".
- Tier honestly: small = no choices at all; medium = follows a recipe or this brief; large = needs judgement you could not remove.
- Big ask? Split into cards that each land green on their own, in order. Mark which can run in parallel (no shared files).

## 5. Check the brief before handing it over

- Could a model that has never seen this repo finish it? Read it as that model.
- Is every path real? Is every name real?
- Does "Done when" fail today and pass after?
- Does it add a second copy of something that exists?
- Is anything the user said missing?

## 6. Hand over

- Put the brief where the repo keeps them (a cards doc), or give it to the user to paste into a new chat on the tier named.
- Tell the user in three lines: what it will build, what you decided for them, what to say to start it.
