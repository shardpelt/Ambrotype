# Future ideas (backlog)

Ideas for later, written down so they aren't lost. None of them are built. Each must go through `amb-protocol` commands like every other edit, so it is undoable and works the same from the GUI, CLI and AI.

## Auto button

One click for a good starting point, like Lightroom's Auto.

- **Auto (Basic):** set Exposure, Contrast, Highlights, Shadows, Whites, Blacks, Vibrance and Saturation from image statistics: histogram percentiles, mid-grey placement, clipping, and saturation of the brightest colours.
  - Shift-click per slider: auto that slider only, like Lightroom's Shift+double-click.
- **Auto white balance:** grey-world or white-patch estimate, written as Temperature/Tint.
- **Per-panel autos:**
  - Upright: auto, with the transform.
  - Film negative: already exists, as `amb:NegativeAuto`.
  - Lens profile: auto, already exists.
- **How:**
  - Output is ordinary slider values, so the user can see and tweak them.
  - Deterministic: same photo, same result.
  - Calibrate it against Lightroom's Auto with the trial-month data (record Lightroom's Auto values per test raw).
- **AI variant (optional, bring-your-own-AI):** "Auto in the style of …" using the `amb match` look-matching solver.

## Randomize button ("Surprise me")

Play with settings and discover looks you wouldn't have tried.

- Pick random values within **sensible ranges** per slider. Use narrower ranges and a bias towards subtle values, not uniform over −100..100, so most results are usable.
- **Scope:** all panels, or only the open panel. **Lock** sliders or groups so they're never changed (e.g. keep white balance and exposure).
- **Strength slider:** subtle ↔ wild. **Seeded,** so a result can be reproduced and shared ("seed 4821").
- **One undo step.** "Again" re-rolls, and "Save as preset" keeps a look you like.
- **Variants:** "randomize colour only", "randomize film stock" (with negatives), or "mutate": small random changes around the current look, for iterative exploring.
- Show a few random variants side by side as thumbnails, and click one to apply it.

## Tutorial mode

A guided, interactive way to learn editing inside the app. Free, unlike most courses.

- **Lessons:**
  - Short steps on a bundled CC0 sample raw: "Recover the sky with Highlights", "Fix the white balance with Temp", "Add a vignette".
  - Each step highlights the control, explains what it does (from the settings descriptions we already have), and checks when the goal is reached by reading the settings or image statistics.
- **Learn mode for any photo:** hover a slider to see a before/after strip of what it does to *this* photo.
- **Lightroom switchers:** a lesson that maps Lightroom panels and shortcuts to ours, and shows how to load their presets and sidecars.
- **Optional AI coach (bring-your-own-AI):** explains why a preset looks the way it does, or suggests the next step. It can only use the same commands a user can, so every suggestion is visible and undoable.
- Lessons are data files (steps, target conditions, text), so the community can write and translate them.
