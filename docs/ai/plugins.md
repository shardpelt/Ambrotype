# Plugins: how to write one

A plugin is a small program that makes edits: "auto tone", "a random film look", "warm the skin tones". The user runs it from the **Plugins** button (Ctrl+Shift+P) or with `amb plugin run`. This guide is for people and for AI assistants asked to "make me a plugin that…". Design background: [design/customization-and-hub.md](../design/customization-and-hub.md) §5–6.

## What a plugin can and can't do

- **It gets:** the photo's current develop settings, its own parameters and a random seed. With permission, it also gets camera facts (`photo-info`) and the histogram and average colour of the edited photo (`photo-stats`).
- **It returns:** edits, as the same commands a slider makes (`set_param`, `adjust_param`, `reset_param`, `reset_group`), and a short message. The app applies them as **one undo step**, shown in History as "plugin <name>".
- **It can't:** read or write files, use the network, read the clock, see other photos, or paint pixels. There is no permission for those; the sandbox doesn't have them.
- **Limits:** about a billion instructions per run (a second or two), 64 MB of memory, 1000 commands and 1 MB of output.

Plugins are Rust compiled to WebAssembly (`wasm32-unknown-unknown`) and run in the `wasmi` interpreter (`crates/amb-plugin`). Other languages work too if they follow the [ABI](#abi-for-other-languages).

## Quick start

You need Rust ([rustup.rs](https://rustup.rs)) and the WebAssembly target:

```sh
rustup target add wasm32-unknown-unknown
amb plugin new my-plugins/warm-it-up      # from the repo: cargo run -p amb-cli -- plugin new ...
# edit warm-it-up/src/lib.rs and warm-it-up/plugin.json
cd my-plugins/warm-it-up && cargo test    # the plugin's own tests run natively
amb plugin build my-plugins/warm-it-up    # builds plugin.wasm, then checks it on a sample photo
amb plugin run my-plugins/warm-it-up photo.nef --dry-run --params amount=70
```

To use it in the editor, either build it inside the app's plugins folder (**Plugins → Open plugins folder**: `<settings folder>/plugins/<id>/`) and press **Reload**, or use **Install from folder…**.

| OS | Plugins folder |
|---|---|
| macOS | `~/Library/Application Support/org.ambrotype.editor/plugins/` |
| Windows | `%APPDATA%\org.ambrotype.editor\plugins\` |
| Linux | `~/.config/org.ambrotype.editor/plugins/` |

`amb plugin new` makes a self-contained project: `plugin.json`, `Cargo.toml`, `src/lib.rs` and a copy of the SDK in `sdk/`. A build leaves a `target/` folder; it is never copied or shared.

## The code

```rust
use amb_plugin_sdk::{Input, Output};

pub fn run(input: Input) -> Result<Output, String> {
    let amount = input.number("amount");                  // a parameter from plugin.json
    let vibrance = input.setting("Vibrance").unwrap_or(0.0); // the photo's current value
    Ok(Output::new()
        .set("Vibrance", (vibrance + amount / 2.0).min(100.0))
        .adjust("Clarity2012", amount / 5.0)
        .message(format!("Vibrance +{}", amount / 2.0)))
}

amb_plugin_sdk::export_plugin!(run);
```

- **`Input`:**
  - `number(name)`, `flag(name)`, `choice(name)`: the plugin's parameters (defaults filled in, numbers clamped to their range).
  - `setting(name)`: a numeric develop setting, or `None` when it is at its default. `settings` holds them all.
  - `photo`: `Some` with `photo-info`: camera, lens, width, height, iso, shutter (s), aperture, focal_length (mm).
  - `stats`: `Some` with `photo-stats`. It has `red`, `green`, `blue` and `luma` histograms (256 bins of sRGB 8-bit), `mean` (0–1), `percentile(f)` (the luma level below which a fraction `f` of pixels lie), and `share_above(level)`. They describe a ~512 px render with the current edits.
  - `rng()`: a random generator seeded per run; `seed` is the raw number. The same seed gives the same result.
- **`Output`:**
  - `set(name, value)`: an absolute value. Numbers are clamped to the setting's range.
  - `adjust(name, delta)`: change a number from its current value. Don't use it on settings without a default, such as `Temperature`; use `set`.
  - `reset(name)`, `reset_group(group)`: back to the default.
  - `message(text)`: shown to the user after the run. Say what changed, briefly.
- **Errors:** return `Err("why")` and the user sees it (for example "needs the photo-stats permission").
- **Log:** `amb_plugin_sdk::log("text")` adds a line to the run's log (shown in the dialog; `amb plugin run` prints it).
- **Setting names:** use only names from [settings-reference.md](settings-reference.md); an unknown name fails the whole run, and nothing is applied.

## plugin.json

```json
{
  "format": "ambrotype-plugin/1",
  "id": "warm-it-up",
  "name": "Warm it up",
  "version": "1.0.0",
  "author": "you",
  "description": "Warmer skin and a touch more vibrance.",
  "permissions": [],
  "params": [
    { "name": "amount", "label": "Amount", "type": "number", "min": 0, "max": 100, "default": 50, "step": 1 },
    { "name": "style", "label": "Style", "type": "choice", "options": ["Soft", "Strong"], "default": "Soft" },
    { "name": "skin_only", "label": "Mostly skin tones", "type": "bool", "default": true }
  ],
  "source": "https://github.com/you/warm-it-up"
}
```

- **`id`:** lower case, digits and dashes; it is also the folder name. It can't be the id of a built-in plugin (`auto-tone`, `random-look`).
- **`permissions`:** `photo-info` and/or `photo-stats`, or none. The user is asked once, and asked again whenever the module or its permissions change. Ask only for what the plugin uses.
- **`params`:** up to 32. The dialog draws a slider, a drop-down or a checkbox for each; `amb plugin run --params a=1,b=Strong` sets them on the command line. `label` is shown; an optional `description` becomes a tooltip.
- **`wasm`:** the module file, default `plugin.wasm`. `source`: where the code lives, optional.
- **`kind`:** only `"action"` (a plugin you run that makes edits) for now.

## Rules for AI assistants

- **Where to write:** only in the plugin's own folder (the plugins folder above, or a path the user names). Don't change Ambrotype's source unless the user is changing Ambrotype itself.
- **Permissions and settings:** ask for the fewest permissions, and use only setting names from the settings reference.
- **Before handing it over:** write native tests (`Input::for_test(json!({...}))`) and run `cargo test`, then `amb plugin build`. Fix everything it reports. Tell the user what the plugin does, its parameters and its permissions.
- **Results:** make it idempotent where that makes sense. Running a tone plugin twice should not keep pushing the photo, because its statistics come from the photo as already edited.
- **What not to try:** don't work around the sandbox (no file, network or clock code). It wouldn't link: the only import a plugin may have is `amb.log`.

## Sharing

**Share as pull request…** in the Plugins dialog offers a plugin to the hub (`Ambrotypehq/ambrotype-hub`, folder `plugins/<id>/`). It first lists exactly which files would be sent. Then:
- **With the GitHub CLI** installed and logged in (`gh auth login`): it forks the hub, adds the files in one commit, and opens the pull request with your own login.
- **Without it:** it opens GitHub's upload page for that folder, with your plugin's folder shown next to it. Drag the files in and choose "Propose changes".

Include the source: reviewers read it and rebuild `plugin.wasm` before merging. A change to Ambrotype itself (not a plugin) goes as a normal pull request to the Ambrotype repo, following [AGENTS.md](../../AGENTS.md).

## ABI (for other languages)

A plugin module is a `wasm32` module that:
- exports `memory`
- exports `amb_alloc(len: i32) -> i32`: memory for the input
- exports `amb_run(ptr: i32, len: i32) -> i64`: reads `len` bytes of input JSON at `ptr`, and returns the output JSON's address in the high 32 bits and its length in the low 32 bits
- may import `amb.log(ptr: i32, len: i32)`, and nothing else

The input JSON has `api` (1), `params`, `settings`, `seed`, and with permission `photo` and `stats`. The output JSON is `{"commands": [...], "message": "..."}`, or `{"error": "..."}`. The command objects are the protocol's (`amb protocol schema command`), limited to `set_param`, `adjust_param`, `reset_param` and `reset_group`. `crates/amb-plugin/src/tests.rs` has small hand-written modules.

## Examples

- [`plugins/auto-tone`](../../plugins/auto-tone): Exposure, Whites and Blacks from the histogram (`photo-stats`).
- [`plugins/random-look`](../../plugins/random-look): a new look from a style each run (the seed; no permissions).

Both are built into the app. After changing one, rebuild its module with `amb plugin build plugins/<id>` and commit `plugin.wasm` with the source.
