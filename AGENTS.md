# Notes for AI coding agents (Claude Code, Codex, …)

Ambrotype is a free, open-source Lightroom alternative in Rust. Start with [docs/NEXT.md](docs/NEXT.md) (where we are and what's next), then [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Layout
- `crates/amb-presets`: Lightroom XMP/preset reader and the tag registry (`data/tags.json`).
- `crates/amb-engine`: the raw pipeline.
- `crates/amb-protocol`: undoable edit commands. **Every edit from the GUI, CLI or an AI goes through these commands.**
- `crates/amb-export`: file writing.
- `crates/amb-gui`: Tauri 2 editor. Its UI is plain HTML/CSS/JS in `crates/amb-gui/ui/` (no npm, no build step).
- `crates/amb-cli`: the `amb` tool.
- `crates/amb-plugin` (the sandboxed plugin host) and `crates/amb-plugin-sdk` (what plugins are written with); example and built-in plugins in `plugins/` (rebuild a changed one with `amb plugin build plugins/<id>` and commit its `plugin.wasm`). Guide: [docs/ai/plugins.md](docs/ai/plugins.md).
- Design notes in `docs/design/`, known gaps in `docs/GAPS.md`.

## Before you commit
```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Changed a tag's range, default or description? Regenerate the AI settings reference with `cargo run -p amb-cli -- settings reference` (a test checks it is current).
- GUI changes: run the editor (`cargo run --release -p amb-gui -- photo.nef`) and try the change by hand; there are no automated UI tests yet.
- Running several agents at once: give each its own `CARGO_TARGET_DIR` (or its own worktree and target dir), or they overwrite each other's binaries.

## Conventions
- Settings and panels are generated from the registry; don't hard-code ranges or labels in the UI.
- Models that imitate Lightroom are "ours, not Adobe's": black-box measurement only, never Adobe code or data files (see `docs/legal-and-naming.md`).
- Match the surrounding code's style and comment density. Keep docs in plain, short sentences, and update `docs/NEXT.md`, `docs/GAPS.md` and `docs/ARCHITECTURE.md` when you change what exists.
- Commit messages: a `area: summary` first line, then why. Don't put AI model names in commits or code.
- Licence: the project is GPL-3.0-or-later (the plugin SDK is MIT OR Apache-2.0). New source files start with an SPDX line (`// SPDX-License-Identifier: GPL-3.0-or-later`). Code ported from another project needs a compatible licence and a credit in the file and in `NOTICE`.
- Large raw files, Lightroom renders and preset collections are never committed.
