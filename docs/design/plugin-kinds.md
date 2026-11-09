# Plugin kinds

What a plugin can be, and why these. Decided 2026-11-04, when the owner asked for tabs (tasks) as plugins and for the kinds to be thought through. How to write each is in [ai/plugins.md](../ai/plugins.md).

## The kinds

| Kind | What it is | Runs code? | Where it shows | Examples |
|---|---|---|---|---|
| `action` | A button that makes edits once (a set of protocol commands, one undo step) | Yes, sandboxed WebAssembly | Plugins dialog; a task's tool button | Auto tone, Random look, Film recipe |
| `effect` | A look rendered with the photo every time, kept in its edit (`amb:Effects`); `color` (a colour table) or `pixels` | Yes, sandboxed WebAssembly | Effects → Plugin effects; a task's tool button | Duotone, Light leak, Hue trip, Holographic |
| `task` | A tab in the task bar: a layout and tool buttons | No | The task bar (Ctrl+Alt+6–9) | Trippy |

A **task** is data only, so it can't do harm and needs no approval: its layout is checked like a layout file, and each tool does one of three things, all of which go through the usual commands:

- `command`: an editor command by its shortcut id (Keyboard Shortcuts lists them), such as `auto-edit`, `remove-object`, `trip-me`;
- `effect`: an effect plugin put on the photo from one of its recipes or with `params`, in place of the one of that plugin already there;
- `plugin`: an action plugin run with `params`.

So a task is how someone bundles a way of working: Trippy is psychedelic effects with the Effects panel open; a "Wedding" task could be a layout with Remove object, a skin preset's plugin and an export command.

## Why three, and not more

- **One kind per place in the app.** Actions are run, effects are rendered, tasks are shown. Each has its own rules (effects can't ask for permissions because they run on every render with no one to ask; tasks have no code at all).
- **Tasks don't run code.** A tab that ran code would need its own panel API. Buttons that call commands, effects and action plugins cover what a task needs, and keep tasks safe to share on the hub.
- **Not yet:**
  - `exporter` (a file format or a web service): needs file and network permissions we don't give plugins yet;
  - `importer` (a catalogue from another app): the same;
  - `panel` (free-form controls in a column): needs a UI API in the sandbox ([GAPS.md](../GAPS.md)).
  Each would be a new kind when it comes, with its own rules, rather than one broad kind that can do everything.

## Built-in tasks

Library, Develop, Color, Retouch, Film and Collage are built into the app (their layouts come from the hub). Trippy is the first task plugin (`plugins/trippy`), built in to show the kind; installed task plugins follow it in the task bar.
