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
 CLI ─┼──►  amb-protocol (typed commands + queries)  ──►  engine / catalog
 MCP ─┤        set_param · apply_preset · add_mask · render_preview
 Chat ┘        image_stats · match_look · export · list_presets · ...
```

- **`amb-protocol`** is the single command API the GUI itself uses. It is already planned in SCOPE §9. If the GUI can do something, a script or an AI can too, with the same validation, undo and history.
- **Surfaces that expose it:**
  1. **CLI:** `amb` with `--json` output. It has started: `amb preset inspect --json`, `amb preset dump`.
  2. **MCP server:** `amb mcp` exposes tools and resources over the Model Context Protocol. This is how "use my existing AI subscription" works in practice. The user's own AI app (Claude Desktop, Claude Code, or any MCP-capable client) connects to Ambrotype and drives it, billed to the user's existing plan. We never handle their credentials.
  3. **In-app assistant panel:** a chat or terminal-style window inside Ambrotype. It works with a user-supplied **API key** (Anthropic, OpenAI, Google, ...) or a **local model** (Ollama / llama.cpp). Keys are stored in the OS keychain.
  - Note: consumer chat subscriptions generally cannot be plugged into third-party apps directly. The supported routes are MCP (their AI app drives ours) and API keys. We say so plainly in the UI.
- **AI-readable documentation:** a generated `docs/ai/` guide and an `llms.txt` covering:
  - every control's name, meaning, range and default (generated from the tag registry `amb-presets/data/tags.json`, so it never drifts from the code)
  - the tools, with examples
  - photographic intent ↔ slider mappings ("warmer and more filmic" → Temperature +, Tint slightly +, a lifted black point in the tone curve, lower Saturation and higher Vibrance)

## "Make it look like these samples": `amb match`

*v1 built 2026-10-12* (`amb_engine::matching`, `amb_library::look`, `amb match`, MCP `match_look`, and the editor's Presets → Match look of other photos…, which shows the proposed settings on the photo before they are applied as one undo step). What it does today: the references and the photo are developed small (256 px) and measured as L* percentiles, the mean a*/b* of soft shadow, midtone and highlight zones, chroma percentiles and the spread of a*/b* (which tells saturation from a white balance shift). Levenberg–Marquardt moves white balance, the six tone sliders, Vibrance, Saturation and the shadow and highlight Color Grading tints together, with a finite-difference Jacobian (one render per setting per step) and a small penalty per change. On a synthetic raw it recovers a known edit (Temperature within 10 K, Exposure and Saturation exactly) in about 60 renders; a JPEG reference gets close with a different mix of sliders. Not yet: HSL per colour, the tone curve, an explanation in words, per-photo adaptation in batches.

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
  - `amb-protocol` command set for develop parameters
  - `--json` everywhere in the CLI
  - the generated settings reference (`docs/ai/`)
- **Phase 2:**
  - ~~`amb mcp` server (develop, preset, render-preview and export tools)~~ (done 2026-10-10; [ai/mcp.md](../ai/mcp.md))
  - ~~`amb match` v1 (global tone/colour statistics → parameters)~~ (done 2026-10-12)
- **Phase 3:**
  - the in-app assistant panel (BYO API key / local model)
  - a vision refinement loop
  - match with masks (sky, subject)
- **Later:** learn-my-style from edit history; batch "apply my look to this shoot" with per-photo adaptation.
