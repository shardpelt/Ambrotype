# Recipes: common changes, step by step

Each recipe lists the places a change touches, in order. A test (`crates/amb-cli/tests/recipes.rs`) checks that every file named here exists and contains the names given with it, so the list stays true.

## Done means it works as asked

- **Run the exact commands the request names**, as written, and read their output. "Tests pass" is not enough when the request says `amb do photo.nef "amb:LUTAmount=50"` should work.
- **For a test photo:** `amb testimage colorchecker photo.dng` (also `ramp`, `hue`, `edges`, `negative`, `dust`). Then:
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
- **UI tests:** scripts in `crates/amb-gui/tests/ui/` drive the editor from inside the page and check what it shows. Run one:
  ```sh
  amb testimage colorchecker /tmp/ui/test.dng
  amb testimage ramp /tmp/ui/more/other.dng      # browse.js shows a second folder too
  DISPLAY=:99 ambrotype --ui-test crates/amb-gui/tests/ui/smoke.js /tmp/ui/test.dng
  ```
  It prints each step and exits 0 when all pass. A script is the body of an async function of `t` (`ui/core/uitest.js`): `t.step(name, fn)`, `t.setSlider("Exposure2012", 1)`, `t.key("Ctrl+Z")`, `t.click(selector)`, `t.until(what, () => cond)`, `t.expect(ok, what)`, `t.row(name)`, `t.setting(name)`, `t.visible(el)`. Save dialogs don't open: exports go to a temporary folder. A `console.error` or an uncaught error fails the run. Use a fresh config folder (`XDG_CONFIG_HOME`) so your own settings don't change the result. CI runs every script in that folder on Linux, in order, with one settings folder; run them the same way before pushing a UI change (the UI is built into the binary, so build first):
  ```sh
  cargo build -p amb-gui -p amb-cli
  T=/tmp/ui-check && rm -rf $T && mkdir -p $T/more
  target/debug/amb testimage colorchecker $T/test.dng
  target/debug/amb testimage ramp $T/more/other.dng
  target/debug/amb lrcat sample $T/more/Sample.lrcat $T/more/other.dng
  export XDG_CONFIG_HOME=$T/.cfg XDG_CACHE_HOME=$T/.cache XDG_DATA_HOME=$T/.data
  for t in crates/amb-gui/tests/ui/*.js; do
    xvfb-run -a -s "-screen 0 1600x1000x24" target/debug/ambrotype --ui-test "$t" $T/test.dng || echo "FAILED: $t"
  done
  ```
  The settings folders are hidden so the app's own thumbnails aren't photos of the folder. A change to `amb-gui` alone (no other crate depends on it) may run `cargo clippy -p amb-gui --all-targets -- -D warnings`, `cargo test -p amb-gui` and `cargo test -p amb-cli --test code_size --test recipes` instead of the whole workspace's; whoever pushes runs the whole workspace's. A debug build folder takes about 13 GB: on "No space left on device", `rm -rf target/debug/incremental target/release`. A UI bug fix gets a step there.
- **Built-in layouts, themes and presets:** never edit them in `crates/amb-gui/data/hub/`. They are copies, checked against `crates/amb-gui/data/hub/index.json`. Change them in the hub repo and run `tools/sync-hub.py`, or leave that step to the maintainer and say so.

## Add a develop setting (a slider, switch or choice)

1. `crates/amb-presets/data/tags.json`: one entry in `tags`: `name`, `group`, `kind`, `min`, `max`, `default`, `status`.
   - Lightroom's settings keep their XMP name.
   - Our own start with `amb:` (like `amb:LUTAmount`).
2. `crates/amb-protocol/data/descriptions.json`: its `label` and `description`. The settings reference and the editor's tooltips show them. Optionally a `coach` (`looks`, `more`, `less`): the editor's "?" card shows it.
3. `crates/amb-engine/src/pipeline/settings/mod.rs`: a field on `DevelopSettings`, read in `from_settings` (in `settings/from_settings.rs`).
   - The stage itself: called from `develop` in `crates/amb-engine/src/pipeline/mod.rs`, at its place in the pipeline order (the module header lists it).
   - A stage that looks at neighbouring pixels takes a `Tile`, so the 1:1 view (`render_region`) matches the full render.
4. `crates/amb-engine/src/plan.rs`: the editor's GPU preview runs a `Step` list. Add a GPU step, or a `Cpu` step that runs your engine code, or the preview won't show the setting.
5. A setting that needs a learned model: add `"ai": true` to its entry in `crates/amb-presets/data/tags.json`, so AI-free mode refuses it ([design/ai-free.md](design/ai-free.md)).
5. `crates/amb-gui/src/schema.rs`: `panels` decides which panel and section show it. The control itself comes from the registry; don't hard-code ranges or labels in the UI.
6. Regenerate the generated docs (tests check them):
   - `cargo run -p amb-cli -- settings reference > docs/ai/settings-reference.md`;
   - `cargo run -p amb-cli -- ref > docs/ai/core.md`.
7. Check: `amb do photo.dng "Name=value"` (an `amb:` name works too), then `amb stats` or `amb render`. Add an engine test for the stage.

## Add a toolbar button

