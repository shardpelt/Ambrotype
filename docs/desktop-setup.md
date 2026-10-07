# Running Ambrotype on your desktop

## 1. Install and build (Windows, macOS or Linux)
1. Install Rust from <https://rustup.rs>. On Windows, accept the Visual Studio C++ build tools prompt. On macOS, run `xcode-select --install` first.
2. Get the code:
   ```sh
   git clone https://github.com/shardpelt/Ambrotype.git
   cd Ambrotype
   git checkout claude/lightroom-alternative-scope-xlnf0p
   ```
   Already cloned under the old name (`flightbroom`)? GitHub redirects it, but you can point it at the new name:
   ```sh
   git remote set-url origin https://github.com/shardpelt/Ambrotype.git
   ```
3. Build and test:
   ```sh
   cargo test                      # all unit and integration tests should pass
   cargo build --release           # target/release/amb (CLI) and target/release/ambrotype (editor); .exe on Windows
   ```

## 2. Try it on your own photos
```sh
amb render photo.NEF out.jpg                       # any raw rawler supports (NEF, CR2/CR3, ARW, RAF, DNG, ...)
amb render photo.NEF out.jpg --preset look.xmp --amount 70
amb preset inspect look.xmp                        # what a preset contains and what we can't do yet
amb preset census "D:/Presets"                     # which settings your whole preset collection uses
```
Rendering covers the core Develop panels (white balance, Basic tone and presence, curves, HSL, Color Grading, Detail, Lens Corrections, Effects) plus film negatives, with a placeholder base look; nothing is fitted to Lightroom yet. The command lists what it skipped. If you can, send back the `amb preset census` output for your bought presets. It's only setting names and counts, no preset content, and it tells us what to build first.

## 3. Prepare the Lightroom kit (before subscribing)
Put the kit on the first Kingston SSD and the exports on the second one (see [lightroom-trial-plan.md](lightroom-trial-plan.md)).
```sh
amb sweep plan                                                # the 74 sweeps
amb sweep generate --out E:/kit --synthetic ramp,hue,edges,colorchecker
amb sweep generate --out E:/kit-raw --groups tone,white_balance,presence  D:/raws/*.NEF
```

## 4. Smoke test with Lightroom's 7-day free trial
1. Import a few files from `E:/kit` with **Add**.
2. Check three things:
   - The synthetic DNGs open, showing the profile "Ambrotype Linear ProPhoto".
   - The settings are applied: open `colorchecker__Exposure2012_p1.dng` and Exposure should read +1.00.
   - The proprietary-raw copies pick up their `.xmp` sidecars.
3. Note the Lightroom version and the process version it shows.
4. Report anything that doesn't import before starting the paid month.

## The desktop editor (GUI)

The editor is `crates/amb-gui` (Tauri 2). It is an early version: open a raw, edit with sliders and presets, before/after, undo/redo, save the XMP sidecar, export.

```sh
cargo run --release -p amb-gui                    # opens an empty window; drop a raw on it or press Ctrl+O
cargo run --release -p amb-gui -- photo.nef       # opens a photo straight away
```

- **Windows:** needs the WebView2 runtime (already part of Windows 10/11).
- **macOS:** works out of the box.
- **Linux:** install the WebKitGTK development packages first. On Ubuntu/Debian:
  ```sh
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libxdo-dev
  ```
