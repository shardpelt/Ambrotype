# Future ideas (backlog)

Ideas for later, written down so they aren't lost. None of them are built. Each must go through `amb-protocol` commands like every other edit, so it is undoable and works the same from the GUI, CLI and AI.

## Auto button

*Built 2026-10-12* (`amb_engine::matching::auto`, `amb auto`, Basic → Tone → Auto, WB → Auto): the tone sliders start from zero and the match solver stretches the photo's own tones so its black point, middle and white point move part of the way to L* 2, 46 and 95; white balance takes 60% of the cast out of the near-grey midtones, only when near-grey covers at least 15% of the photo (otherwise it keeps the camera's: in a scene with a little near-grey, water, dry grass or a metal ball, its colour says more about the object than the light). Not yet: Vibrance/Saturation, Shift-click per slider, calibration against Lightroom's Auto in the trial.

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

*Built 2026-10-12* (Presets → Surprise me…): six looks from the built-in Random look plugin, with consecutive seeds, rendered small without being applied; Style, Strength and Change colours re-roll them, More gives six new ones, and a click applies one as a single undo step named with its seed. Not yet: locks, scope per panel, "mutate" around the current look, film stocks, entering a seed.

Play with settings and discover looks you wouldn't have tried.

- Pick random values within **sensible ranges** per slider. Use narrower ranges and a bias towards subtle values, not uniform over −100..100, so most results are usable.
- **Scope:** all panels, or only the open panel. **Lock** sliders or groups so they're never changed (e.g. keep white balance and exposure).
- **Strength slider:** subtle ↔ wild. **Seeded,** so a result can be reproduced and shared ("seed 4821").
- **One undo step.** "Again" re-rolls, and "Save as preset" keeps a look you like.
- **Variants:** "randomize colour only", "randomize film stock" (with negatives), or "mutate": small random changes around the current look, for iterative exploring.
- Show a few random variants side by side as thumbnails, and click one to apply it.

## Tutorial mode

*v1 built 2026-10-12* (Learn, F1, or "Learn the basics" on the empty screen): lessons are `crates/amb-gui/ui/lessons.json`; a card shows one step, outlines the control it is about (opening its panel), and moves on by itself when the step's goal is met in the photo's settings (`changed`, `above`/`below`, a history label). Two lessons: Editing basics (on any open photo) and Coming from Lightroom. Not yet: a bundled sample raw, goals on image statistics, the hover before/after strip, translations, the AI coach.

A guided, interactive way to learn editing inside the app. Free, unlike most courses.

- **Lessons:**
  - Short steps on a bundled CC0 sample raw: "Recover the sky with Highlights", "Fix the white balance with Temp", "Add a vignette".
  - Each step highlights the control, explains what it does (from the settings descriptions we already have), and checks when the goal is reached by reading the settings or image statistics.
- **Learn mode for any photo:** hover a slider to see a before/after strip of what it does to *this* photo.
- **Lightroom switchers:** a lesson that maps Lightroom panels and shortcuts to ours, and shows how to load their presets and sidecars.
- **Optional AI coach (bring-your-own-AI):** explains why a preset looks the way it does, or suggests the next step. It can only use the same commands a user can, so every suggestion is visible and undoable.
- Lessons are data files (steps, target conditions, text), so the community can write and translate them.

## Feedback button and usage insights

Decide what to refine and build next from real use, not guesses. It must stay **privacy-first and opt-in**: nothing leaves the computer without the user seeing it and clicking send. That is part of the no-rug-pull promise in SCOPE.

- **Feedback button** (GUI toolbar and `amb feedback` in the CLI):
  - Pick a kind:
    - Bug
    - Idea
    - **Looks different from Lightroom**: the most valuable kind for us
    - Slow
  - Add a short text. Optionally attach context, all shown before sending:
    - app version and OS
    - camera and lens model
    - the current settings (XMP)
    - the not-rendered list
    - preview timing
  - The photo is attached only if the user ticks it.
- **Delivery without running a server:**
  - The default opens a pre-filled **GitHub issue** in the browser (title, labels, body). It's public, free, needs no account system, and is easy to triage and vote on with 👍.
  - Alternatives: a pre-filled email; later, a small self-hosted endpoint if volume needs it.
- **Usage insights (opt-in, local first):**
  - The app keeps a small local counter file:
    - which panels and sliders are used
    - which preset settings were **not rendered** (most important: it shows exactly what bought presets need)
    - preview and export times
    - crashes
  - "Share usage report" shows the file and sends it as an issue attachment.
  - No identifiers, no file names, no photos.
- **Closing the loop:** a `feedback` label set and a monthly "top requests" triage that updates GAPS.md. Release notes link back to the issues they close.

## Token-efficient AI docs and API

*Built 2026-10-11:* the core card ([ai/core.md](../ai/core.md), `amb ref`, about 360 tokens), `amb ref GROUP`, `amb do` with net-change output, `amb edit --brief` and `amb stats`, also over MCP. Look matching is still to come.

So AI assistants can drive Ambrotype cheaply and reliably: fewer tokens per task, fewer mistakes, fewer retries.

**Writing the docs in Chinese, pure math or binary would not help.** Today's model tokenizers are trained mostly on English and code.
- Chinese text usually costs about as many tokens as the same meaning in terse English, often more.
- Binary or machine code costs far more tokens per idea, and models misread it.
- Math notation is compact only for formulas.
- The real cost is mistakes: a cryptic doc causes wrong calls and retries, which cost more than the words saved.

What does save tokens:

1. **Tiered docs.** Load only what's needed. Current sizes (estimated at 4 characters per token):
   - `llms.txt`: about 400 tokens
   - `docs/ai/README.md`: about 4,000
   - `docs/ai/settings-reference.md`: about 15,000

   Plan:
   - a ≤ 300-token **core card**: the command grammar plus the 30 most used settings, with ranges in one line each
   - per-group reference files fetched on demand (`amb ref tone`)
   - the full reference only for search
2. **Terse, regular formats.**
   - One line per setting: `Exposure2012 -5..5 =0 EV`.
   - Short, stable names. No prose where a table or line does the job.
   - The same wording everywhere, so prompt caching works: a stable prefix is cached and then billed at a fraction of the price.
3. **Measure, don't guess.** Count tokens with the provider's token-count API in CI, and fail if the core card grows past its budget.

**Token-efficient API (yes, a good idea):**
- **Compact command syntax.** For example `amb do photo.nef "Exposure2012=+0.5 Contrast2012+=10 reset:hsl"`: one short string instead of a JSON array. Short aliases are optional (`exp`, `con`, `hl`, `sh`), defined once in the core card.
- **Batch and diff-only output.** Many edits per call. `--brief` replies with only what changed (`Exposure2012 0→0.5`) plus warnings, not the whole state.
- **Numbers instead of pictures.** An image costs roughly a thousand tokens or more. `amb stats photo.nef --brief` can return a few hundred tokens: histogram percentiles, clipping %, mean colour, white-balance estimate. Check a picture only when needed, at small size.
- **MCP server.** Assistants call typed tools directly, with no parsing of help text, and the tool list is cached. Same commands as `amb-protocol`.
- **Look matching does the heavy lifting.** `amb match photo.nef ref1.jpg ref2.jpg` solves the sliders in code. The AI then reviews a short diff instead of iterating by trial and error.