1. `crates/amb-gui/ui/index.html`: a `<button id="btn-…" title="…">Word</button>` among the others in `topbar`.
2. `crates/amb-gui/ui/looks/layout.js`: `TOOLBAR_IDS` maps the layout name to the button's id.
3. `crates/amb-gui/src/looks/mod.rs`: `TOOLBAR` lists the names a layout may use; check the array's length.
4. `crates/amb-gui/ui/core/app.js`: the click handler, with the others in `init`.
5. `crates/amb-gui/ui/core/keys.js`: optionally a shortcut in `KEY_ACTIONS`, with `button:` set to the id so the tooltip shows the key.
6. `crates/amb-gui/ui/core/menu.js`: with a shortcut, the action's id in `MENU_BAR` (the menu bar), and a title in `MENU_TITLES` if its label is a sentence; a test checks the ids.
7. `crates/amb-gui/ui/looks/editor.js`: its name in `TOOL_TITLES`, for editing layouts.
8. `crates/amb-gui/ui/library/library.js`: if it acts on the photo, add its id to `VIEWER_BUTTONS` (hidden in the library).
9. `crates/amb-gui/ui/core/icons.js`: an icon in `ICON_PATHS` and `TOOLBAR_ICONS`, only if the picture is obvious; otherwise it stays a word.
10. `docs/ai/ui-layouts.md`: a row in the toolbar table, and in the example `toolbar`.
   - The test `format_reference_is_complete` checks every name is documented.
11. Built-in layouts list their buttons. A new button shows in every layout that has no `toolbar` list; to add it to the built-in ones, see "Done means it works as asked" above.

## Add a CLI command

1. `crates/amb-cli/src/main.rs`: a line in `USAGE` and a case in the `match` in `main`.
   - The function goes in the file for its area in `crates/amb-cli/src/` (`render.rs`, `edit.rs`, `library.rs`, `look.rs`, …), or a new one.
   - Reuse the library code rather than writing files by hand.
   - Sidecars: `amb_protocol::sidecar` (`read_info`, `with_info`, `write_atomic`).
   - Edits: `amb_protocol::edit` (`run`), so they are undoable like the editor's.
2. `crates/amb-cli/tests/`: a test that runs the built binary (`env!("CARGO_BIN_EXE_amb")`) on a test photo, as `mcp.rs` does.

## Add an edit command to the protocol

1. `crates/amb-protocol/src/command.rs`:
   - a variant of `Command`;
   - its text in `label`, which History shows.
2. `crates/amb-protocol/src/state/mod.rs`: what it does, in `execute`. A command is one undo step.
3. `crates/amb-protocol/src/schema.rs`: its JSON Schema in `command_schema`.
4. `crates/amb-protocol/src/compact.rs`: a compact form, if it needs one (parsed in `one`).
5. `crates/amb-protocol/src/card.rs`: the core card in `core`, then regenerate `docs/ai/core.md` (step 6 of "Add a develop setting").
6. `crates/amb-cli/src/mcp.rs`: the list of commands in the MCP tool's description.
   - Also the `amb edit` line in `USAGE` in `crates/amb-cli/src/main.rs`.
7. `docs/ai/README.md`: what it does, for AI clients.
8. `crates/amb-protocol/tests/protocol.rs`: a test.
9. A command that can add an AI part is checked by `refuse_ai` in `crates/amb-protocol/src/state/mod.rs`; if it adds one by a new route, call it there ([design/ai-free.md](design/ai-free.md)).

## Add an AI model, task or feature

AI-free mode promises no model touches a photo ([design/ai-free.md](design/ai-free.md)). Every step below keeps that true; the guards fail if one is missed.

1. `crates/amb-ai/src/lib.rs`: a model task is a row in `TASKS`, and each model an entry in `crates/amb-ai/data/models.json`.
2. `crates/amb-ai/src/model.rs`: every place that loads, downloads or runs the model calls `amb_presets::ai_free::check()?` first.
   - `crates/amb-cli/tests/code_health.rs`: `every_model_path_passes_ai_free_mode` counts the load sites against the gates in each file.
3. `crates/amb-engine/src/subject.rs`: a new hook the engine offers is a `pub fn set_…`; gate it at call time. `crates/amb-ai/src/hooks.rs` installs it.
   - `crates/amb-presets/src/ai_free.rs`: add it to `GATED_HOOKS`; the same test compares the lists.
4. `crates/amb-presets/data/tags.json`: a setting that needs the model gets `"ai": true`; a mask kind goes in `ai_mask_kinds` (or `ai_mask_subtypes`), a spot kind in `ai_spot_kinds`. The protocol then refuses it while the mode is on, and AI-free mode names it as left out.
5. `crates/amb-gui/ui/index.html`: a button or box that ends in the model gets a `data-ai` attribute (built in a script: `"data-ai": aiFreeUses(kind)`, or `data-ai` on the element), so the editor turns it off.
6. `crates/amb-ops/tests/ai_free.rs`: a case where the counting hooks see zero calls with the mode on and some with it off.
7. `docs/design/ai-free.md`: the list of what counts as AI, if the feature is new.

## Add a kind of hub item

1. `crates/amb-gui/src/hub/mod.rs`:
   - `known_kind` accepts it;
   - `fetch_item` reads and checks it for the Browse dialog;
   - `browse` marks what's installed.
   - In `hub/kinds.rs`, an `install_…` function checks its sha256 and saves it.
2. `crates/amb-gui/src/commands/mod.rs`: register the new Tauri command in `handler`.
   - The command itself goes in `crates/amb-gui/src/commands/hub.rs` and calls the install.
3. `crates/amb-gui/ui/dialogs/hub.js`: in `hubFlow`, a preview and the Install button's branch. `crates/amb-gui/ui/index.html`: a tab in `hub-tabs`.
4. Docs: `docs/design/customization-and-hub.md`. In the hub repo (Ambrotypehq/ambrotype-hub), tools/build_index.py lists the folders it indexes.
