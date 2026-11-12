# Ambrotype docs map

Scope: 70 Markdown files (git ls-files '*.md', minus docs/ai/settings-reference.md, docs/ai/core.md, crates/amb-gui/data/hub/**). Read-only; no repo file edited; cargo not run.

Method: headings of every file, opening lines, grep for stale signals ("not yet", "planned", "Not yet", "built"), path check of every repo path mentioned (ls/test -e, plus crate and ui/ shorthand), relative link check (all `](...)` links resolve), and file-size and crate-list checks against `ls`. Purposes come from headings and opening lines; not every line of the 14k was re-read.

Audience key: U = user, C = contributor, A = agent driving the app (amb CLI/MCP), K = coding agent changing the code.

## Table

| path | lines | last change | audience | purpose | overlaps with |
|---|---|---|---|---|---|
| .github/pull_request_template.md | 13 | 2026-10-23 | C | PR form: what and why, how checked, licence | AGENTS/CONTRIBUTING checks |
| AGENTS.md | 49 | 2026-11-11 | K | Coding-agent guide: layout, common changes, pre-commit checks, conventions | CONTRIBUTING, recipes, README code table |
| CLAUDE.md | 5 | 2026-10-23 | K | Imports AGENTS.md; reporting and licence rules | AGENTS |
| CODE_OF_CONDUCT.md | 24 | 2026-10-23 | C | Conduct rules, scope, reporting | CONTRIBUTING "Conduct and security" |
| CONTRIBUTING.md | 42 | 2026-10-23 | C | How to contribute: steps, AI-assisted work, licence, hub pointer | AGENTS (checks), recipes, legal, PR template |
| README.md | 87 | 2026-11-10 | U | Front page: what it does, build, code table, docs index, licence | ARCHITECTURE crates, AGENTS layout, desktop-setup build |
| SECURITY.md | 20 | 2026-10-23 | U | Vulnerability reporting, scope, supported versions | CONTRIBUTING "Conduct and security" |
| docs/ARCHITECTURE.md | 405 | 2026-11-12 | C | How it is built today: crates, data flow, export, editor, what does not exist | README, design/structure crate table, GAPS, SCOPE §9 |
| docs/FEATURES.md | 147 | 2026-11-12 | U | What the app does, by task, with where and the amb command | design/* feature notes, desktop-setup tour, ai/README commands |
| docs/GAPS.md | 359 | 2026-11-12 | C | Every known gap, guess and open question; top 10 | design/* "Not yet"/"Open questions", WHATS-NEXT, NEXT, release-notes limits, ARCHITECTURE "does not exist" |
| docs/NEXT.md | 173 | 2026-11-09 | C | Where we are; next steps by track; v0.1 release checklist | WHATS-NEXT, GAPS top 10, SCOPE §8, cleanup |
| docs/SCOPE.md | 457 | 2026-11-09 | C | Original scope: vision, licence decision, phased roadmap, risks, doc index | research/*, ARCHITECTURE §9 target, NEXT, legal §licence |
| docs/WHATS-NEXT.md | 160 | 2026-11-12 | C | Running short list: waiting on owner, up next, ideas, recently done | NEXT, GAPS top 10, design/future-ideas |
| docs/cleanup.md | 644 | 2026-11-09 | K | Exact cleanup steps; all S1-F14 ticked, so now a done log | design/structure (plan), design/agent-navigation, recipes |
| docs/desktop-setup.md | 191 | 2026-11-11 | U | Build and run per OS; editor tour and shortcuts; Lightroom measuring; local AI agent | README build, releasing, lightroom-trial-plan, ai/README |
| docs/legal.md | 38 | 2026-10-23 | C | Legal position: names, licence, releases, what is legal to do | SCOPE §6, research/02 §3.4, CONTRIBUTING licence, README licence |
| docs/lightroom-trial-plan.md | 189 | 2026-10-29 | C | Step-by-step Lightroom trial: kit, batches, plug-in | desktop-setup §3, design/calibration, GAPS §14 |
| docs/make-it-yours.md | 67 | 2026-10-30 | U | How to change a layout or theme and share through the hub | ai/ui-layouts, design/customization-and-hub, FEATURES, CONTRIBUTING |
| docs/own-models.md | 57 | 2026-10-23 | U | Use your own ONNX models; MediaPipe people model setup | ARCHITECTURE amb-ai, ai/README "AI models" |
| docs/recipes.md | 93 | 2026-11-09 | K | Step-by-step for common changes; "done means it works" | AGENTS "Common changes", cleanup (moved paths), structure |
| docs/releasing.md | 43 | 2026-10-23 | C | How release installers are built, signed, published | desktop-setup §0, release-notes, NEXT checklist, GAPS §15 |
| docs/release-notes/v0.1.0.md | 43 | 2026-10-23 | U | Notes for the v0.1.0 preview (draft release text) | NEXT checklist, releasing, GAPS "Known limits" |
| docs/ai/README.md | 337 | 2026-11-11 | A | Guide for an AI driving the app: edit loop, safety, commands, queries, intent cheat sheet | ai/mcp, ai/core.md, ai/custom-interface, desktop-setup AI section, ai/film-recipes |
| docs/ai/assistant.md | 34 | 2026-11-09 | U | The in-app assistant panel: model, context, answers, not yet | design/ai-control, ai/README |
| docs/ai/custom-interface.md | 112 | 2026-11-09 | A | Build a whole new front end on the app's commands; rules and sharing | design/customization-and-hub §1-3, ai/ui-layouts, make-it-yours |
| docs/ai/film-recipes.md | 85 | 2026-10-30 | U | Paste a Fuji recipe; map to our looks; trademark note | design/film-look-measurement, design/film-negatives |
| docs/ai/mcp.md | 58 | 2026-11-11 | A | Running `amb mcp`: setup per client, tools, behaviour, not yet | ai/README (commands), ARCHITECTURE, GAPS §12 |
| docs/ai/plugins.md | 221 | 2026-11-11 | K | How to write a WASM plugin: quick start, code, plugin.json, kinds, rules | design/plugin-kinds, customization-and-hub §5, ARCHITECTURE amb-plugin, GAPS plugins row |
| docs/ai/ui-layouts.md | 221 | 2026-11-11 | K | Format reference for layout and theme files | make-it-yours (how-to), customization-and-hub §1-2, custom-interface |
| docs/design/agent-navigation.md | 163 | 2026-11-09 | K | Plan and results for helping agents find their way (test rounds A1, B1) | design/structure, cleanup, recipes |
| docs/design/ai-control.md | 74 | 2026-10-23 | C | Design: AI as another client of amb-protocol; `amb match`; safety | ai/README, ai/assistant, SCOPE §9 |
| docs/design/calibration.md | 31 | 2026-10-23 | C | Calibration panel: tags, model, open questions | lightroom-trial-plan, GAPS §14 |
| docs/design/camera-profiles.md | 41 | 2026-10-23 | C | Camera profiles (DCP): sources, choice, application, not yet | GAPS §5, research/02 §3 |
| docs/design/collage-and-borders.md | 116 | 2026-11-09 | U/C | Borders, watermarks, collage maker, layout format | FEATURES "Getting photos out", ai/ui-layouts |
| docs/design/crop.md | 68 | 2026-11-09 | C | Crop and straighten: tags, semantics, pipeline, tool | FEATURES, GAPS §8 |
| docs/design/customization-and-hub.md | 166 | 2026-11-09 | C | Layouts, themes, editor mode, hub, plugins, AI-made features | make-it-yours, ai/ui-layouts, ai/custom-interface, ai/plugins, design/plugin-kinds |
| docs/design/dust.md | 71 | 2026-11-09 | C | Dust and hairs on film scans: finding, lines, strength, bench | design/spot-removal, design/film-negatives |
| docs/design/film-look-measurement.md | 44 | 2026-10-30 | C | Plan for measuring Fujifilm simulations (status "planned") | ai/film-recipes, GAPS §9 |
| docs/design/film-negatives.md | 103 | 2026-11-09 | C | Film negatives: settings, model v1, stocks, workflow | design/dust, ai/film-recipes, FEATURES "Film negatives" |
| docs/design/future-ideas.md | 115 | 2026-10-23 | C | Backlog with "Built" notes: Auto, Surprise me, tutorial, feedback, token-efficient AI | WHATS-NEXT "Ideas", GAPS §0 |
| docs/design/gpu.md | 69 | 2026-11-09 | C | GPU preview: how, tiling, stages, checks, AI models | ARCHITECTURE amb-gpu, GAPS §13 |
| docs/design/hdr-merge.md | 36 | 2026-10-23 | C | HDR merge: use, method, DNG, gaps | design/panorama (same layout) |
| docs/design/lens-corrections.md | 106 | 2026-11-09 | C | Lens corrections: what we render, lensfun database, legal line | GAPS §8, ARCHITECTURE amb-lens |
| docs/design/library.md | 228 | 2026-11-11 | U/C | The library: folders, timeline, import, collections, duplicates, Lightroom catalogue | FEATURES "Library", ARCHITECTURE amb-library |
| docs/design/luts.md | 23 | 2026-10-23 | C | LUT (.cube) looks: rendering, editor, not yet | FEATURES "Getting photos out" |
| docs/design/masks.md | 116 | 2026-11-09 | C | Masks and local adjustments: format, Select Sky, model, commands | design/spot-removal, ai/README "Masks", FEATURES Editing |
| docs/design/panorama.md | 48 | 2026-11-09 | C | Panorama merge: use, method, checks, gaps | design/hdr-merge |
| docs/design/plugin-kinds.md | 33 | 2026-11-09 | C | The three plugin kinds and built-in tasks | ai/plugins, customization-and-hub §5 |
| docs/design/spot-removal.md | 60 | 2026-11-09 | C | Heal and clone spots; AI Remove | design/dust, design/masks, ai/README "Spot removal" |
| docs/design/structure.md | 206 | 2026-11-09 | K | Codebase shape, sizes, problems, target shape, rules to stay lean | cleanup, agent-navigation, ARCHITECTURE crates |
| docs/design/tabs.md | 59 | 2026-11-11 | K | Which UI kind (tab, mode, view, tool, dialog) a feature is | AGENTS "Conventions", ui-layouts |
| docs/design/transform.md | 81 | 2026-11-09 | C | Transform and guided Upright | design/lens-corrections, GAPS §8 |
| docs/research/01-lightroom-feature-inventory.md | 302 | 2026-10-23 | C | Lightroom feature inventory (reference) | research/02, 13, SCOPE §3 |
| docs/research/02-preset-and-profile-formats.md | 345 | 2026-10-23 | C | Lightroom preset and profile formats | design/camera-profiles, legal, GAPS §10 |
| docs/research/03-darktable-survey.md | 414 | 2026-10-23 | C | darktable source survey | research/04, 05, 11 |
| docs/research/04-oss-ecosystem.md | 505 | 2026-10-23 | C | Open-source raw ecosystem survey | research/03, 06, 07, SCOPE §4 |
| docs/research/05-processing-algorithms.md | 295 | 2026-10-23 | C | How ACR/Lightroom works; open approximations | research/02, GAPS §4-5 |
| docs/research/06-tech-stack-options.md | 425 | 2026-10-23 | C | Language, GPU, UI, catalog stack options | research/07, 11, SCOPE §9 |
| docs/research/07-rapidraw-survey.md | 267 | 2026-10-23 | C | RapidRAW survey | research/04, 06, SCOPE §5 |
| docs/research/08-cross-platform-and-mobile.md | 449 | 2026-10-23 | C | Cross-platform parity and future mobile | SCOPE §8-9, research/15 |
| docs/research/09-market-commercial-alternatives.md | 266 | 2026-10-23 | C | Commercial alternatives market | research/17, 14, SCOPE §4 |
| docs/research/10-reading-list.md | 288 | 2026-10-23 | C | Reading list | research/* |
| docs/research/11-architecture-lessons-from-large-oss.md | 677 | 2026-10-23 | C | Architecture lessons from large OSS projects | research/06, cleanup, structure |
| docs/research/12-user-feedback-and-design-implications.md | 340 | 2026-10-23 | C | User feedback and design implications | research/13, 15, GAPS §2 |
| docs/research/13-adobe-community-ideas.md | 473 | 2026-10-23 | C | Adobe community feature requests | research/12, FEATURES |
| docs/research/14-dpreview-and-preset-sellers.md | 203 | 2026-10-23 | C | DPReview and preset sellers | research/09 |
| docs/research/15-app-store-reviews.md | 335 | 2026-10-23 | C | App store reviews | research/12, 08 |
| docs/research/16-film-scan-apps-survey.md | 196 | 2026-10-31 | C | Film scan apps survey | design/film-negatives, dust |
| docs/research/17-competitors-2026.md | 152 | 2026-11-11 | C | 2026 competitors | research/09, watchlist |
| docs/research/watchlist.md | 46 | 2026-11-11 | C | Competitors to watch with dates | research/17 |

## Broken references

Raw hits from the path check: 116 (before shorthand and historical filtering). Breakdown:

Real, in-repo, currently wrong (3):
- docs/GAPS.md:73 → `docs/decisions/` (missing)
- docs/SCOPE.md:281 → `docs/decisions/` (missing)
- docs/research/11-architecture-lessons-from-large-oss.md:277 → `docs/decisions/` (cites this folder; not present)

Resolve only with a crate prefix (shorthand, not broken): 
- AGENTS.md:6,9; README.md:48; ARCHITECTURE.md:16,21,33,59; design/ai-control.md:27 → `data/tags.json`, `data/models.json` = `crates/amb-presets/data/`, `crates/amb-ai/data/`
- ARCHITECTURE.md:139,261,262,266,356; design/crop.md:52; design/masks.md:97; design/spot-removal.md:41 → `tools/*.js` = `crates/amb-gui/ui/tools/`
- ARCHITECTURE.md:249; design/library.md:206 → `tests/ui/` = `crates/amb-gui/tests/ui/`
- ARCHITECTURE.md:302; NEXT.md:35 → `data/sample/` = `crates/amb-gui/data/sample/`
- ARCHITECTURE.md:313; design/agent-navigation.md:84,87,97,113; design/customization-and-hub.md:3 → `data/hub/` = `crates/amb-gui/data/hub/`

Other repos, user's own setup, or URL templates (not broken, but unclear without a note):
- docs/design/customization-and-hub.md:89; docs/make-it-yours.md:56; docs/recipes.md:93 → `tools/build_index.py` lives in the hub repo, not here
- docs/desktop-setup.md:175 → `.claude/agents/` is the user's own folder (missing here, as expected)
- docs/ai/plugins.md:21-25 → `plugins/warm-it-up` is created by the tutorial command
- docs/ai/mcp.md:44 → `ambrotype://docs/NAME` is a URI template
- docs/design/lens-corrections.md:81 → `data/db` is lensfun's upstream layout
- docs/releasing.md:31 → `crates/amb-gui/binaries/amb-` is a prefix; folder exists
- docs/own-models.md:7 → `assets/Model` is a false match inside a URL
- docs/research/* (54 hits) → external projects (rawler, gpui, FFmpeg, NegPy, negaflow, Zed, Lapce, etc.); snapshots, not this repo

Historical (paths moved by completed cleanup steps, docs/cleanup.md, 20 hits):
- crates/amb-gui/src/session.rs, looks.rs, hub.rs; crates/amb-protocol/src/state.rs, masks.rs; crates/amb-engine/src/mask.rs, negative.rs, lens.rs; crates/amb-ai/src/net.rs; crates/amb-library/src/object.rs (now amb-ops); each is a "git mv" source. Note: the moved files are now folders (`session/`, `looks/`, `hub/`, `state/`, `masks/`, `mask/`, `negative/`). Those names exist under the same stems.
- Placeholders: `data/x.json`, `crates/amb-library/src/X.rs`, `crates/amb-ops/src/X.rs`, `crates/amb-gui/ui/NAME.js`, `ui/FOLDER/NAME.js`.

Relative Markdown links `](...)`: all resolve (0 broken).

## Stale statements

- docs/design/structure.md:90: "UI scripts over 900 lines: `library.js` 1,488, `collage.js` 1,452, `app.js` 1,188, `masks.js` 1,082, `layout.js` 1,009, `editor.js` 989." Now library.js is 554 lines, masks.js 524; cleanup F9-F13 split them (2026-11-09). The table in this doc is a 2026-11-09 snapshot that is not marked as before-cleanup.
- docs/design/structure.md:150: "An agent reading `session.rs` or `library.js` loads 1,500 to 1,900 lines to change ten." session.rs was split (F2); library.js is now 554 lines.
- docs/design/structure.md:203: "don't grow `app.js`, `library.js`, `session.rs` or `main.rs`." Rule still fine; file names it protects were split.
- docs/design/agent-navigation.md:153: "Clean up the code ... (planned in [structure.md], the steps in [cleanup.md])." Cleanup is done (all ticked, 2026-11-09).
- docs/cleanup.md:3: "Exact steps for the cleanup planned in [design/structure.md]." Every S and F step is ticked; the doc is now a log.
- docs/design/ai-control.md:20: "It is already planned in SCOPE §9." amb-protocol is built and is the GUI's own API (ARCHITECTURE).
- docs/research/11-architecture-lessons-from-large-oss.md:5: lead reviewer note says SCOPE "still planned a future iOS app and an MPL-2.0 core". Superseded by SCOPE §6 GPL decision (2026-10-08). Needs a banner.
- docs/research/08-cross-platform-and-mobile.md:348: `crates/core/*`, `crates/gpl-pack/*` are an old layout (SCOPE 2026-10-03 draft), not current.
- docs/design/film-look-measurement.md:3: "Status: planned (2026-10-08)." Verify still true; film-recipes is 2026-10-30.
- docs/SCOPE.md:219-220: "the project was renamed **Ambrotype** (for now)" and "Trademark registers (TMview, WIPO) not yet searched." Verify; legal.md does not mention a search. Also SCOPE.md:431 "(for now)".
- docs/ARCHITECTURE.md:12: "About 104,000 lines of Rust, 735 Rust tests and 3 UI test scripts (89 steps)" repeated in NEXT.md:7 and structure.md:11. Same numbers in three places; re-check together.
- README.md:9, SECURITY.md:20, desktop-setup.md:5, release-notes, NEXT v0.1 checklist, GAPS §15: "no installers / no releases yet". Consistent today (v0.1 unchecked), but five places to update at release.

Not stale, checked: "no automated UI tests" is gone (UI tests exist: AGENTS, recipes, ARCHITECTURE, GAPS §2 all agree). "No build step, no npm" for crates/amb-gui/ui is consistent across AGENTS, ARCHITECTURE, structure, custom-interface.

## Overlap groups

1. Roadmap and "what's next" (6 docs): NEXT.md (tracks, checklist), WHATS-NEXT.md (short list, recently done), GAPS.md §0 and "Top 10 to close next", SCOPE.md §8 (phases), design/future-ideas.md (backlog), cleanup.md (steps). Each restates priorities; they disagree in dates and order over time.
2. "Not yet" and open questions (14+ docs): ARCHITECTURE "What does not exist yet", GAPS.md (every row), FEATURES.md, and "Not yet" or "Open questions" sections in design/camera-profiles, dust, gpu, library, luts, plugin-kinds, lens-corrections, crop, masks, spot-removal, transform, calibration, plus ai/assistant, ai/mcp. Each design note repeats "Open questions (for the Lightroom trial)".
3. Crate and layout lists (4 docs): AGENTS.md Layout, README.md code table, ARCHITECTURE.md crate table, design/structure.md crates table. Each has a different set of rows (see Crate list mismatches).
4. Checks and build commands (5 docs): AGENTS.md "Before you commit", CONTRIBUTING.md (steps), recipes.md "Done means", .github PR template "How it was checked", desktop-setup build, cleanup.md "Checks". fmt/clippy/test lines are repeated.
5. Editor UI tests and Xvfb (7 docs): AGENTS.md, CONTRIBUTING.md:20, recipes.md:15-24, ARCHITECTURE.md:249, GAPS.md:90, NEXT.md:42, structure.md:11,191, cleanup.md:65-75. Same how-to in three places (AGENTS, recipes, cleanup).
6. Layouts, themes, hub (6 docs): make-it-yours.md (how-to), ai/ui-layouts.md (format reference), design/customization-and-hub.md (design and plan), ai/custom-interface.md (front end over MCP), ARCHITECTURE.md:321, FEATURES "Make it yours". Plus CONTRIBUTING.md:38 pointer. ui-layouts and customization-and-hub overlap on the layout format.
7. AI driving the app (7 docs): ai/README.md, ai/mcp.md, ai/core.md (generated), ai/custom-interface.md, desktop-setup "Working with an AI agent locally", design/agent-navigation.md, design/ai-control.md; plus design/future-ideas "Token-efficient AI docs" and FEATURES "Scripts and AI". ai/README and ai/mcp both list the tools/commands.
8. Plugins (5 docs): ai/plugins.md (how-to), design/plugin-kinds.md (kinds), design/customization-and-hub §5 (plugins), ARCHITECTURE amb-plugin bullet, GAPS plugin row.
9. Lightroom trial and measuring (5 docs): lightroom-trial-plan.md (step by step), desktop-setup §3 and "Lightroom trial batches", design/calibration.md, GAPS §14 "Needs Lightroom measurement", release-notes "Not yet measured".
10. Film (4 docs): ai/film-recipes.md, design/film-negatives.md, design/film-look-measurement.md, FEATURES "Film negatives", plus research/16. ai/film-recipes and design/film-look-measurement both describe the looks.
11. Legal and names (5 docs): legal.md, SCOPE.md §6 and name, research/02 §3.4, CONTRIBUTING licence, .github PR template licence, README licence. Licence text repeated in four places.
12. Releases and installers (5 docs): releasing.md, desktop-setup §0, release-notes/v0.1.0.md, NEXT v0.1 checklist, README:9. SECURITY:20 says the same.
13. Codebase shape and cleanup (4 docs): design/structure.md (plan), cleanup.md (steps), design/agent-navigation.md (test rounds), ARCHITECTURE.md (as built). structure and cleanup both carry the plan; cleanup now done.
14. Research (18 docs): research/01-17 plus watchlist; overlap with SCOPE §3-5 and §13 index; watchlist duplicates research/17 table.

## Crate list mismatches

`ls crates`: amb-ai amb-cli amb-color amb-engine amb-export amb-gpu amb-gui amb-lens amb-library amb-ops amb-plugin amb-plugin-sdk amb-presets amb-protocol amb-testkit (15).

- AGENTS.md Layout: all 15 listed. None missing, none absent. OK.
- README.md code table: amb-ops missing (13 rows cover 14 crates; amb-plugin and amb-plugin-sdk share a row).
- docs/ARCHITECTURE.md crate table (13 rows): amb-plugin-sdk missing entirely. amb-plugin appears only in a bullet at line 345, not in the table. amb-ops, amb-color, amb-lens, amb-testkit present.
- docs/design/structure.md crates table: amb-testkit, amb-lens, amb-color present; not checked in full.
- UI folders: AGENTS.md lists core, looks, develop, tools, library, collage, dialogs, help. `ls crates/amb-gui/ui/` directories are exactly those 8. OK.
- Other UI files in `ui/` (not folders): ai-apps.json, basics.json, lessons.json, index.html, style.css, widgets.css, widgets-demo.html. Not mentioned in AGENTS.

## Counts

- Docs mapped: 70.
- Broken references (real, in-repo): 3 hits (docs/decisions/). Plus 20 cleanup-history and placeholder hits, which are expected.
- Stale statements: 13 listed (3 are "verify").
- Biggest overlap groups: roadmap/status (6 docs); "not yet"/open questions (14+); crate lists (4); checks and build commands (5); UI tests and Xvfb (7).
