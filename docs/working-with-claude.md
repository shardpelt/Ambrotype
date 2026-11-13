# Working with Claude: a short guide for the owner

One page, things to do. Which model does which work, in full, is in [delegation.md](delegation.md); this page is what to type.

## 1. Which model when

| Situation | Model | What to type |
|---|---|---|
| A routine card or a small fix | Medium (Sonnet) | "do the next card in docs/cleanup.md" |
| A new feature idea, or a decision to make | Large (Opus), one short chat | "I want X. Ask me what you need, then write the card or brief into docs/." Then a new medium chat: "build the card for X" |
| Reading, summaries, a docs row, Life-OS style upkeep | Small (Haiku) | "read docs/WHATS-NEXT.md and tell me in five lines what is waiting on me" |
| An audit of everything, or many briefs at once | Top (the newest model), rarely | "audit the codebase for X and write the findings as cards in docs/cleanup.md" |

If unsure, start medium. If it gets stuck on a choice, it says so; take that to a large chat.

## 2. When to start a new chat

- **New task: new chat.**
- **The chat has compacted once: new chat.**
- **Back after an hour: new chat.** The saved context has expired, so the first message pays to read everything again.
- **Stay only** for the same task in the same sitting.
- Favourite chats are bookmarks to read, not places to keep typing.

## 3. Before you leave a chat

If the state is in the repo (`docs/WHATS-NEXT.md`, `docs/cleanup.md`, `docs/ROADMAP.md`), there is nothing to summarise. Just leave.

If something lives only in the chat, say "write what is not in the repo yet into docs/WHATS-NEXT.md", then start fresh.

- `/clear` starts fresh in the same window.
- `/compact` keeps a summary but costs a large request. Prefer `/clear` plus the repo.

## 4. What makes it cheap or expensive

Cheap:
- One task per chat.
- A tight brief (section 5).
- Small models read; tests judge.
- State kept in the repo.

Expensive:
- Long chats.
- Vague asks.
- Several agents at once near the limit.
- Switching model in the middle of a task.
- Resuming day-old chats.

## 5. Asking well

Say: one outcome; what done looks like; what already exists; what is yours to decide; where to stop.

| Weak | Better |
|---|---|
| "side scroll is acting weird in map view" | "The row of photos under the map doesn't scroll with the wheel or by dragging, like the filmstrip does. Make it scroll the same way. Done when both work in the UI test. Stop after the row; leave the map alone." |
| "make export better" | "Export to JPEG keeps the sidecar's crop but ignores Resize in the dialog. Make `amb export --long-edge 2000` and the dialog agree. Reuse the existing resize in `amb-export`; ask me before changing the sidecar format." |
| "clean up the docs" | "Add a row for each doc in docs/ missing from docs/README.md, and fix links that don't resolve. Don't rewrite any doc's text. Stop when the docs test passes." |

## 6. When the limit is near

Say "land safely". Running work is stopped cleanly, the state is written to the repo and pushed. `/usage` shows what is left.

## 7. The 5-hour window

All models share one 5-hour window. A big parallel run can use it up in under an hour. Plan large runs at the start of a window, and keep the end of it for small things.

## Related

Where changes land and how you take them to `main`: [branches.md](branches.md).
