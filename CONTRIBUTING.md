# Contributing

Thanks for helping. Ambrotype is young, so small, focused changes are the easiest to get in.

## Before you start

- **Read** [AGENTS.md](AGENTS.md) (layout, checks, conventions) and [docs/NEXT.md](docs/NEXT.md) (what's being worked on, and a Help wanted list). [docs/recipes.md](docs/recipes.md) lists what common changes touch.
- **Open an issue first** for anything bigger than a bug fix or a small improvement, so we can agree on the approach before you spend time on it.

## Making a change

1. Fork the repository and make a branch from `main`.
2. Make the change. Match the surrounding code's style, and keep docs in plain, short sentences.
3. Run the checks (CI runs the same):
   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
4. For editor (GUI) changes, run the editor and try the change by hand (`cargo run --release -p amb-gui -- photo.nef`); there are no automated UI tests yet. A screenshot in the pull request helps.
5. Update the docs the change affects: `docs/NEXT.md`, `docs/GAPS.md`, `docs/ARCHITECTURE.md` and the feature's note in `docs/design/`.
6. Commit with a first line like `area: summary` (for example `engine: faster dehaze`), then say why. Open a pull request against `main`; the template asks a few questions.

## AI-assisted contributions

Using an AI coding assistant is fine, and the repo is set up for it (`AGENTS.md`). You're responsible for what you send: read the diff, run the checks, and try the change. Don't put AI model names in commits or code.

## Licence of contributions

- Ambrotype is GPL-3.0-or-later (the plugin SDK MIT OR Apache-2.0). By opening a pull request you agree that your change is licensed under the same terms, and that you have the right to send it.
- You keep the copyright in your own work; "The Ambrotype contributors" includes you, and the Git history records what you wrote.
- Code from other projects is welcome only under a GPL-compatible licence, with its credit kept in the file and added to [NOTICE](NOTICE).
- **Never** Adobe code, profiles, presets, lens files or other data files, and never Lightroom renders: our models are built by black-box measurement only ([docs/legal.md](docs/legal.md)).
- Large raw files and preset collections don't go in the repository.

## Looks, presets and plugins

Layouts, themes, presets and plugins go to the [hub repo](https://github.com/Ambrotypehq/ambrotype-hub), not here ([docs/make-it-yours.md](docs/make-it-yours.md) shows how) (the app's built-in ones too; `tools/sync-hub.py` copies them into `crates/amb-gui/data/hub/`). See [docs/ai/plugins.md](docs/ai/plugins.md) for plugins.

## Conduct and security

Be kind; see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Security problems go privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.
