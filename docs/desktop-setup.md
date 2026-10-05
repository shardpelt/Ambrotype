# Running Ambrotype on your desktop

## 1. Install and build (Windows, macOS or Linux)
1. Install Rust from <https://rustup.rs>. On Windows, accept the Visual Studio C++ build tools prompt.
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
- **Lens profiles** are found as for the CLI (lensfun database, your Adobe `.lcp` files; see docs/design/lens-corrections.md).
- **Keyboard:**

  | Keys | Action |
  |---|---|
  | Ctrl+O | Open |
  | Ctrl+S | Save sidecar |
  | Ctrl+E | Export |
  | Ctrl+Z / Ctrl+Shift+Z | Undo / redo |
  | `\` | Before/after |
  | Double-click a slider | Reset it |

