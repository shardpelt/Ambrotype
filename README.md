# flightbroom

A free, open-source alternative to Adobe Lightroom® for Windows, macOS and Linux that can apply the Lightroom presets you already bought, faithfully.

**Status:** early. The scoping is done. Working so far: the preset reader, the Lightroom calibration kit, a raw renderer covering the core of Lightroom's Develop panels (white balance, camera colour, Basic tone/presence incl. Texture/Clarity/Dehaze, HSL, Color Grading, curves, sharpening, noise reduction, post-crop vignette, grain), and a command protocol so scripts and AI assistants can drive edits.

## Docs
- [docs/SCOPE.md](docs/SCOPE.md): the project scope, strategy, roadmap, risks and open decisions. Start here.
- [docs/research/](docs/research/): research on Lightroom's features, preset and profile formats, darktable, RawTherapee and the OSS ecosystem, processing algorithms, tech stack, RapidRAW, cross-platform concerns, the commercial market, and a reading list.
- [docs/adobe-docs/](docs/adobe-docs/): source-only digests of Adobe's official Lightroom and Camera Raw help.
- [docs/desktop-setup.md](docs/desktop-setup.md): build and run it on your machine.
- [docs/lightroom-trial-plan.md](docs/lightroom-trial-plan.md) and [docs/sources-to-collect.md](docs/sources-to-collect.md): what to gather next.
- [docs/design/ai-control.md](docs/design/ai-control.md) and [docs/ai/](docs/ai/): bring-your-own-AI control, the AI guide and the generated settings reference ([llms.txt](llms.txt)).

## Code (Rust workspace)

| Crate | What it does |
|---|---|
| `crates/flb-presets` | Reads Lightroom/Camera Raw `.xmp` presets, XMP sidecars, legacy `.lrtemplate` presets, and XMP embedded in DNG/JPEG "presets" into a lossless settings model. Includes a tag registry (`data/tags.json`), per-preset inspection/fidelity reports, and provisional Amount scaling. |
| `crates/flb-engine` | Processing engine v0, a staged pipeline over linear ProPhoto: raw decode (rawler), white balance, Exposure, Dehaze (dark-channel prior), Basic tone (global v1), Texture/Clarity (edge-aware local contrast), Vibrance/Saturation, 8-band HSL, Color Grading, parametric and point curves, noise-adaptive luminance and colour NR, sharpening with Detail and Masking, post-crop vignette (3 styles), resolution-independent grain, and sRGB output. All model constants are fit-ready (`--calibration`). |
| `crates/flb-protocol` | Typed, undoable edit commands and queries validated against the settings registry (`flb edit`, `flb query`, `flb protocol schema`), plus the AI-readable settings reference (`docs/ai/`). |
| `crates/flb-color` | Colour science: Lightroom-style Temperature/Tint ↔ CIE xy (Robertson's method). |
| `crates/flb-testkit` | Calibration tooling: synthetic DNG targets (ramp, hue grid, edges, ColorChecker) and the Lightroom slider-sweep kit (raw copies + XMP sidecars, or DNGs with embedded settings, + manifest). |
| `crates/flb-cli` | The `flb` command-line tool. |

```sh
cargo test
cargo run -p flb-cli -- preset inspect path/to/preset.xmp
cargo run -p flb-cli -- preset census ~/my-presets      # which settings your preset collection actually uses
cargo run -p flb-cli -- sweep plan                      # the Lightroom calibration sweeps
cargo run -p flb-cli -- testimage colorchecker cc.dng   # a synthetic linear DNG target
cargo run --release -p flb-cli -- render photo.nef out.jpg --preset look.xmp --amount 70
```

All rendering models are first versions ("ours, not Adobe's"). Their constants are to be fitted against Lightroom renders (`flb calibration defaults` prints them). `flb render` lists which preset settings it doesn't render yet. Next: a lens-correction profile path, a local (edge-aware) Highlights/Shadows, camera profiles (DCP), and the MCP server and look matching for AI control (SCOPE §8).

*Licence: not yet chosen (see SCOPE §6; GPL-3.0-or-later is recommended). Lightroom is a trademark of Adobe Inc. This project is not affiliated with Adobe.*
