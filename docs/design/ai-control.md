# Design: AI-controllable editing (bring your own AI)

Status: proposal, 2026-10-04. The owner's idea: an assistant window inside the app, driven by the user's own AI, so a user can say *"make this photo look like these three samples"* and the AI does it with the app's real controls.

**Verdict: a good idea, and a real differentiator, if it follows four rules.** Users hate forced AI, paywalled AI and AI that starves performance work ([research/12 §2.2, §7](../research/12-user-feedback-and-design-implications.md)). So:
1. **Optional.** The app is fully usable without any AI, and nothing nags.
2. **The AI only uses the same controls a human uses.** Every AI action is a normal, undoable edit (slider values, masks, presets) visible in history. The AI never paints pixels. The result stays a non-destructive recipe the user can tweak.
3. **Local and private by default.** No image leaves the machine unless the user picks a cloud model and confirms. Local models are supported. No telemetry.
4. **The heavy lifting is deterministic code, not the language model.** "Match these samples" is an algorithm we own (below). The AI orchestrates, explains and iterates, so the feature also works offline, cheaply and reproducibly.

## Architecture: the AI is just another client

```
 GUI ─┐
 CLI ─┼──►  flb-protocol (typed commands + queries)  ──►  engine / catalog
 MCP ─┤        set_param · apply_preset · add_mask · render_preview
 Chat ┘        image_stats · match_look · export · list_presets · ...
```

- **`flb-protocol`** is the single command API the GUI itself uses. It is already planned in SCOPE §9. If the GUI can do something, a script or an AI can too, with the same validation, undo and history.
- **Surfaces that expose it:**
  1. **CLI:** `flb` with `--json` output. It has started: `flb preset inspect --json`, `flb preset dump`.
  2. **MCP server:** `flb mcp` exposes tools and resources over the Model Context Protocol. This is how "use my existing AI subscription" works in practice. The user's own AI app (Claude Desktop, Claude Code, or any MCP-capable client) connects to Flightbroom and drives it, billed to the user's existing plan. We never handle their credentials.
  3. **In-app assistant panel:** a chat or terminal-style window inside Flightbroom. It works with a user-supplied **API key** (Anthropic, OpenAI, Google, ...) or a **local model** (Ollama / llama.cpp). Keys are stored in the OS keychain.
  - Note: consumer chat subscriptions generally cannot be plugged into third-party apps directly. The supported routes are MCP (their AI app drives ours) and API keys. We say so plainly in the UI.
- **AI-readable documentation:** a generated `docs/ai/` guide and an `llms.txt` covering:
  - every control's name, meaning, range and default (generated from the tag registry `flb-presets/data/tags.json`, so it never drifts from the code)
  - the tools, with examples
  - photographic intent ↔ slider mappings ("warmer and more filmic" → Temperature +, Tint slightly +, a lifted black point in the tone curve, lower Saturation and higher Vibrance)

## "Make it look like these samples": `flb match`

Prior art shows demand. Imagen and Aftershoot sell AI "personal profiles" that learn a photographer's style from past Lightroom edits ([research/09](../research/09-market-commercial-alternatives.md)).

The core is a deterministic look-matching solver:
1. **Analyse the reference images:**
   - luminance distribution: tone curve and contrast
   - per-hue-band chroma and lightness: HSL targets
   - white-point estimate
   - colour-grading tints in shadows, midtones and highlights
   - saturation, vibrance and clarity proxies (local contrast energy)
2. **Analyse the target the same way,** after its own base render.
3. **Solve for parameters, not pixels.** Optimise our engine's parameters (Temperature/Tint, Exposure, Contrast, Highlights/Shadows/Whites/Blacks, tone curve, HSL, Color Grading, Vibrance/Saturation) to minimise the statistical distance. Content-independent statistics avoid "copying" the reference's subject.
4. **Output a preset or recipe with a human-readable explanation,** e.g. "warmed WB by 450 K, lifted blacks, desaturated greens". It's applied with an Amount slider, like any preset.
5. **Optional AI loop:** a vision-capable model looks at the render next to the references and proposes targeted adjustments ("skin is too orange") through the same tools, within a step budget and with preview before applying.

Later we can also learn a style from the user's own edit history (Imagen-style), locally.

## Safety and UX rules
- **Preview, then apply.** AI changes show as a pending diff ("8 sliders changed") with Accept / Undo.
- **Permission prompts** before batch operations, exports that overwrite files, deletes or moves, and anything that sends images to a cloud model.
- **Step and cost limits** per request, shown to the user.
- **Logging:** a local, user-visible log of AI tool calls and their arguments.
- **No AI in core correctness paths.** Rendering, preset import and catalog integrity never depend on a model.

## Roadmap placement
- **Phase 1:**
  - `flb-protocol` command set for develop parameters
  - `--json` everywhere in the CLI
  - the generated settings reference (`docs/ai/`)
- **Phase 2:**
  - `flb mcp` server (develop, preset, render-preview and export tools)
  - `flb match` v1 (global tone/colour statistics → parameters)
- **Phase 3:**
  - the in-app assistant panel (BYO API key / local model)
  - a vision refinement loop
  - match with masks (sky, subject)
- **Later:** learn-my-style from edit history; batch "apply my look to this shoot" with per-photo adaptation.
