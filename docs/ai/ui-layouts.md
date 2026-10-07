# Layouts and themes ("looks"): format reference

How to make a layout or theme for the Ambrotype editor, by hand or by asking an AI assistant ("give me a compact layout with knobs for the tone sliders and a warm dark theme"). Design background: [design/customization-and-hub.md](../design/customization-and-hub.md). Validation and storage: `crates/amb-gui/src/looks.rs`.

A look is two small JSON files:
- a **layout**: what goes where, and which kind of control each setting uses
- a **theme**: colours

They contain no code and no CSS, so they are safe to share. Built-in looks can't be changed; clone one in the Looks dialog, or write a new file.

**People don't need this file.** **Edit layout** in the toolbar (or in the Looks dialog) does all of it by hand:
- drag panels, blocks, settings and toolbar buttons to move them
- click one to change its control type, label, title or columns, or to split or merge panels
- add what isn't shown from the **Add** list
- drag a column's edge to resize it

Saving writes the same JSON described below. A built-in layout is saved as your own copy.

## Where the files go

Your own looks live in the app's settings folder, under `looks/layouts/` and `looks/themes/`. **Looks → Open folder** shows it.

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/org.ambrotype.editor/looks/` |
| Windows | `%APPDATA%\org.ambrotype.editor\looks\` |
| Linux | `~/.config/org.ambrotype.editor/looks/` |

- **File name:** the file name (without `.json`) becomes the look's id, `user:<name>`. Use lower case and dashes.
- **New files:** after adding a file, press **Reload** in the Looks dialog, or use **Import…** for a file kept somewhere else.
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
  "toolbar": ["open", "save", "export", "|", "undo", "redo", "|", "before", "crop", "zoom", "wb", "clip", "|", "looks", "edit", "settings", "feedback"],
  "left":  { "width": 250, "items": ["presets", "history"] },
  "right": { "width": 340, "items": ["histogram", { "panel": "basic", "open": true }, { "panel": "detail" }] }
}
```

### Density

`density` sets the size of text and controls: `compact` for small laptop screens, `normal` (the default), or `large` for touch screens or easier reading.

### Toolbar

- **Buttons**, in order:

  | Name | Button |
  |---|---|
  | `open` | Open |
  | `save` | Save sidecar |
  | `export` | Export |
  | `undo`, `redo` | Undo, Redo |
  | `before` | Before/After |
  | `crop` | Crop tool |
  | `zoom` | 100% view |
  | `wb` | White-balance picker |
  | `clip` | Clipping overlay |
  | `looks` | The Looks dialog |
| `edit` | Editor mode (rearrange the layout in place) |
| `settings` | Settings, including the keyboard shortcuts |
  | `feedback` | Feedback |

- `"|"` is a separator.
- Buttons left out are hidden, but their keyboard shortcuts still work.
- `looks` is always added if missing, so a layout can't lock you out.

### Columns

`left` and `right` each have:
- `width`: in px, 0–800.
- `items`: a list, shown top to bottom.

A column with width 0 or no items is hidden. Each item is one of:
- The strings `presets`, `histogram` or `history`: the presets list (with the Amount slider), the histogram with the photo's camera info, or the edit history. Each appears at most once; leave one out to hide it.
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
| `basic` | `White Balance`, `Tone`, `Presence` | |
| `tone_curve` | `Parametric` | the point-curve editor |
| `color_mixer` | `HSL` | Hue / Saturation / Luminance tabs |
| `color_grading` | `Shadow`, `Midtone`, `Highlight`, `Global`, `Mix` | the colour wheels (instead of the Shadow…Global sliders) |
| `detail` | `Sharpening`, `Noise Reduction` | |
| `lens` | `Profile`, `Manual`, `Defringe` | |
| `effects` | `Post-Crop Vignetting`, `Grain` | |
| `negative` | `Conversion`, `Print` | |

- Including a whole panel brings its composite widget (the curve or the wheels).
- Including one section brings only that section's sliders, so `color_grading/Shadow` gives three plain sliders.

### Control types

Each setting is a number, an on/off switch, or a choice; [settings-reference.md](settings-reference.md) says which. The first type listed is the default.

| Setting type | Widgets |
|---|---|
| number | `slider` (label, slider, value box), `knob` (drag up/down; Shift for fine), `bar` (a filled bar with the label inside; drag across), `number` (a value box; drag the label to scrub) |
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

- **`scheme`:** `dark` or `light`; it sets the colour of native controls such as scrollbars and drop-downs.
- **Colour token values:** hex colours (`#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`).
- **`radius`:** the corner rounding in px, 0–16.
- **Missing tokens** fall back to the Dark theme.
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

- **Share…** in the Looks dialog saves the file anywhere, ready to send; **Import…** adds one.
- A hub for browsing and submitting looks is planned (Omarchy-style: each look is a git repo, and an index repo takes pull requests). See the design doc.
