# Custom interfaces: a whole new front end

A layout ([ui-layouts.md](ui-layouts.md)) moves the editor's own panels and controls around. A **custom interface** replaces the editor's window altogether: your own HTML, CSS and JavaScript, drawn however you like, talking to the same engine through the same commands. Use it when you, or an AI agent working for you, want something the built-in editor can't become: a one-slider app for a friend, a big-button touch editor, a culling screen, a kiosk.

The built-in interface is itself one of these: plain files in `crates/amb-gui/ui/`, no build step. Read it when you need an example of any command.

## What it is

A folder with an `index.html`, and whatever else it loads (scripts, styles, pictures, fonts) next to it. Paths are relative to the folder: `<script src="app.js">`, `<link href="css/look.css">`. Nothing outside the folder can be loaded from it.

An optional `interface.json` names it:

```json
{
  "name": "Minimal",
  "description": "A one-column editor: open a photo, a few sliders, undo, save.",
  "author": "Your name",
  "license": "GPL-3.0-or-later"
}
```

A working example to copy: [`examples/interfaces/minimal/`](../../examples/interfaces/minimal/) (about 120 lines of JavaScript).

## Using one

- **Settings → General → Interface.** Pick one from the list (the folders in `interfaces/` in the settings folder; **Open folder** shows it), or **Use a folder…** for one kept anywhere. The window switches at once. The open photo comes along, edits and history included.
- **Going back:** the interface's own button (every interface should have one), the menu bar's **Interface → Back to the Built-in Interface** (macOS always; Windows and Linux when the menu bar is on), or start the app with `ambrotype --builtin-ui` (or `AMB_BUILTIN_UI=1`), which ignores the choice for that run.

| OS | Installed interfaces |
|---|---|
| macOS | `~/Library/Application Support/org.ambrotype.editor/interfaces/` |
| Windows | `%APPDATA%\org.ambrotype.editor\interfaces\` |
| Linux | `~/.config/org.ambrotype.editor/interfaces/` |

**It is a program, not a theme.** It can do anything the built-in interface can: read and edit photos, write sidecars, change settings. So the app asks before switching to one. Use your own, or one whose code you have read. Layouts and themes are only data; interfaces are code.

## Talking to the app

The page runs in the app's own window, with `window.__TAURI__` there. Every call goes through one function and returns a promise:

```js
const { invoke } = window.__TAURI__.core;
const opened = await invoke("open_photo", { path: "/photos/IMG_0001.NEF" });
```

Arguments are named in camelCase (`maxEdge`, not `max_edge`). A failed call rejects with a message to show.

### The commands most interfaces need

| Command | Arguments | Gives back |
|---|---|---|
| `take_initial_path` | | The photo the app was started with, or the one open when the window switched; `null` if none. Call once at start. |
| `pick_photo` | | A path from the system's file dialog, or `null`. |
| `open_photo` | `path` | `{info, state}`: `info` has `name`, `path`, `camera`, `width`, `height`, `as_shot_temperature`, `as_shot_tint`, `defaults`; `state` as below. Opening the photo that's already open keeps its edits and history. |
| `execute` | `commands`, `group` (optional) | `{state, notes}`. Every edit goes through here: one call is one undo step. |
| `preview` | `maxEdge`, `overrides`, `before`, `clipping`, `maskOverlay` | The rendered photo, framed (below). Pass `overrides: {}`, `before: false`, `clipping: false`, `maskOverlay: null` for the plain edit. |
| `save_sidecar` | | The `.xmp` path written. |
| `editor_schema` | | Every setting by panel and section, with `name`, `label`, `kind`, `min`, `max`, `step`, `default`. Build controls from this; don't copy ranges into your code. |
| `library_list` | `dir`, `more` (optional list of folders), `subfolders` | The photos in folders, with capture details. |
| `thumbnail` | `path`, `edge`, `edited` | JPEG bytes. |
| `set_interface` | `path` (`null` for the built-in) | Switches the window. |

`state` is `{settings, can_undo, can_redo, history, redo_history, …}`; `settings` holds every setting the edit has changed, by name.

The commands for `execute` are the ones the CLI and AI tools use ([README.md](README.md), or `amb protocol schema`):

```js
await invoke("execute", { commands: [{ cmd: "set_param", name: "Exposure2012", value: 0.5 }] });
await invoke("execute", { commands: [{ cmd: "adjust_param", name: "Contrast2012", delta: 10 }] });
await invoke("execute", { commands: [{ cmd: "undo" }] });
await invoke("execute", { commands: [{ cmd: "apply_preset", path: "/presets/Warm.xmp", amount: 100 }] });
```

Settings use Lightroom's names (`Exposure2012`, `Temperature`, `ColorGradeShadowHue`); [settings-reference.md](settings-reference.md) lists them all.

### The preview

`preview` returns bytes: a 4-byte little-endian length, that much JSON (`width`, `height`, `millis`, `histogram`, …), then a JPEG.

```js
const buf = new Uint8Array(await invoke("preview", { maxEdge: 1600, overrides: {}, before: false, clipping: false, maskOverlay: null }));
const len = new DataView(buf.buffer, buf.byteOffset, 4).getUint32(0, true);
const meta = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + len)));
img.src = URL.createObjectURL(new Blob([buf.subarray(4 + len)], { type: "image/jpeg" }));
```

Ask for one preview at a time and drop the in-between ones while a slider moves (the example shows how). `overrides` (`{"Exposure2012": 1}`) previews a value without making it an edit: use it while dragging, then `execute` on release.

### Events

```js
window.__TAURI__.event.listen("sidecar-changed", () => { /* the .xmp changed on disk: reopen */ });
```

`sidecar-changed`, `folder-changed` (after `watch_folders`), `index-progress` and `index-finished` (the library catalogue).

### Everything else

Export, masks, crop, presets, the library catalogue, collections, plugins: the full list is the `generate_handler!` block in `crates/amb-gui/src/main.rs`, and each is used somewhere in `crates/amb-gui/ui/`. Search the UI for `invoke("name"` to see its arguments in use.

## Rules for AI agents

- **Write the interface into its own folder:** `interfaces/<name>/` in the settings folder, or a path the user names. Never edit `crates/amb-gui/ui/` for this.
- **Always give a way back:** a visible button that calls `invoke("set_interface", { path: null })`.
- **Edits go through `execute`.** Don't write sidecars or settings files yourself.
- **Read ranges and labels from `editor_schema`**, not from memory.
- **Load nothing from the internet.** Keep scripts, styles and fonts in the folder, so it works offline and does only what its files say.
- **Test it:** choose it in Settings → General → Interface, open a photo, move a control, undo, save. If the window shows nothing, start with `--builtin-ui` and look at the page in the built-in one's place (the files are served exactly as they are).

## Sharing

Interfaces can go on the hub like layouts and themes; Browse hub… shows their files before you install, with the same warning. How to submit one: the hub's README.
