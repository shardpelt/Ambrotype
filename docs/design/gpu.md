# GPU preview

The editor's preview is rendered at the size of the screen: about 2560 pixels wide on a HiDPI display, and 60% of that while a slider moves. On 4 CPU cores a 2005 × 1338 preview of a typical edit takes about 0.6 s, and a 1504 × 1004 one about 0.5 s (dehaze 134 ms, Texture/Clarity 117, colour 119, noise reduction 87, Highlights/Shadows 75). The GPU path runs the develop pipeline as compute shaders instead.

## How it works

- **`amb-gpu`** is the only crate that touches wgpu (version 26, the newest that builds with the declared Rust 1.85). It works on Metal, Vulkan, DirectX 12 and OpenGL.
- **`amb_engine::plan`** turns a photo and its settings into a `Plan`: camera RGB after stage 0 (Lens Corrections, Transform, crop, film negative; on the CPU, since it comes first), then the stages in the CPU pipeline's order as `Step`s. Their parameters (matrices, lookup tables, colour settings) come from the same code the CPU stages use.
- **Steps** with a GPU kernel run on the GPU, one kernel each, on one image buffer that stays there. A step without a kernel yet is a `Step::Cpu`: the image comes back, the engine's own stage runs on it, and it goes back up. So every edit is right by construction, and each stage ported to the GPU makes previews faster. On Apple Silicon the GPU shares the CPU's memory, so the trips are cheap.
- **On the CPU only:** masks, spot removal, defringe and camera profiles (they keep state across several stages). An edit with any of them is previewed on the CPU. So are 100% views and exports, which stay on the CPU pipeline.
- **The editor** sets up the GPU in the background at start and uses it once ready. The status bar says "on the GPU", and its tooltip names the GPU, or why a preview was made on the CPU. A software GPU (llvmpipe, lavapipe, WARP) is skipped: it is slower than the CPU pipeline. `AMB_GPU=off` turns the GPU off; `AMB_GPU=software` allows a software one (for testing).

## Stages

| Stage | GPU |
|---|---|
| Camera matrix, white balance, Exposure, highlight recovery, Calibration shadow tint | yes |
| Dehaze | not yet (CPU step) |
| Base look | yes |
| Highlights and Shadows | yes |
| Tone curve (Contrast, Whites, Blacks, ...) | yes |
| Texture and Clarity | yes |
| Colour (Vibrance, Saturation, HSL, black and white, Color Grading) | yes |
| Parametric and point curves | yes |
| Noise reduction | not yet (CPU step) |
| Sharpening (with Masking) | yes |
| Post-crop vignette (all three styles) | yes |
| Grain | not yet (CPU step) |

Highlights/Shadows and Texture/Clarity are built from plane filters that mirror `amb_engine::filters` and `local`: Gaussian blurs (an exact kernel up to sigma 3, three box blurs above), fractional box blurs, block downsampling and the fast guided filter's coefficients. The engine hands over their numbers (`filters::gaussian_plan`, `local::GuidedPlan`, `local::plan`, `local_tone::plan`), computed by the functions its own stages use. Where the CPU keeps running sums along a row, each GPU thread sums its own window, so the two differ only in rounding.

## Checks

Each test renders the same synthetic raw (gradients, edges, fine texture, saturated colours and a clipped patch) through the CPU pipeline and through the GPU path, and compares every pixel. On lavapipe the largest difference is 7e-6 for the GPU stages, and 5e-5 for a full edit that also runs every CPU step. The tests allow 2e-3, for real GPUs' faster maths functions. CI's Linux job installs lavapipe so the tests run there. On a machine with no GPU at all they are skipped.

The editor's preview of the same edit was also compared both ways in the editor, with lavapipe: the two screenshots are identical.

## Open questions

- Speed has not been measured on a real GPU yet: this was built on a machine without one. Measure on the Mac (Apple Silicon): the status bar shows each preview's time, and `AMB_GPU=off` gives the CPU's for comparison.
- Which stages to port next, by time: Dehaze, noise reduction and grain. Colour noise reduction measures a noise profile over the whole image (tile statistics and percentiles); on the GPU, the tiles would be summed there and the percentiles taken on the CPU.
