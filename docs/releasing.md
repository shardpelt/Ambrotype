# Releasing

The `Release` workflow (`.github/workflows/release.yml`) builds installers for every OS with Tauri's bundler.

## Making a release

1. Set the version in `crates/amb-gui/tauri.conf.json` and the crates' `Cargo.toml` (they share it).
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The workflow builds each platform and attaches the installers to a **draft** GitHub release. Check them, write the notes, then publish.

To try the build without a release, run the workflow by hand from the Actions tab: the installers are kept as build artifacts.

## What gets built

| Platform | Runner | Installers |
|---|---|---|
| Linux x64 | ubuntu-22.04 | `.deb`, `.rpm`, `.AppImage` |
| Windows x64 and ARM64 | windows-latest, windows-11-arm | `.msi`, `-setup.exe` (NSIS) |
| macOS Apple Silicon and Intel | macos-latest, macos-13 | `.dmg` |

Every installer also carries the `amb` command-line tool, next to the editor's own program (Tauri's `externalBin`): in `/usr/bin` with the `.deb`, in the install folder on Windows, and in `Ambrotype.app/Contents/MacOS` on macOS. That is the `amb` to point an AI app at for `amb mcp` ([ai/mcp.md](ai/mcp.md)).

The release settings are in `crates/amb-gui/tauri.release.conf.json`, merged over `tauri.conf.json` only for releases (`cargo tauri build --config tauri.release.conf.json`), so everyday builds don't need the bundled `amb`.

## Building installers yourself

```sh
cargo install tauri-cli --version "^2" --locked
cargo build --release -p amb-cli
cp target/release/amb crates/amb-gui/binaries/amb-$(rustc -vV | sed -n 's/host: //p')   # add .exe on Windows
cd crates/amb-gui && cargo tauri build --config tauri.release.conf.json
```

The installers land in `target/release/bundle/`. On Linux, the webview packages from [desktop-setup.md](desktop-setup.md) are needed, plus `patchelf` for the AppImage.

## Signing

Unsigned installers work, but macOS Gatekeeper and Windows SmartScreen warn about them.

- **macOS:** set the repository secrets `APPLE_CERTIFICATE` (the base64 `.p12` of a Developer ID Application certificate), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and for notarisation `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID`. Tauri signs and notarises when they are set.
- **Windows:** not set up yet. The plan is SignPath's free signing for open-source projects (see [GAPS.md](GAPS.md)).
- **Linux:** packages are unsigned.
