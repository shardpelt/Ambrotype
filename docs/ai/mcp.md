# Ambrotype over MCP

`amb mcp` is a Model Context Protocol server. Your own AI app (Claude Desktop, Claude Code, or any MCP client) starts it and uses its tools to edit, preview and export your photos with Ambrotype's commands, billed to your own AI plan. Ambrotype never sees your AI credentials, and the AI only changes settings in the photo's XMP sidecar: every change is a normal, undoable edit that also opens in Lightroom.

## Setting it up

Build or install `amb`, then point your AI app at it.

**Claude Code:**

```sh
claude mcp add ambrotype -- /path/to/amb mcp
```

**Claude Desktop** (Settings → Developer → Edit Config, `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "ambrotype": { "command": "/path/to/amb", "args": ["mcp"] }
  }
}
```

Other clients: run `amb mcp` as a stdio server. Lens profiles (`AMB_LENSFUN_DB`, `AMB_LCP_DIRS`) and camera profiles (`AMB_DCP_DIRS`) are found as for `amb render`; set those variables in the client's server config if yours are elsewhere.

## Tools

| Tool | What it does |
|---|---|
| `describe_photo` | Camera, lens, exposure, capture time, rating, flag, label, keywords, the current edit (settings, masks, spots), its snapshots and settings that don't render yet. |
| `edit_photo` | Runs edit commands (`set_param`, `adjust_param`, `apply_preset`, `add_mask`, `add_spot`, `save_snapshot`, `undo`, …) on the photo's sidecar as one step. `dry_run` reports the changes without writing. |
| `query_photo` | Read-only queries: `get_param`, `list_params`, `describe`, `list_groups`, `list_masks`, `list_spots`, `list_snapshots`. |
| `render_preview` | Renders the photo with its edit (or with a preset instead, at an Amount) and returns a JPEG, 256–2048 pixels, with the as-shot white balance and anything not rendered. |
| `export_photo` | Writes the edited photo to a file (format from the extension), with size, quality and colour space. Never overwrites unless told to. |
| `list_photos` | The photos in a folder, with virtual copies, ratings, flags, labels, keywords and whether they're edited. |
| `set_photo_info` | Rating, colour label, pick/reject flag and keywords, written where Lightroom reads them. |
| `settings_reference` | The setting groups, or one group's settings with ranges, defaults and meanings. |

Resources: `ambrotype://ai-guide` (the [AI guide](README.md): the editing loop, commands, masks, spots and intent-to-settings recipes) and `ambrotype://settings-reference`.

The commands and queries are the same as `amb edit` and `amb query`; `amb protocol schema` prints their JSON Schema.

## How it behaves

- **Preview, then apply.** The server tells the AI to run `edit_photo` with `dry_run` first and show you the diff, and to look at `render_preview` after visual changes. Your AI app also asks before each tool call unless you allow it.
- **Nothing is overwritten** by `export_photo` unless you agree (`overwrite`).
- **A log of every tool call** goes to stderr, which MCP clients show as the server's log.
- **Renders run as a separate `amb render`,** so a preview of a 24 MP raw takes a few seconds in a release build.
- **The editor doesn't watch sidecars.** If a photo is open in the editor while an AI edits it over MCP, reopen it to see the AI's changes (and don't edit it in both at once).

## Not yet

- Look matching (`amb match`): "make this look like these samples".
- Masks drawn by the AI from what's in the photo (subject, sky); masks are placed by coordinates today.
- An assistant panel inside the editor.
