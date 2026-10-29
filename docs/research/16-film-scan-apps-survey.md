# Film scan apps: negaflow and NegPy (survey)

Snapshot taken 2026-10-28:
- [negaflow](https://github.com/habinsong/negaflow) at `96991e3`, dated 2026-10-27
- [NegPy](https://github.com/marcinz606/NegPy) at `6227fee`, dated 2026-10-26

Method: I read each project's README, its docs (negaflow's `docs/product` and `docs/reference`; NegPy's `docs/PIPELINE.md` and `docs/USER_GUIDE.md`), its NOTICE file and negaflow's screenshots. I did not read their source code; it was used only to count files. Everything below comes from their docs, so treat each claim as theirs.

The owner pointed at both on 2026-10-26. The question was which of their ideas we can build ourselves, without contacting their authors. The answer, and the backlog it led to, is in §3 and §6. The track itself is in [NEXT.md](../NEXT.md) under "Film lab track".

## 1. Project facts

| | negaflow | NegPy |
|---|---|---|
| Licence | Apache-2.0 | GPL-3.0 |
| Author | Song Habin, solo | GitHub user marcinz606, with contributors |
| Stack | macOS: Swift with Core Image (about 1,200 files). Windows: C# UI over a C++ core with Direct3D (about 1,500 files) | Python with a PyQt6 UI (about 1,000 files), a Rust core (`negpy_core`, 90 files) and wgpu shaders (33) |
| Platforms | macOS, Windows | Linux, macOS, Windows |
| Raw decoding | LibRaw (CDDL) | LibRaw through rawpy |
| Where edits live | The app's catalogue, with sidecars it manages | A SQLite database keyed by file content (files can move), optional `.negpy` sidecars |
| Third-party material | LibRaw; a test raw from raw.pixls.us (CC0); FILM-R v2 dataset references (CC BY 4.0) | Dye curves from spektrafilm (CC BY-SA 4.0); DX edge codes from The Big Film Database (CC BY-SA 4.0); print tables made from Fujifilm printer software and profiles (§4); IR dust ideas from digital-fauxice (MIT) |

## 2. How they compare with Ambrotype

Ambrotype is a general raw editor in which negatives are one panel. Both of these are film-scan tools first.

| Area | negaflow | NegPy | Ambrotype now |
|---|---|---|---|
| Inversion | Density from a measured film base, then a print curve. Targets: MAIN, PRINT, minilab styles, EXPIRED | Per-channel log bounds and stretch, then a paper curve in density (toe, straight line, shoulder), graded in ISO-R points | Density from a film base, levels per channel, a stock look, then our own curve with toe and shoulder ([design/film-negatives.md](../design/film-negatives.md)) |
| Film base | Found as a wide bright band along the frame edge, with several strips handled together; an eyedropper; typed RGB; a per-stock default when it can't be measured | Read from the rebate; click the clear film between frames to set the roll's base | The median of the brightest 0.3 % of the picture |
| Auto | One Auto Balance switch. Off uses a fixed range (colour 1.55 D, B&W 2.17 D), so equal film gives equal prints | Auto Density and Auto Grade, metered only on parts with detail, with holds for highlights and shadows | One Auto for base, levels and midtone. Off uses stored values |
| Colour cast | Levels per channel | Cast Removal: balances each layer against the frame's greys, from shadows to highlights | Assumes the deepest shadows and brightest highlights are neutral; fails on sunsets ([GAPS.md](../GAPS.md) §9) |
| Rolls | Develop a whole folder with one process and target | Roll baseline (odd frames keep their own), scenes grouped by light, roll defaults with per-frame overrides, Lock Bounds | Freeze one frame; Sync negative roll (one median for the selection) |
| Scan corrections | None listed | Flat-field from a shot of the bare light; channel bleed between dyes, light and sensor (3 × 3, per film type or measured from a colour chart shot on film); narrowband light; three-shot RGB merge | Lens corrections only |
| Frames | Finds frames on a flatbed preview by grain texture and fits an even spacing | Half-frame split, stitching a large negative, HDR for slides | Crop to frame (finds one frame's edges); panorama and HDR merges for raws |
| Dust and scratches | Automatic, a guided rectangle, brush, clone, infrared. Checks that wires and window frames aren't scratches. Repairs are layers | Automatic (spots and hairs), infrared, heal strokes, a line tool for straight scratches, clone. Keeps the grain | Heal and clone spots by hand |
| Darkroom aids | Soft proof, clipping overlay | Densitometer under the cursor, zone overlay, placing a spot in a zone, step wedge, test strips, ring-around, Pick Better (choose between two versions), peeks at the negative | Original toggle; zones highlighted when teaching |
| Film look on digital photos | Halation in linear light, a density curve with dye-layer interaction, paper roll-off, grain that follows density | Glow and halation (red-weighted), lab-style chroma denoise and skin protection | Film recipe looks (colour and tone only) |
| Black and white | Print styles: cyanotype, glass plate, gelatin silver | Lith, cyanotype, Sabattier; chemical toners (selenium, sepia, gold and others); edge burn; filed carrier | B&W mixer, Color Grading, duotone effect |
| Print and output | Contact sheet, picture package, custom package, paper sizes, rulers, printer ICC profile applied to the whole page, lab-print proof | Contact sheet as film strips (edge print, roll label, straight proof), soft proof with paper white and ink black, a flat 16-bit master, a linear export | Collage pages and borders; soft proof for export colour spaces only |
| Capture | Scanner plugin (SANE, GPL-2.0+) | Plustek, Coolscan, SANE; tethered camera scanning | None |
| Library | Folders, film-type filter, drag photos between folders | Rolls (from folders or any frames), keep/reject, search terms such as `film:portra iso:>=400`, a hot folder, gear and film metadata | Folders, collections, smart collections, timeline, map, duplicates, flags, ratings |

Where Ambrotype is ahead:
- Lightroom-faithful general editing, and reading Lightroom presets and sidecars
- library depth
- local AI masks
- plugins, the hub, and customisable layouts and themes
- the AI layer (MCP, assistant)

## 3. What we can build without contacting anyone

Ideas and methods are not protected by copyright; code, text and data are. So we can rebuild any idea in §6 in our own code, from:
- their public docs
- published sources: film and paper datasheets, papers, expired patents

Neither author needs to be contacted. We credit the idea in the design doc for each feature, as we do for darktable's `negadoctor`.

Copying would also be allowed:
- NegPy's code is GPL-3, the same as ours.
- negaflow's Apache-2.0 code can go into a GPL-3 project.
- Either needs a credit in the file and in `NOTICE`.

Our own rule still applies: no third-party code or data comes in without the owner's OK ([AGENTS.md](../../AGENTS.md), CLAUDE.md). None of §6 needs it.

## 4. What we don't take

- **NegPy's Frontier and Crystal Archive print tables.** NegPy made them from Fujifilm's Frontier printer software (`ColorData.dat`) and from Fujifilm's printer profiles. Treat them like Adobe's data: not ours to ship ([legal.md](../legal.md)).
- **negaflow's lab-scanner profiles.** Its HS and SP targets are statistics from its author's own scans, sorted by lab scanner and named after the machines. Two reasons not to take them:
  - They are data, not ideas, so taking them would need the owner's OK and a credit.
  - Imitating a named machine needs trademark care.

  Lab-style looks of our own with generic names (as negaflow's F135 and HR are) are fine.
- **Third-party data that NegPy carries.** spektrafilm's dye curves and the DX edge-code database are CC BY-SA 4.0. That licence is allowed into GPL-3 with a credit, but each is a separate decision for the owner. Numbers we read ourselves from a public datasheet are facts; cite the datasheet.
- **Trade names.** Digital ICE, iSRD, SRDx, HyperTone, Frontier, Portra and similar appear only to say what something is not. Our features get plain names (Dust and scratches, not "ICE").
- **Scanner drivers.** negaflow-scanner-sane, pyopticfilm and SANE's genesys backend are separate projects. Capture stays in Phase 4 ([SCOPE.md](../SCOPE.md)).

The FILM-R v2 dataset (Daniela Ivanova, CC BY 4.0) is 44 damaged scans with expert repairs. It can be our dust-repair test bench:
- a script downloads it
- it is never committed
- the bench doc credits it

## 5. Patents

NegPy cites five patents for its metering and lab tools:

| Patent | Used for | Granted | Status |
|---|---|---|---|
| Agfa, US 4,104,069 | Highlight hold | 1978 | Expired |
| Alkofer, US 4,731,671 | Contrast matching (Auto Grade) | 1988 | Expired |
| Boyack & Juenger, US 5,724,456 | Brightness from detailed areas (Auto Density) | 1998 | Expired |
| Fujifilm, US 5,739,922 | Chroma denoise | 1998 | Expired |
| Gindele, US 7,113,649 | Shadow reach | 2006, filed 2002-06-24 | Its 20-year term ran out in 2022; check for a term adjustment before building |

## 6. Ideas for Ambrotype (the backlog)

Each idea is rebuilt from the description, in our pipeline and our `amb:` namespace. Sizes: S is a day or two, M is a week, L is more.

**Conversion**
1. **Fixed range when Auto is off** (negaflow). A nominal density range per film type, so a roll converts consistently without Freeze. S.
2. **Better film base** (both):
   - an eyedropper on the rebate
   - a base found as a band along the frame edge, ignoring the holder and light leaks
   - several strips in one scan
   - a stored roll base

   M.
3. **Cast removal along the greys** (NegPy; built 2026-10-29 as `amb:NegativeCastRemoval`). Fit red and blue to green using the frame's least-saturated pixels in shadows, midtones and highlights, with a strength slider. Fixes the sunset case in GAPS §9. M.
4. **Separate Auto Density and Auto Grade** (NegPy):
   - metered only on parts of the frame with detail
   - pulled only part-way toward a typical value
   - with holds for highlights and shadows

   Replaces model v1's fixed percentiles. M.
5. **Roll baseline** (NegPy). Builds on Sync negative roll:
   - frames far from the roll's median keep their own analysis
   - Use this frame as the baseline
   - Lock bounds
   - scenes: frames grouped by light, each with its own baseline

   M.
6. **Metering region and border** (NegPy): draw the area to analyse; ignore a border for the holder. S.
7. **A paper curve in density** (NegPy): toe, straight line and shoulder; contrast in ISO-R grades; split grade; preflash. B&W papers fitted by us from public datasheets. Candidate for model v2. L.
8. **Expired-film target** (negaflow): limited correction for faded, fogged film. S.

**Dust and scratches**

9. **Dust and scratch repair** (both). Fills GAPS §9's "no dust and scratch removal". L. It would bring:
   - an automatic pass across the whole frame
   - a guided rectangle that looks harder
   - a heal brush that searches its stroke
   - a line tool for straight scratches, with no automatic detection, since a long line is as likely a horizon
   - an infrared path, where a scan has a fourth channel or an `_ir` file
   - a check that keeps wires, railings and lettering
   - grain carried across the repair
   - repairs as an ordered list of layers in the sidecar, each with an on switch and a strength
   - a test bench on FILM-R (§4)

**Scans in**

10. **Flat-field correction** (NegPy): a profile from a shot of the bare light source, applied per roll. M.
11. **Channel bleed correction** (NegPy): a 3 × 3 matrix per film type, or measured from a colour chart shot on the film. M.
12. **Frames from a strip or sheet** (both):
    - half-frame split with the gap found automatically
    - a strip shot in one go split into frames, found by grain texture with an even spacing
    - stitching a large negative (reuses panorama merge)

    M.

**Darkroom aids**

13. **Readouts and probes** (NegPy):
    - density and zone under the cursor
    - zone overlay
    - place a spot in a zone
    - a step wedge
    - the negative's density range
    - clipping at the scan

    M.
14. **Test strip and ring-around** (NegPy): a strip of exposures or a grid of colour shifts to click on. M.
15. **Pick Better** (NegPy): two versions beside the edit; click the better one; each round tunes one setting. Fits our teaching track. M.
16. **Peeks** (NegPy): hold a key to see the negative as scanned. S.

**Looks**

17. **Film look on digital photos** (both). Fills GAPS §9's halation and grain row. M:
    - halation in linear light, red-weighted, before density
    - grain that follows density
    - a highlight roll-off like paper
18. **Black-and-white processes** (NegPy): lith, cyanotype, Sabattier, and chemical toners. They could ship as effect plugins. M.
19. **Lab-scanner-style finishing** (NegPy): chroma denoise that keeps edges, a chroma ceiling for skin, saturation that stays in gamut. Generic names. S each.
20. **Edge burn and filed carrier** (NegPy) as borders or effects. S.

**Print and output**

21. **Contact sheet** (both):
    - the roll as film strips, with edge print, roll label and order by scene
    - a straight proof at one exposure
    - black, grey or white paper

    Builds on collage pages. M.
22. **Print layouts** (negaflow): single image, picture package, custom package, paper sizes, rulers. Overlaps Phase 4 print. M.
23. **Printer and paper soft proof** (both): a printer ICC profile applied once to the composed page; paper white and ink black. Fills the soft-proof row in GAPS. M.
24. **Flat master and linear export** (NegPy): a 16-bit flat (log) TIFF for other editors. S.

**Library**

25. **Film metadata and search terms** (NegPy):
    - stock, camera, lens and development per roll, with saved gear
    - search terms such as `film:… iso:>=400 -rejected:`
    - a hot folder that loads files as a scanner writes them

    M.

We already have negaflow's high-quality JPEG setting (no chroma subsampling at high quality), conversion on camera RGB before the colour matrix, Copy and Paste settings by group, HDR merge and panorama merge.
