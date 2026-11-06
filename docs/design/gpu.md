# GPU preview

The editor's preview is rendered at the size of the screen: about 2560 pixels wide on a HiDPI display, and 60% of that while a slider moves. On 4 CPU cores a 2005 × 1338 preview of a typical edit takes about 0.6 s, and a 1504 × 1004 one about 0.5 s (dehaze 134 ms, Texture/Clarity 117, colour 119, noise reduction 87, Highlights/Shadows 75). The GPU path runs the develop pipeline as compute shaders instead.

## How it works

- **`amb-gpu`** is the only crate that touches wgpu (version 26, the newest that builds with the declared Rust 1.85). It works on Metal, Vulkan, DirectX 12 and OpenGL.
- **`amb_engine::plan`** turns a photo and its settings into a `Plan`: camera RGB after stage 0 (Lens Corrections, Transform, crop, film negative; on the CPU, since it comes first), then the stages in the CPU pipeline's order as `Step`s. Their parameters (matrices, lookup tables, colour settings) come from the same code the CPU stages use.
- **Steps** run on the GPU, one or a few kernels each, on one image buffer that stays there. Every stage after stage 0 has its kernels now. `Step::Cpu` runs a stage without one: the image comes back, the engine's own stage runs on it, and it goes back up.
- **CPU steps inside the GPU preview:** a camera profile's tables and tone curve (its matrix runs on the GPU), defringe, spot removal, masks, a LUT and filling the edges a lens correction leaves run with the engine's own code between the GPU stages: masks are drawn once after spot removal, and each later mask stage (dehaze, tone, texture and colour, curves, detail, the overlay) runs the engine's own code right after its global stage. Every edit can be previewed on the GPU. 100% views stay on the CPU pipeline.
- **The editor** sets up the GPU in the background at start and uses it once ready. The status bar (off unless Settings → General turns it on) says "on the GPU", and its tooltip names the GPU, or why a preview was made on the CPU. A software GPU (llvmpipe, lavapipe, WARP) is skipped: it is slower than the CPU pipeline. `AMB_GPU=off` turns the GPU off; `AMB_GPU=software` allows a software one (for testing).

## Full-size exports

Export, batch export, collages, Convert raws and `amb render` develop at full size on the GPU too (`amb_gpu::full::render`), with the same plan as the preview. The photo goes to the CPU pipeline instead when:

- there is no GPU, or only a software one;
- an earlier full-size render failed on this GPU (out of memory or a lost device): wgpu reports those by panicking, which is caught.

A photo too large for the GPU at once is rendered **in tiles** (2026-11-02, `amb_gpu::full`): each tile is planned like the CPU's 1:1 regions (`amb_engine::plan::plan_region`): the tile plus a margin as wide as its filters reach, with the whole-photo measurements (haze, a negative's levels, colour ranges, sky) taken once from a small copy (`pipeline::measure_globals`). Spot removal, masks and Dehaze use the tile's position; vignette and grain get its origin (`Plan::origin`), so they line up across tiles. Tiles hold at most what fits: one binding (`Gpu::max_pixels`; colour noise reduction keeps 5 floats a pixel) and about 12 copies of the tile within the memory budget.

**The memory budget** (`Gpu::memory_budget`): `AMB_GPU_MEMORY` (MB) when set; else 80% of what the driver says is free ([`memory.rs`](../../crates/amb-gpu/src/memory.rs): Vulkan's `VK_EXT_memory_budget`, DXGI's video memory info on Direct3D 12, Metal's recommended working set); else 4 GB. On lavapipe (software Vulkan) the driver reports 15 GB free.

One full-size render runs at a time. The editor and its exports share one GPU (`amb_gpu::full::shared`). `amb render` and `amb convert` say "developed on the GPU (name)" when they used it; `amb convert --json` also says why the CPU. Tests: `a_full_size_render_matches_the_cpu` (whole), `a_large_photo_renders_in_tiles_like_the_cpu` and `tiles_share_the_whole_photo_s_haze` (about 40 and 12 tiles, within 2e-6 of the CPU). On the 24 MP D750 raw in tiles on lavapipe: mean difference 0.005 of 255, 0.008% of pixels more than 2 apart, scattered (noise reduction measures the noise per tile, as the CPU's regions do), no seams. Contact sheets develop small photos on the CPU, several at once.

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
- Speed has not been measured on a real GPU yet: this was built on a machine without one. Measure on the Mac (Apple Silicon): the status bar (Settings → General) shows each preview's time, and `AMB_GPU=off` gives the CPU's for comparison.
- Every stage runs on the GPU now, with a few small trips back: the dehaze estimate (when Dehaze is set), colour noise reduction's guided filter and noise measurement (a 1/16 copy and the tile sums), and luminance noise reduction's noise measurement (one set of sums per 16-pixel tile). Each trip waits for the GPU to finish what came before it.
- Luminance noise reduction: the GPU makes the centred encoded luminance and the Haar power of each 16-pixel tile, the CPU takes the noise percentiles per luminance band and returns the noise and eps tables (`detail::luminance_nr_tables`), and the GPU runs the two self-guided filters, Detail's soft threshold, Contrast's blur and the luminance ratio.

## AI models (2026-11-06)

`amb-ai`'s models run on the GPU too, with our own kernels (`src/nn.rs`, `shaders/nn.wgsl`), not a vendor runtime:

- **The graph:** `amb-ai/src/net.rs` loads a model with tract, simplifies it (tract's "decluttered" form) and walks it layer by layer. A layer with a kernel runs on the GPU; any other (shapes, integer maths, a cast from bytes) runs with tract's own code on the CPU, its inputs read back and its outputs sent again. Weights are sent once and kept.
- **Which models:** only if every convolution and matrix product can run on the GPU, and only if the model has at least 2·10⁸ multiply-adds (smaller ones are quicker on the CPU). A run that fails (a wgpu error included) runs on the CPU, and the model stays there.
- **Kernels:** convolution as a matrix product reading the input in place (64 × 64 outputs a workgroup, 4 × 4 a thread, each thread reading its own rows and columns: tiles shared in workgroup memory crashed Windows on ARM's software GPU, so they wait for a real GPU to try), a direct one for depthwise convolutions, transposed convolution, matrix products by strides (up to three batch axes; operands copied into order only when their axes don't line up), element-wise maths with broadcasting, a strided copy for transposes, slices, broadcasts, tiles and nearest upsampling, padding (constant, reflect, edge), reductions, softmax, RMS norm, max pooling and gather. All f32.
- **Order:** work is recorded in one command list and sent every 64 kernels. A queue write lands before work not yet sent, so the session sends its work before each upload (a reused buffer may still be read).
- **Checks:** `net.rs`'s tests build a small network and compare it with tract; `AMB_NET=model.onnx AMB_NET_SHAPE=… cargo test -p amb-ai --release -- --ignored net` compares a real model (on lavapipe: YuNet 3e-7, SAM 2.1 tiny 8e-6, Big-LaMa 9e-7, MI-GAN one level in 255 from rounding to bytes). `AMB_NET_CHECK=1` compares every layer and names the first that differs.
- **Not yet:** speed on a real GPU is unmeasured; no fp16; a tensor over the GPU's largest buffer binding runs its layer on the CPU (lavapipe: 128 MB, which SAM's attention passes).

