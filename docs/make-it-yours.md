# Make it yours: layouts, themes and the hub

How to change how Ambrotype looks and is laid out, and how to share it. Nothing here needs code: a **layout** (what goes where) and a **theme** (colours) are each one small JSON file. The exact file format is in [ai/ui-layouts.md](ai/ui-layouts.md), for when you write one by hand or ask an AI to.

## 1. Pick one

**Appearance** in the toolbar (Ctrl+L) lists the layouts and themes. Click one to use it.
- **Built-in layouts:** for every day (Classic, Compact, Studio…), for a task (Black & white; the task bar's Color, Retouch, Film and Collage have their own, listed in Appearance only while you're in that task), and for comfort (Beginner, Big controls, Left-handed).
- **Built-in themes:** Dark and Light, High contrast, and a few in other styles.
- **Save this setup…** keeps the layout and theme you have now together under a name, to switch back in one click.

## 2. Change a layout

1. In Appearance, choose the layout closest to what you want and press **Edit layout…** (Ctrl+Shift+L). A built-in layout is copied first; yours is changed as it is.
2. On screen:
   - **drag** panels, settings and toolbar buttons to move them;
   - **click** one to change its control (slider, knob, bar or number), label or columns, or to split or merge panels;
   - add what isn't shown yet with **Add…**;
   - drag a column's edge to resize it.
   - the bar at the top also sets the name, the size of text and controls, and the theme.
3. Press **Save**. It's kept as your own layout.

The toolbar can show words instead of icons (the Beginner layout does): set `"toolbar_style": "words"` in the layout file ([ai/ui-layouts.md](ai/ui-layouts.md)).

## 3. Make a theme

Three colours are enough: the background, the text and an accent. Everything else is worked out from them.
1. Press **Open folder** in Appearance. Your own looks are in `looks/themes/` and `looks/layouts/` there.
2. Make a file such as `looks/themes/warm-dark.json`:
   ```json
   { "format": "ambrotype-theme/1", "name": "Warm dark", "description": "Brown-black with an amber accent",
     "tokens": { "bg": "#1c1814", "text": "#ece4d8", "accent": "#e0a040" } }
   ```
3. Press **Reload** in Appearance and pick it. Add more colours later only where the result isn't what you want.

Or ask an AI assistant: "make me an Ambrotype theme, warm and dark, with an amber accent", and point it at [ai/ui-layouts.md](ai/ui-layouts.md).

Check a file before sharing it: `ambrotype --check-look warm-dark.json` lists any problem.

## 4. Share it

Three ways, from simplest to widest:
- **As a file:** **Share…** in Appearance saves it anywhere. Send it to someone, who adds it with **Import…**.
- **From your own GitHub repo:** put the file at the top of a repo as `theme.json` (or `layout.json`). Others paste the repo's link (`https://github.com/you/warm-dark`) into **Browse → Install a layout or theme from a link**.
- **In the hub**, where everyone using Ambrotype can browse it: pick your look in Appearance and press **Submit to hub…**.
  1. GitHub opens with your file filled in.
  2. Press "Propose changes", then "Create pull request". GitHub makes your copy of the hub for you if needed.
  3. Once a maintainer merges it, it shows up in everyone's **Browse**.

  The hub's rules (one look per file, a clear name, CC0 unless you say otherwise, no other product's branding) are in [its README](https://github.com/Ambrotypehq/ambrotype-hub#readme).

## 5. Your own hub

The hub is only a GitHub repo of files with an `index.json`; there's no server. To run one for a club, a class or a team:
1. Fork or copy [Ambrotypehq/ambrotype-hub](https://github.com/Ambrotypehq/ambrotype-hub).
2. Add your files under `looks/`, `presets/`, `collages/`, `sizes/`, `plugins/` or `interfaces/`, then run `python3 tools/build_index.py` to update `index.json`.
3. In Ambrotype, **Settings → General → Hub address**: the raw address of its index, such as `https://raw.githubusercontent.com/your-club/ambrotype-hub/main/index.json`. A hub on your own computer works too (`file:///path/to/hub/index.json`). **Use the default hub** goes back.

Browse then lists your hub's items; installing checks each file against the checksum in the index.

## Also shared through the hub

The same Browse button installs develop presets, collage templates, export size lists and plugins. Plugins are small programs, so they're sandboxed and show what they ask for before you install them; how to write one is in [ai/plugins.md](ai/plugins.md).

## Go further: a whole interface of your own

A layout rearranges the editor's own controls. If you want something else altogether (one big slider for a friend, a touch screen, a culling screen), write a **custom interface**: a folder with an `index.html` and your own scripts and styles, which replaces the whole window and calls the same commands. Ask an AI agent to write one with [ai/custom-interface.md](ai/custom-interface.md) as its guide, then pick it in **Settings → General → Interface**. **Share…** next to it sends it to the hub; others find it under **Browse → Interfaces**. It's a program, so the app asks before installing or showing one, and the menu bar's **Interface → Back to the Built-in Interface** (or starting with `--builtin-ui`) always comes back.
