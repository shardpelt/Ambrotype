# What's next

A short running list, kept up to date as work lands. Newest decisions first. The full roadmap and its tracks are in [NEXT.md](NEXT.md); what's missing is in [GAPS.md](GAPS.md).

## Waiting on the owner

1. **v0.1.0:** try the editor on a Mac and once on Windows (the list is in NEXT.md's v0.1 checklist), then say go: tag, check the draft release, publish, and point the README at it.
2. **Auto Density and Auto Grade on real negatives:** try both switches (Negative panel, beside Auto) on a roll. If they look right, they become Auto's default.
3. **GPU speed:** export a 24 MP raw on the Mac with and without `AMB_GPU=off`; that number decides whether tiling for large photos comes next.
4. **Your sample raw** to replace the stand-in sample photo, after the release.

## Up next (in this order)

1. **Paper-like highlight roll-off** for film looks (Film lab item 6), then **a paper curve in density** (item 9).
2. **Film looks tied to stocks:** halation, bloom and grain amounts per film stock, so picking a stock brings its glow and grain.
3. **GPU exports in tiles,** so photos over about 30 MP use the GPU too, and **ask the GPU its free memory** (Vulkan, DXGI, Metal) as CalibRaw does, instead of a fixed 4 GB ([design/gpu.md](design/gpu.md), [research/17](research/17-competitors-2026.md)).
4. **Convert raws on more cameras:** try Sony, Canon and Fujifilm raws (only a Nikon D750 so far).
5. **Dust:** the guided rectangle and a heal brush that searches its stroke (Film lab item 5).
6. **Darkroom aids:** density and zone under the cursor, test strip, ring-around (Film lab item 7).
7. **From the competition:** open a Lightroom catalogue in place; pick the best in a burst by open eyes ([research/17](research/17-competitors-2026.md)).

## Recently done

- 2026-10-31: grain that follows density (Effects → Grain → Film response)
- 2026-10-31: filmstrip multi-select: Ctrl/Shift-click, right-click for preset, paste, export, convert
- 2026-10-31: Auto Density and Auto Grade for negatives (off by default)
- 2026-10-31: subfolders as the first tiles of the library grid; double-click opens or closes a tree folder
- 2026-10-31: Convert raws from the editor; New preset from this photo (Ctrl+Shift+N)
- 2026-10-31: halation and bloom (Effects → Glow)
- 2026-10-31: hub CI installs every submission with the app's own checks (`ambrotype --check-hub`)
- 2026-10-31: Convert raws: three at a time, best of each burst
- 2026-10-31: History values, "exposure 0.7" in Search, auto-advance after rating
- 2026-10-31: full-size exports on the GPU, with the CPU as fallback
