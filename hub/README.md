# Ambrotype hub

Shared layouts, themes and plugins for [Ambrotype](https://github.com/shardpelt/Ambrotype). Layouts and colour themes are called "looks" here (they live in `looks/`). Ambrotype's **Appearance → Browse hub…** lists the looks and plugins here and installs them with one click.

Like [Omarchy](https://learn.omacom.io/2/the-omarchy-manual/90/extra-themes)'s themes, there is no server: the hub is this repo. Each look is one small JSON file, and `index.json` lists them with a checksum. Looks contain no code, so they're safe to install. The app checks each file's checksum and validates it before saving it.

## Add your look

**From the app (easiest):** in **Appearance**, pick your own layout or theme and press **Submit to hub…**. GitHub opens with the file already filled in. Press "Propose changes" and then "Create pull request". If you don't have write access, GitHub makes the fork for you.

**By hand:**
1. Fork this repo.
2. Add your file as `looks/layouts/<name>.json` or `looks/themes/<name>.json`. Use lower case and dashes, e.g. `warm-dark.json`. The format is described in [ui-layouts.md](https://github.com/shardpelt/Ambrotype/blob/main/docs/ai/ui-layouts.md).
3. Check it with `ambrotype --check-look looks/themes/warm-dark.json`.
4. Run `python3 tools/build_index.py` to update `index.json`.
5. Open a pull request.

If you skip step 4, a maintainer can run it before merging.

**Rules:**
- One look per file, with a clear `name` and a one-line `description`. Add `"author"` to be credited.
- Looks are shared under CC0 (public domain) unless the file says otherwise in `"license"`.
- Nothing that copies another product's branding or name.

## Presets

Develop presets go in `presets/<name>.xmp` (lower case and dashes), in Lightroom's XMP preset format: export one from Lightroom, or use Ambrotype's **Presets → Save…** and take the file from the presets folder. The app lists them under **Browse hub… → Presets**, shows what each changes, and installs it into the presets list (group "Hub"). They also make up the **Preset library** (Presets → Preset library…), grouped by their `crs:Group` (give each a `Name`, a `Group` and a one-line `Description`). The ones here when the app is built ship inside it, so the library works before this repo is published or without a connection.

- Share only presets you made. Bought or downloaded packs belong to their makers.
- Add `crs:Author`, `crs:Description` and a `crs:License` if you like; presets are CC0 otherwise.
- Run `python3 tools/build_index.py` to list it in `index.json`.

## Collage templates and size profiles

- **Collage templates** go in `collages/<name>.json`: the file the collage maker's **Share…** saves (`"format": "ambrotype-collage/1"`). Installed ones join the templates in the collage maker.
- **Size profiles** go in `sizes/<name>.json`: a named set of sizes, installed as size presets for the crop tool and the export dialog:

  ```json
  { "format": "ambrotype-sizes/1", "name": "Etsy", "description": "Listing photos and the shop banner.",
    "sizes": [ { "name": "Listing", "width": 2000, "height": 1600 }, { "name": "Banner", "width": 3360, "height": 840 } ] }
  ```

## Plugins

Plugins are small programs that make edits (Ambrotype's **Plugins** button). They run sandboxed: no files, no network, only settings the user can see and undo. How to write one: [plugins.md](https://github.com/shardpelt/Ambrotype/blob/main/docs/ai/plugins.md).

- **Send one in:** in the Plugins dialog, pick your plugin and press **Share as pull request…**. It shows the files first, then sends them with the GitHub CLI (your own login) or opens GitHub's upload page in your browser.
- **Where they go:** `plugins/<id>/`, with `plugin.json`, the built `plugin.wasm`, and the Rust source (`Cargo.toml`, `src/`). No source, no merge: reviewers read the code and rebuild the module. Say the plugin's licence in its `Cargo.toml` (`license = "..."`), any open-source licence works.
- **Reviewing:** read the source and the permissions it asks for, then `amb plugin build plugins/<id>` (the module should come out the same, or replace it with yours) and `amb plugin check plugins/<id>`.
- **Installing:** **Appearance → Browse hub… → Plugins** lists them, with what each asks to see, and installs one with a click. The app checks the manifest and the module against `index.json` (run `python3 tools/build_index.py` after merging a plugin), and the plugin asks for its permissions the first time it runs.

## For maintainers

- CI (`.github/workflows/check.yml`) fails when a file isn't valid JSON, is in the wrong folder, or `index.json` is out of date, and when a plugin folder lacks its manifest, module or source.
- Before merging, run `ambrotype --check-look` on new files, and try them in the app.
- The app reads `index.json` from `https://raw.githubusercontent.com/<owner>/<repo>/main/index.json`. Ambrotype's default points at `Ambrotypehq/ambrotype-hub`. Anyone can point their app at another hub in **Settings → General**.
