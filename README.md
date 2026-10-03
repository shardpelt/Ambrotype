# flightbroom

A free, open-source alternative to Adobe Lightroom® for Windows, macOS and Linux that can apply the Lightroom presets you already bought, faithfully.

**Status:** early. The scoping is done; the first code is the preset reader.

## Docs
- [docs/SCOPE.md](docs/SCOPE.md): the project scope, strategy, roadmap, risks and open decisions. Start here.
- [docs/research/](docs/research/): research on Lightroom's features, preset and profile formats, darktable, RawTherapee and the OSS ecosystem, processing algorithms, tech stack, RapidRAW, cross-platform concerns, the commercial market, and a reading list.
- [docs/adobe-docs/](docs/adobe-docs/): source-only digests of Adobe's official Lightroom and Camera Raw help.
- [docs/lightroom-trial-plan.md](docs/lightroom-trial-plan.md) and [docs/sources-to-collect.md](docs/sources-to-collect.md): what to gather next.

## Code (Rust workspace)

| Crate | What it does |
|---|---|
| `crates/flb-presets` | Reads Lightroom/Camera Raw `.xmp` presets, XMP sidecars, legacy `.lrtemplate` presets, and XMP embedded in DNG/JPEG "presets" into a lossless settings model. Includes a tag registry (`data/tags.json`), per-preset inspection/fidelity reports, and provisional Amount scaling. |
| `crates/flb-testkit` | Calibration tooling: generates the Lightroom slider-sweep kit (raw copies + XMP sidecars + manifest). |
| `crates/flb-cli` | The `flb` command-line tool. |

```sh
cargo test
cargo run -p flb-cli -- preset inspect path/to/preset.xmp
cargo run -p flb-cli -- preset census ~/my-presets      # which settings your preset collection actually uses
cargo run -p flb-cli -- sweep plan                      # the Lightroom calibration sweeps
```

Nothing renders images yet; the processing engine is the next milestone (see SCOPE §8, Phase 1).

*Licence: not yet chosen (see SCOPE §6; GPL-3.0-or-later is recommended). Lightroom is a trademark of Adobe Inc. This project is not affiliated with Adobe.*
