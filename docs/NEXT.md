# Where we left off (2026-10-05)

A short "start here" for the next work session. Details are in [ARCHITECTURE.md](ARCHITECTURE.md) (what exists) and [GAPS.md](GAPS.md) (everything missing).

## State

- **Branch:** `claude/lightroom-alternative-scope-xlnf0p`. Everything is committed and pushed; 266 tests are green.
- **Name:** **Ambrotype** (renamed from Flightbroom). The GitHub repo is `shardpelt/Ambrotype` (renamed; old links redirect).
- **Built:**
  - preset/sidecar reader and writer (`crs:` and our `amb:` tags)
  - edit protocol with undo
  - raw engine: core Develop panels, Lens Corrections, film negatives
  - export: JPEG/PNG/TIFF/WebP/AVIF/EXR, colour spaces with ICC
  - CLI `amb`
  - desktop editor v1 (`amb-gui`, Tauri 2)

## Owner to-dos

- [ ] Rename the GitHub repo; search trademarks for "Ambrotype" (TMview, WIPO, classes 9 and 42); register ambrotype.org (it is also our XMP namespace URI). See [legal-and-naming.md](legal-and-naming.md).
- [ ] Try the editor on the desktop: `cargo run --release -p amb-gui -- photo.nef`. Note what feels wrong.
- [ ] Run `amb preset census <your preset folders>` and share the output (setting names and counts only). It ranks what to build next.
- [ ] Lightroom trial month: follow [lightroom-trial-plan.md](lightroom-trial-plan.md) (sweep kit: `amb sweep generate`).

## Suggested next steps (in order)

1. **Editor polish:** zoom/1:1 and pan, point-curve editor, colour wheels, crop/rotate tool, WB eyedropper, clipping overlays, persisted preset folders. Release-build preview timing on the desktop.
2. **Crop, rotate, Transform/Upright** in the engine (numeric `Perspective*` first). This unlocks correct post-crop vignette and Constrain Crop.
3. **Local Highlights/Shadows** (edge-aware), then **masks** (linear/radial/brush/range).
4. **Camera profiles** (read the user's DCPs in place, fitted base looks) once trial data exists.
5. **Calibration panel, B&W mixer, Point Color.**
6. **Feedback button** (pre-filled GitHub issue, opt-in usage report): small and high value, so do it early, ideally with the first builds testers get.
7. **Token-efficient AI layer:** core card, `amb do` compact syntax, `--brief` output, `amb stats`, then the MCP server.
8. **Auto and Randomize buttons, Tutorial mode** ([design/future-ideas.md](design/future-ideas.md)).
9. **GPU preview path,** and installers per OS (`cargo tauri build`, in CI).

## Working notes

- This repo was built with Claude Code. Large text sweeps went to smaller models, and parallel work to worktree agents that were then merged.
- Workflows (multi-agent orchestration) need explicit opt-in per session.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. On Linux the GUI needs the WebKitGTK dev packages (see desktop-setup.md).
