# Layouts and themes ("looks"): format reference

How to make a layout or theme for the Ambrotype editor, by hand or by asking an AI assistant ("give me a compact layout with knobs for the tone sliders and a warm dark theme"). Design background: [design/customization-and-hub.md](../design/customization-and-hub.md). Validation and storage: `crates/amb-gui/src/looks.rs`.

A look is two small JSON files:
- a **layout**: what goes where, and which kind of control each setting uses
- a **theme**: colours

They contain no code and no CSS, so they are safe to share. Built-in looks can't be changed; clone one in the Appearance dialog, or write a new file.

Want more than rearranging the editor's own controls? A **custom interface** replaces the whole window with your own HTML, CSS and JavaScript: [custom-interface.md](custom-interface.md).

**People don't need this file** (their guide: [make-it-yours.md](../make-it-yours.md)). **Edit layout…** in the Appearance dialog (or Ctrl+Shift+L; an `edit` toolbar button if a layout lists one) does all of it by hand:
- drag panels, blocks, settings and toolbar buttons to move them
- click one to change its control type, label, title or columns, or to split or merge panels
- add what isn't shown from the **Add** list
- drag a column's edge to resize it

Saving writes the same JSON described below. A built-in layout is saved as your own copy.

## Where the files go

Your own looks live in the app's settings folder, under `looks/layouts/` and `looks/themes/`. **Appearance → Open folder** shows it.

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/org.ambrotype.editor/looks/` |
| Windows | `%APPDATA%\org.ambrotype.editor\looks\` |
| Linux | `~/.config/org.ambrotype.editor/looks/` |

- **File name:** the file name (without `.json`) becomes the look's id, `user:<name>`. Use lower case and dashes.
- **New files:** after adding a file, press **Reload** in the Appearance dialog, or use **Import…** for a file kept somewhere else.
- **Check a file before using it.** This prints every problem and exits with status 1 if there are errors:
  ```sh
  ambrotype --check-look my-layout.json my-theme.json
  # or from the repo: cargo run -p amb-gui -- --check-look my-layout.json
  ```

## Rules for AI assistants

- **Writing files:** only write files into the `looks/` folder, or to a path the user names. Don't edit the built-in files in the repo unless the user is changing Ambrotype itself.
- **Before handing a file over:** run `--check-look` and fix every *error*. *Warnings* are allowed (unknown names are skipped or shown as placeholders), but say what they are.
- **Use exact setting names:** take them from [settings-reference.md](settings-reference.md) and the panel table below. Don't invent settings: a layout can only show settings the editor has controls for.
- **Each setting once.** A layout can show each setting only once, and the point curve and the colour wheels once each.

## Layout file

```jsonc
{
  "format": "ambrotype-layout/1",          // required
  "name": "Knob desk",                       // required, shown in the list
  "description": "Knobs for tone, bars for presence.",   // optional
  "author": "you",                           // optional
  "based_on": "builtin:studio",              // optional; set by Clone
  "theme": "builtin:dark",                   // optional: switching to this layout also picks this theme
  "density": "normal",                       // "compact" | "normal" | "large"
  "toolbar_style": "icons",                  // "icons" (where one is obvious) | "words"
  "toolbar": ["open", "library", "save", "export", "collage", "|", "undo", "redo", "|", "before", "crop", "masks", "spots", "zoom", "wb", "clip", "frame", "|", "hub", "plugins", "|", "looks", "edit", "settings", "keys", "feedback"],
  "left":  { "width": 250, "items": ["presets", "history"] },
  "right": { "width": 340, "items": ["histogram", { "panel": "basic", "open": true }, { "panel": "detail" }] }
}
```

### Filmstrip

`filmstrip`: `true` (the default) shows the other photos of the folder under the photo; `false` hides it. F shows or hides it for the session.

### Tabs

A column's items can include `{ "tab": "Name" }` (1 to 24 characters, each name once per column). The items after it, up to the next tab, show while that tab is chosen; items before the first tab always show (the histogram, say). The tabs sit in one bar at the top of the column, where the first one is, and the chosen tab is remembered per layout on this computer. Search, and keys that pick a slider, bring its tab forward. Classic and Minimalism use three: Tone & colour, Detail & lens, More.

```json
"right": { "width": 330, "items": ["histogram", { "tab": "Tone" }, { "panel": "basic", "open": true }, { "tab": "Detail" }, { "panel": "detail" }] }
```

### Chrome

`chrome`: `"normal"` (the default) or `"hidden"`. Hidden shows only the photo, edge to edge: no top bar, status bar or column edges. The side columns slide in over the photo while the pointer is at the window's left or right edge (or with Ctrl+[ and Ctrl+]), and the top bar while it's at the top. Shortcuts all work. The Minimalism layout uses it.

### Density

`toolbar_style` shows the toolbar as icons where an icon is obvious (the default) or as `words` only, as the Beginner layout does. Each button's name is always in its tooltip.

`density` sets the size of text and controls: `compact` for small laptop screens, `normal` (the default), or `large` for touch screens or easier reading.

### Toolbar

- **Buttons**, in order:

  | Name | Button |
  |---|---|
  | `open` | Open |
  | `library` | The library grid |
  | `save` | Save sidecar |
  | `export` | Export |
  | `collage` | The collage maker |
  | `undo`, `redo` | Undo, Redo |
  | `before` | Before/After |
  | `crop` | Crop tool |
  | `masks` | Mask tool |
  | `spots` | Spot removal (heal and clone) |
  | `zoom` | 100% view |
  | `wb` | White-balance picker |
  | `clip` | Clipping overlay |
  | `frame` | Frame: a border and text around the photo, previewed outside the edit |
  | `hub` | Browse: everything other people shared in the hub (layouts, themes, presets, collages, sizes, plugins) |
  | `plugins` | The Plugins dialog |
  | `looks` | The Appearance dialog (layouts and themes) |
  | `edit` | Editor mode (rearrange the layout in place) |
  | `settings` | Settings, including the keyboard shortcuts |
  | `keys` | All keyboard shortcuts on one card |
  | `feedback` | Feedback |

- `"|"` is a separator.
- Buttons left out are hidden, but their keyboard shortcuts still work.
- `looks` is always added if missing, so a layout can't lock you out.

### Columns

`left` and `right` each have:
- `width`: in px, 0–800.
- `items`: a list, shown top to bottom.

A column with width 0 or no items is hidden. A shown column can be folded away for the session and back with the arrow on the photo's edge (Ctrl+[ left, Ctrl+] right); the editor remembers it on this computer. Drag the line between two blocks in a column (presets, rating, history) to resize them; double-click it to reset. Each item is one of:
- The strings `presets`, `info`, `histogram` or `history`: the presets list (with the Amount slider), the rating stars and keyword field, the histogram with the photo's camera info, or the edit history. Each appears at most once; leave one out to hide it.
- **A built-in panel:** `{ "panel": "<id>", "title"?, "open"?, "widgets"?, "hide"? }`
  - `open`: true to start expanded.
  - `widgets`: maps setting names to a control type, e.g. `{ "Exposure2012": "knob" }`.
  - `hide`: setting names to leave out. A hidden setting can then be shown in another panel.
- **Your own panel:** `{ "title": "...", "open"?, "columns"?, "items": [...] }`.
  - `columns` (1–4) lays knobs out in a grid; everything else spans the full width.
  - Each item in `items` is one of:
    - `{ "setting": "Exposure2012", "widget"?: "knob", "label"?: "Exp" }`: one setting.
    - `{ "include": "basic" }` or `{ "include": "basic/Tone" }`: a whole built-in panel, or one of its sections.
    - `{ "widget": "point-curve" }` or `{ "widget": "color-wheels" }`: the tone-curve editor (`point-curve`) or the colour-grading wheels (`color-wheels`).
    - `{ "label": "Tone" }`: a small section heading.

The crop panel isn't placed by layouts. It appears at the top of the column with the most panels, just below the histogram if that column has it.

### Built-in panels and their sections

| Panel id | Sections (for `include: "id/Section"`) | Comes with |
|---|---|---|
| `basic` | `Profile`, `White Balance`, `Tone`, `Presence` | |
| `tone_curve` | `Parametric` | the point-curve editor |
| `color_mixer` | `HSL` | Hue / Saturation / Luminance tabs |
| `black_white` | `Treatment`, `Mix` | the black & white switch and the eight mix sliders |
| `color_grading` | `Shadow`, `Midtone`, `Highlight`, `Global`, `Mix` | the colour wheels (instead of the Shadow…Global sliders) |
| `detail` | `Sharpening`, `Noise Reduction` | |
| `lens` | `Profile`, `Manual`, `Defringe` | |
| `transform` | `Upright`, `Transform` | |
| `effects` | `Post-Crop Vignetting`, `Grain`, `Plugin effects` | The effect plugins on the photo (`amb:Effects`), each with its own controls |
| `calibration` | `Shadows`, `Red Primary`, `Green Primary`, `Blue Primary` | |
| `negative` | `Conversion`, `Print` | |

- Including a whole panel brings its composite widget (the curve or the wheels).
- Including one section brings only that section's sliders, so `color_grading/Shadow` gives three plain sliders.

### Control types

Each setting is a number, an on/off switch, or a choice; [settings-reference.md](settings-reference.md) says which. The first type listed is the default.

| Setting type | Widgets |
|---|---|
| number | `slider` (label, slider, value box; Shift for fine), `knob` (drag up/down; Shift for fine), `bar` (a filled bar with the label inside; drag across), `number` (a value box; drag the label to scrub) |
| on/off | `checkbox`, `toggle` (a switch) |
| choice | `select` (a drop-down), `segmented` (a row of buttons) |

A widget of the wrong type (for example `toggle` on Exposure) is an error. An unknown widget name is a warning, and the default is used.

## Theme file

```json
{
  "format": "ambrotype-theme/1",
  "name": "Warm dark",
  "scheme": "dark",
  "tokens": {
    "bg": "#1d1a17", "bg-deep": "#13110f", "viewer": "#13110f", "panel": "#25211d", "panel-2": "#2d2823",
    "line": "#3a332c", "hover": "#352f29", "tab-active": "#40382f",
    "text": "#ece6df", "muted": "#a89e93", "faint": "#776d63",
    "accent": "#e09a4f", "accent-ink": "#1d1a17", "danger": "#e5675a",
    "thumb": "#e2dbd3", "track": "#4f463d", "radius": 6
  }
}
```

- **Three colours are enough.** A theme can give only `bg`, `text` and `accent`; the editor works out the other tokens from them (steps between the background and the text, matched to the Dark and Light themes). Any token you do give is kept, so start with three and set more only where the result isn't what you want:
  ```json
  { "format": "ambrotype-theme/1", "name": "Mint", "tokens": { "bg": "#102020", "text": "#e0f0f0", "accent": "#40d0a0" } }
  ```
- **`scheme`:** `dark` or `light`; when missing, it follows the background's brightness; it sets the colour of native controls such as scrollbars and drop-downs.
- **Colour token values:** hex colours (`#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`).
- **`radius`:** the corner rounding in px, 0–16.
- **Missing tokens** are worked out from `bg`, `text` and `accent` (those three come from Dark, or Light for a light theme, when missing too).
- **What each token colours:**

  | Token | Colours |
  |---|---|
  | `bg` | window background |
  | `bg-deep` | inputs, histogram and lists |
  | `viewer` | around the photo; a mid grey helps judge colour |
  | `panel`, `panel-2` | side columns, and buttons and raised areas |
  | `line` | borders |
  | `hover` | button hover |
  | `tab-active` | the selected tab or segment |
  | `text`, `muted`, `faint` | three text strengths |
  | `accent` | primary buttons, changed settings, knob and bar fills |
  | `accent-ink` | text on the accent |
  | `danger` | errors |
  | `thumb`, `track` | slider knobs and tracks |

## Sharing

- **Share…** in the Appearance dialog saves the file anywhere, ready to send; **Import…** adds one.
- **Browse** in the toolbar (Ctrl+Shift+H; also **Browse hub…** in the Appearance and Plugins dialogs) installs what other people shared, and **Submit to hub…** offers one of yours as a pull request to the hub repo. How the hub repo works: [its README](https://github.com/Ambrotypehq/ambrotype-hub#readme).
- **From a link:** at the bottom of the Browse dialog, paste a link to a layout or theme file, or to a GitHub repo with a `theme.json` or `layout.json` at its top (`https://github.com/you/your-theme`). It's checked like any other look, installed and switched to. So you can share a look from your own repo without going through the hub.
