# AI-free mode

Status: implemented 2026-11-24. One switch, **AI-free mode**. With it on, no learned model touches a photo, by any path. It is for people who must keep an "AI" label off their photos on social platforms and elsewhere, and who need a switch they can trust.

Code: `crates/amb-presets/src/ai_free.rs` (the switch, `check()`, `left_out()`), `crates/amb-library/src/ai_free.rs` (the saved file), `crates/amb-engine/src/ai_free.rs` (what an edit leaves out), `crates/amb-gui/ui/dialogs/ai-free.js` (the editor), `crates/amb-cli/src/ai_free.rs` (`amb ai-free`). Guards: `every_model_path_passes_ai_free_mode` in `crates/amb-cli/tests/code_health.rs`, `crates/amb-ops/tests/ai_free.rs`, `crates/amb-cli/tests/ai_free.rs`.

## The promise

While the mode is on, no AI model loads, downloads or runs on a photo. That holds for the editor, `amb`, the MCP server, the preview, export, batch operations and search. It also holds for a photo edited earlier: an edit that already holds an AI part still opens, but that part is left out of the picture and of every export.

The mode never changes an edit. The settings stay in the sidecar as they were; only the render leaves the AI parts out. Turn the mode off and they are back.

## What counts as AI, and what does not

AI is a **learned model**: a neural network that makes or decides pixels. These are AI here:

- the Subject, Background and People masks (and what builds on them: Select Object, Describe it, Remove object);
- AI Remove (spots of kind `remove` and `content-aware`);
- AI Denoise, and Lightroom's Enhance settings (`Enhance*`);
- Smart search and Find similar photos (the search model);
- the eye finder in Best in each burst (YuNet).

These are **not** AI, and stay on:

- the Sky mask (our own rules on brightness and colour), luminance and colour ranges;
- Auto, Auto tone and Auto white balance (statistics of the photo), Match look, Styles;
- heal and clone, Find dust, lens corrections, Upright, film negative conversion.

Nothing in a heuristic learns from data. If a heuristic is ever replaced by a model, it becomes AI here and gets the gates below.

## Why an outside AI agent driving our commands is fine

An AI assistant or an MCP client that edits through `amb` commands moves the same settings a person would: exposure, a curve, a mask drawn by hand. The pixels are changed by our own pipeline, which is not a learned model. Labels on social platforms come from generated content and from its metadata, not from who moved a slider. So the mode does not stop an assistant; it stops our models.

An assistant can still not add an AI part while the mode is on: those commands are refused (see below).

## The gates

Every way a model could run goes through `amb_presets::ai_free::check()` or the registry's flags:

- **Engine hooks** (`GATED_HOOKS` in `amb_presets::ai_free`): the subject finder and selector, the face finder, the inpainter and the raw denoiser stay installed, and each acts as if absent while the mode is on. The editor can switch at run time.
- **`amb-ai`**: every model load, every download and every run calls `check()` first (`model.rs`, `net/`, `store.rs`, `search.rs`, `objects.rs`, `remove.rs`, `denoise.rs`, `faces.rs`).
- **`amb-ops`**: Select Object and its words, and AI Denoise, check first.
- **The protocol**: every command that would add an AI setting, an AI mask part or an AI spot is refused with `ai_free::MESSAGE`. That includes presets, snapshots, undo, redo, duplicate and a plugin's grouped commands (`refuse_ai` in `crates/amb-protocol/src/state/mod.rs`). The registry says what is AI (`"ai": true` in `crates/amb-presets/data/tags.json`, and the lists `ai_mask_kinds`, `ai_mask_subtypes`, `ai_spot_kinds`).
- **Render**: AI Remove spots and model mask parts render as nothing (an unsupported part does not fill the frame when inverted).
- **Export**: a batch export reports what each photo left out as `warnings`.
- **The editor**: the switch, a badge, AI controls turned off (marked `data-ai`, driven from the registry's lists), and a note naming what the open edit leaves out.
- **`amb ai-free [on|off]`**: shows or sets the saved switch.

## The saved file and the fail-safe

The setting is saved in `ai-free.json` in the editor's config folder (`amb_library::ai_free`). `amb` and the editor read it at start-up. A missing file means off. A file that exists but cannot be read or parsed means **on**: someone who asked for AI-free photos is trusted over convenience.

## Rules for future work

Every new feature must respect the promise. A guard test fails for the first two; the rest are by review.

1. **Every new model load, download or run calls `amb_presets::ai_free::check()` first** (in `amb-ai`, or `amb-ops` for an operation that renders). `every_model_path_passes_ai_free_mode` counts the load sites per file against the `check()` calls; a new site means a conscious edit of its table.
2. **Every new engine hook** (`pub fn set_…` in `amb-engine`) is gated at call time and listed in `GATED_HOOKS`. The test compares the two lists.
3. **Every new setting, mask kind or spot kind that needs a model is flagged in `tags.json`** (`"ai": true`, or its kind in the `ai_*` lists), so the protocol refuses it and `left_out()` names it. Without the flag the switch has a hole.
4. **A new control that ends in a model gets `data-ai`** (or is built from the registry's lists), so it turns off in the editor. The core still refuses it if the control is missed.
5. **A new AI feature gets a step in `crates/amb-ops/tests/ai_free.rs`**: counting hooks must see zero calls with the mode on and some with it off.
6. **Never turn the mode off for a person.** Only they do, in Settings, or `amb ai-free off`.
7. **Keep the words true**: a label or doc that says "AI" is for a model; a heuristic is not called AI (the Sky mask says so).

## Limits

- A plugin's wasm code cannot be inspected for its own AI: the mode cannot know what a third-party plugin does inside its sandbox ([../GAPS.md](../GAPS.md)).
- Single-photo export and Convert raws leave the AI parts out but show no warning yet; the batch export does.
- Outside the app, nothing is claimed: a photo that came out of another tool with an AI label keeps it.