- **Presets:** Lightroom's own preset folder is used automatically when it exists (`%APPDATA%\Adobe\CameraRaw\Settings` on Windows, `~/Library/Application Support/Adobe/CameraRaw/Settings` on macOS), plus `~/.local/share/ambrotype/presets`. "Add folder…" adds any other folder for the session.
- **Looks:** the Looks button picks a layout (Classic, Compact, Big controls, Studio with knobs, Left-handed) and a theme (Dark, Light, High contrast, Neutral grey). Clone one to rename it or edit its colours; Share… and Import… move them between machines, and **Browse hub…** installs looks other people shared. **Edit layout** rearranges the screen in place: drag things to move them (onto the Add list to remove them), click one to change it, then Save. Your AI assistant can write new ones: [ai/ui-layouts.md](ai/ui-layouts.md).
- **Lens profiles** are found as for the CLI (lensfun database, your Adobe `.lcp` files; see docs/design/lens-corrections.md).
- **Keyboard:** every shortcut can be changed in **Settings** (the ⚙ button, or Ctrl+,). It shows them on a keyboard map and as a list, catches clashes, and keeps only your changes, so new defaults still arrive.

  ![Settings: keyboard shortcuts](images/settings-shortcuts.png)

  The defaults:

  | Keys | Action |
  |---|---|
  | Ctrl+O | Open |
  | Ctrl+S | Save sidecar |
  | Ctrl+E | Export |
  | Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y | Undo / redo |
  | `\` | Before/after |
  | Z, or click the photo | 100% view at that point; drag to pan |
  | W | White-balance picker: click something neutral grey or white |
  | J | Clipping overlay (red: clipped highlights, blue: crushed shadows) |
  | R | Crop and straighten (Enter applies, Esc cancels, X swaps the aspect) |
  | Esc | Leave the picker or crop tool |
  | Ctrl+L / Ctrl+Shift+L | Looks / Edit layout |
  | Ctrl+, | Settings and shortcuts |
  | Double-click a slider | Reset it |
  | Ctrl is Cmd on macOS | |

## macOS on Apple Silicon (M1 and later), with an external SSD

- **Format the SSD as APFS** (Disk Utility → Erase → APFS) if it stays with the Mac. exFAT works across Windows and Mac but is slower with many small files, has no permissions, and is less safe if unplugged mid-write. A build makes tens of thousands of small files.
- **Put the code and the build output on the SSD.** A debug `target/` grows to about 20 GB:
  ```sh
  cd /Volumes/<SSD>
  git clone https://github.com/shardpelt/Ambrotype.git && cd Ambrotype
  git checkout claude/lightroom-alternative-scope-xlnf0p
  cargo run --release -p amb-gui -- ~/Pictures/photo.nef
  ```
- **Spotlight:** exclude the `target/` folder (System Settings → Spotlight → Search Privacy) so it doesn't index build files.
- **Always use `--release` to judge speed.** A debug build is about 10× slower (the 1:1 view took about 33 s in debug).
- The app isn't signed or notarised yet, so run it with `cargo run` (or open the built binary from Terminal) rather than by double-clicking a copied app.

## Working with an AI agent locally

The cloud sessions on claude.ai/code run in a container that can't see your drives, Lightroom or your photos. For anything that touches them (the sweep kit, the Lightroom batches, comparing renders), run the agent on your own Mac. The repo itself is the hand-off: [AGENTS.md](../AGENTS.md) (also read as `CLAUDE.md`) and [NEXT.md](NEXT.md) tell a fresh session where things stand.

1. **Install Claude Code** (see <https://code.claude.com/docs> for the current installer), or use the Code tab of the Claude desktop app and pick the repo folder on the SSD.
2. **Start it in the repo:**
   ```sh
   cd /Volumes/<SSD>/Ambrotype
   claude
   ```
   It can now build, run the app, read your raws and Lightroom exports, and run `amb` on them. It asks before running commands unless you allow them.
3. **Steer it from your phone (optional):** run `claude remote-control` in the repo folder instead. The session runs on the Mac and shows up in the Claude app, so you can follow and answer it away from the desk.
4. **Choosing models:** use `/model` to switch, for example a stronger model in plan mode (Shift+Tab) for planning, then the execution model to build. A subagent file in `.claude/agents/` can pin its own model, for example a planner.
5. **Several agents in the background:** [herdr](https://github.com/ogulcancelik/herdr) is a terminal multiplexer for coding agents. It keeps each agent (Claude Code, Codex, …) in its own pane, shows which ones are working, blocked or done, and keeps them running when you close the terminal. For parallel work on this repo:
   - Give each agent its own git worktree (`git worktree add ../amb-<task> -b <task>`).
   - Give each agent its own `CARGO_TARGET_DIR` on the SSD.
   - Merge the branches when they're done.
6. **Keep the Mac awake for long runs.** Background agents (herdr's included) stop when the Mac sleeps. Plug in, and run `caffeinate -dis` in a terminal for as long as the batch runs. A MacBook only stays awake with the lid closed when it's on power and connected to an external display.
7. **Local models (optional):** a quantised Qwen coder model through Ollama or LM Studio (MLX builds are fastest on Apple Silicon) can handle small, cheap tasks. On an 8–16 GB M1, stay around 7–14B parameters. That's too small to drive work on this codebase on its own.

### The Lightroom trial batches with a local agent

The agent can't click through Lightroom for you, but it can do everything around it:

- **Before importing:** generate the sweep kit onto the SSD (`amb sweep generate …`, see [lightroom-trial-plan.md](lightroom-trial-plan.md)) and check that the counts and folder layout match the run sheet.
- **In Lightroom (you):** import the folder, select all, and export with the saved export preset. That's a few clicks per batch, however many images it holds.
- **After exporting:** the agent checks that every expected export exists, then renders the same files with `amb` and compares them, and fits the model constants from the differences.


