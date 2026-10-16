# Recipes: common changes, step by step

Each recipe lists the places a change touches, in order. A test (`crates/amb-cli/tests/recipes.rs`) checks that every file named here exists and contains the names given with it, so the list stays true.

## Done means it works as asked

- **Run the exact commands the request names**, as written, and read their output. "Tests pass" is not enough when the request says `amb do photo.nef "amb:Fade=40"` should work.
- **For a test photo:** `amb testimage colorchecker photo.dng` (also `ramp`, `hue`, `edges`, `negative`). Then:
  - `amb do photo.dng "Name=value"` to edit it;
  - `amb stats photo.dng --json` to see the result as numbers;
  - `amb render photo.dng out.jpg` to export it.
- **Before committing:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`, as AGENTS.md says. Cloud sessions can build the editor (`.claude/hooks/session-start.sh` installs its libraries).
- **Editor changes, without a screen:**
  ```sh
  Xvfb :99 &
  DISPLAY=:99 cargo run --release -p amb-gui -- photo.dng &
  DISPLAY=:99 import -window root shot.png    # then look at shot.png
  ```
  xdotool clicks and types (`DISPLAY=:99 xdotool mousemove 100 20 click 1`).
- **Built-in layouts, themes and presets:** never edit them in `crates/amb-gui/data/hub/`. They are copies, checked against `crates/amb-gui/data/hub/index.json`. Change them in the hub repo and run `tools/sync-hub.py`, or leave that step to the maintainer and say so.

## Add a develop setting (a slider, switch or choice)

1. `crates/amb-presets/data/tags.json`: one entry in `tags`: `name`, `group`, `kind`, `min`, `max`, `default`, `status`.
   - Lightroom's settings keep their XMP name.
   - Our own start with `amb:` (like `amb:LUTAmount`).
2. `crates/amb-protocol/data/descriptions.json`: its `label` and `description`. The settings reference and the editor's tooltips show them.
3. `crates/amb-engine/src/pipeline.rs`:
   - a field on `DevelopSettings`, read in `from_settings`;
   - the stage called from `develop`, at its place in the pipeline order (the module header lists it).
   - A stage that looks at neighbouring pixels takes a `Tile`, so the 1:1 view (`render_region`) matches the full render.
4. `crates/amb-engine/src/plan.rs`: the editor's GPU preview runs a `Step` list. Add a GPU step, or a `Cpu` step that runs your engine code, or the preview won't show the setting.
5. `crates/amb-gui/src/schema.rs`: `panels` decides which panel and section show it. The control itself comes from the registry; don't hard-code ranges or labels in the UI.
6. Regenerate the generated docs (tests check them):
   - `cargo run -p amb-cli -- settings reference > docs/ai/settings-reference.md`;
   - `cargo run -p amb-cli -- ref > docs/ai/core.md`.
7. Check: `amb do photo.dng "Name=value"` (an `amb:` name works too), then `amb stats` or `amb render`. Add an engine test for the stage.

## Add a toolbar button

1. `crates/amb-gui/ui/index.html`: a `<button id="btn-…" title="…">Word</button>` among the others in `topbar`.
2. `crates/amb-gui/ui/layout.js`: `TOOLBAR_IDS` maps the layout name to the button's id.
3. `crates/amb-gui/src/looks.rs`: `TOOLBAR` lists the names a layout may use; check the array's length.
4. `crates/amb-gui/ui/app.js`: the click handler, with the others in `init`.
5. `crates/amb-gui/ui/keys.js`: optionally a shortcut in `KEY_ACTIONS`, with `button:` set to the id so the tooltip shows the key.
6. `crates/amb-gui/ui/editor.js`: its name in `TOOL_TITLES`, for editing layouts.
7. `crates/amb-gui/ui/library.js`: if it acts on the photo, add its id to `VIEWER_BUTTONS` (hidden in the library).
8. `crates/amb-gui/ui/icons.js`: an icon in `ICON_PATHS` and `TOOLBAR_ICONS`, only if the picture is obvious; otherwise it stays a word.
9. `docs/ai/ui-layouts.md`: a row in the toolbar table, and in the example `toolbar`.
   - The test `format_reference_is_complete` checks every name is documented.
10. Built-in layouts list their buttons. A new button shows in every layout that has no `toolbar` list; to add it to the built-in ones, see "Done means it works as asked" above.

## Add a CLI command

1. `crates/amb-cli/src/main.rs`: a line in `USAGE`, a case in the `match` in `main`, and a function.
   - Reuse the library code rather than writing files by hand.
   - Sidecars: `amb_protocol::sidecar` (`read_info`, `with_info`, `write_atomic`).
   - Edits: `amb_protocol::edit` (`run`), so they are undoable like the editor's.
2. `crates/amb-cli/tests/`: a test that runs the built binary (`env!("CARGO_BIN_EXE_amb")`) on a test photo, as `mcp.rs` does.

## Add an edit command to the protocol

1. `crates/amb-protocol/src/command.rs`:
   - a variant of `Command`;
   - its text in `label`, which History shows.
2. `crates/amb-protocol/src/state.rs`: what it does, in `execute`. A command is one undo step.
3. `crates/amb-protocol/src/schema.rs`: its JSON Schema in `command_schema`.
4. `crates/amb-protocol/src/compact.rs`: a compact form, if it needs one (parsed in `one`).
5. `crates/amb-protocol/src/card.rs`: the core card in `core`, then regenerate `docs/ai/core.md` (step 6 of "Add a develop setting").
6. `crates/amb-cli/src/mcp.rs`: the list of commands in the MCP tool's description.
   - Also the `amb edit` line in `USAGE` in `crates/amb-cli/src/main.rs`.
7. `docs/ai/README.md`: what it does, for AI clients.
8. `crates/amb-protocol/tests/protocol.rs`: a test.

## Add a kind of hub item

1. `crates/amb-gui/src/hub.rs`:
   - `known_kind` accepts it;
   - `fetch_item` reads and checks it for the Browse dialog;
   - an `install_…` function checks its sha256 and saves it;
   - `browse` marks what's installed.
2. `crates/amb-gui/src/main.rs`: a Tauri command that calls the install, registered in `invoke_handler`.
3. `crates/amb-gui/ui/hub.js`: in `hubFlow`, a preview and the Install button's branch. `crates/amb-gui/ui/index.html`: a tab in `hub-tabs`.
4. Docs: `docs/design/customization-and-hub.md`. In the hub repo (Ambrotypehq/ambrotype-hub), tools/build_index.py lists the folders it indexes.
