# GPU preview

The editor's preview is rendered at the size of the screen: about 2560 pixels wide on a HiDPI display, and 60% of that while a slider moves. On 4 CPU cores a 2005 × 1338 preview of a typical edit takes about 0.6 s, and a 1504 × 1004 one about 0.5 s (dehaze 134 ms, Texture/Clarity 117, colour 119, noise reduction 87, Highlights/Shadows 75). The GPU path runs the develop pipeline as compute shaders instead.

## How it works

- **`amb-gpu`** is the only crate that touches wgpu (version 26, the newest that builds with the declared Rust 1.85). It works on Metal, Vulkan, DirectX 12 and OpenGL.
- **`amb_engine::plan`** turns a photo and its settings into a `Plan`: camera RGB after stage 0 (Lens Corrections, Transform, crop, film negative; on the CPU, since it comes first), then the stages in the CPU pipeline's order as `Step`s. Their parameters (matrices, lookup tables, colour settings) come from the same code the CPU stages use.
- **Steps** run on the GPU, one or a few kernels each, on one image buffer that stays there. Every stage after stage 0 has its kernels now. `Step::Cpu` remains for a future stage without one: the image comes back, the engine's own stage runs on it, and it goes back up.
- **On the CPU only:** masks, spot removal, defringe and camera profiles (they keep state across several stages). An edit with any of them is previewed on the CPU. So are 100% views and exports, which stay on the CPU pipeline.
- **The editor** sets up the GPU in the background at start and uses it once ready. The status bar says "on the GPU", and its tooltip names the GPU, or why a preview was made on the CPU. A software GPU (llvmpipe, lavapipe, WARP) is skipped: it is slower than the CPU pipeline. `AMB_GPU=off` turns the GPU off; `AMB_GPU=software` allows a software one (for testing).

## Stages

| Stage | GPU |
|---|---|
| Camera matrix, white balance, Exposure, highlight recovery, Calibration shadow tint | yes |
| Dehaze | yes (the estimate is measured on the CPU, on a downscaled copy the GPU makes) |
| Base look | yes |
| Highlights and Shadows | yes |
| Tone curve (Contrast, Whites, Blacks, ...) | yes |
| Texture and Clarity | yes |
| Colour (Vibrance, Saturation, HSL, black and white, Color Grading) | yes |
| Parametric and point curves | yes |
| Colour noise reduction | yes (the low-resolution guided filter and the noise measurement on the CPU, from a downscaled copy and tile sums the GPU makes) |
| Luminance noise reduction | yes (the noise measurement on the CPU, from tile sums the GPU makes) |
| Sharpening (with Masking) | yes |
| Post-crop vignette (all three styles) | yes |
| Grain | yes (the same noise as the CPU's, bit for bit: its 64-bit hash is done on pairs of 32-bit integers) |

Highlights/Shadows and Texture/Clarity are built from plane filters that mirror `amb_engine::filters` and `local`: Gaussian blurs (an exact kernel up to sigma 3, three box blurs above), fractional box blurs, block downsampling and the fast guided filter's coefficients. The engine hands over their numbers (`filters::gaussian_plan`, `local::GuidedPlan`, `local::plan`, `local_tone::plan`), computed by the functions its own stages use. Where the CPU keeps running sums along a row, each GPU thread sums its own window, so the two differ only in rounding.

## Checks

Each test renders the same synthetic raw (gradients, edges, fine texture, saturated colours and a clipped patch) through the CPU pipeline and through the GPU path, and compares every pixel. On lavapipe the largest difference is 8e-6 for a single stage, and 5e-5 for a full edit. The tests allow 2e-3, for real GPUs' faster maths functions. CI's Linux job installs lavapipe so the tests run there. On a machine with no GPU at all they are skipped.

The editor's preview of the same edit was also compared both ways in the editor, with lavapipe: the two screenshots are identical.

## Open questions

- The colour noise reduction split: the GPU downscales the image by 4, the CPU runs the guided filter on that copy and returns its coefficients, the GPU computes each pixel's chroma residual and the sums of each 16-pixel tile (one thread per tile, adding in the CPU's order), the CPU takes the noise percentiles per luminance band from those sums and returns the garrote's threshold table, and the GPU keeps or removes each residual.
- Speed has not been measured on a real GPU yet: this was built on a machine without one. Measure on the Mac (Apple Silicon): the status bar shows each preview's time, and `AMB_GPU=off` gives the CPU's for comparison.
- Every stage runs on the GPU now, with a few small trips back: the dehaze estimate (when Dehaze is set), colour noise reduction's guided filter and noise measurement (a 1/16 copy and the tile sums), and luminance noise reduction's noise measurement (one set of sums per 16-pixel tile). Each trip waits for the GPU to finish what came before it.
- Luminance noise reduction: the GPU makes the centred encoded luminance and the Haar power of each 16-pixel tile, the CPU takes the noise percentiles per luminance band and returns the noise and eps tables (`detail::luminance_nr_tables`), and the GPU runs the two self-guided filters, Detail's soft threshold, Contrast's blur and the luminance ratio.
