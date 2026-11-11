# Tabs, modes, views and tools

What goes where in the editor, so it stays easy to find your way: what earns a tab at the top, and what is a mode, a view, a tool or a dialog instead. Read this before adding a tab (or a task plugin, which adds one).

## The kinds

**A tab** (a task in the code: `ui/core/tasks.js`) is a job you settle into for a while, with its own arrangement of the screen.
- It passes all four:
  1. It is a whole way of working, not one action ("edit a photo", "scan a roll"), and you stay in it for minutes.
  2. It needs different controls on screen than every other tab, so a layout of its own (panels, toolbar, its tools block).
  3. Switching to it and back keeps your place (the photo, the panels).
  4. Nothing is only there: every command in it is also in Search (Ctrl+K) and has a key or a button elsewhere.
- Tabs have Ctrl+Alt+1… in order. Few of them: at most seven built in, task plugins after.

**A mode** is a temporary state you go into for one thing and leave with Esc, landing back where you were.
- One key and one button to enter, Esc (or the same key) to leave, nothing to save.
- It may cover the whole screen or take over the photo's clicks.
- Today: the viewer (V), Slideshow, the crop tool (R), Masks (M), Spot removal (Q), Select object (O), Before/After (\\), the loupe (I), editing the layout.

**A view** is another way of showing the same thing, inside a tab.
- Switching views never changes what is selected or open.
- Today: the library's grid, loupe, and compare (G, E, C), Timeline, Map, Insights (Shift+I); All photos.

**A tool** is one action, or a panel within a tab's layout: Auto edit, Remove object, Find dust, Match look, a preset.

**A dialog** is a one-off job with choices and a confirm: Export, Import, Convert raws, Merge to HDR, Contact sheet.

When unsure, prefer the smaller kind: a tool before a dialog, a mode before a tab. A layout (Appearance) is not a tab either: it rearranges a tab for a person's taste, a tab is a different job.

## How a request is decided

"I want to look through photos full screen, like an image viewer" (2026-11-08): you go in to see, then come back, and it needs no controls of its own. **A mode**: the viewer, V or ⛶ View in the library bar and the filmstrip's bar.

## The tabs

Decided 2026-11-09: **Library · Develop · Film · Collage**, then task plugins (Trippy).

| Tab | Passes? | Why |
|---|---|---|
| Library | Yes | Finding, sorting, rating, tagging many photos; its own views. |
| Develop | Yes | Editing one photo; the default. |
| Film | Yes | Scanning negatives is its own way of working: pick the base, density, sync a roll. |
| Collage | Yes | The collage maker itself: a canvas and tray of its own. It opens over the tab you were in and closes back to it. |
| Trippy | Yes (plugin) | Playful looks; a task plugin, so it can be removed by uninstalling it. |

What went (2026-11-09):
- **Color** was Develop with colour grading up front, the same job with the same controls rearranged. Now Develop's right column has a **Colour** tab (grading wheels and mixer open), and the Colour grading layout stays in Appearance.
- **Retouch** was Develop with its tools (Remove object, Heal, Find dust, the AI masks) first, and those are modes and tools Develop reaches too. Now its **Retouch tools** sit under Quick fixes in Develop (tool blocks fold by their heading), and the Retouch layout stays in Appearance.
- **Collage prep** (the old Collage tab, a layout for getting photos to match) was confused with the collage maker. The Collage tab is now the maker; the Collage prep layout stays in Appearance.
- Someone last in one of those comes back to Develop, with its own layout, once.

Ideas that would pass as tabs later: Print (layouts on paper), Book.

## Practices

- A new tab needs a row in the table above, saying why it can't be a mode, a view or a layout.
- A mode always says how to leave it (Esc), in its bar or hint.
- Every tab, mode and view is in Search, in FEATURES.md, and has a key.
- Settings that change what you see (like "JPEG if there is one") hold in every tab and mode that shows those photos.
