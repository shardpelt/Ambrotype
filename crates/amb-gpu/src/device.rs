// SPDX-License-Identifier: GPL-3.0-or-later
//! The GPU device, image buffers and running a kernel.

use wgpu::util::DeviceExt;

/// A GPU ready for compute work.
pub struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    kernels: std::sync::OnceLock<crate::develop::Kernels>,
}

impl Gpu {
    /// The first suitable GPU, or `None`. A software rasteriser (llvmpipe,
    /// lavapipe, WARP) is taken only when `software` is set or `AMB_GPU` is
    /// `software`: it works, but is slower than the CPU pipeline.
    /// `AMB_GPU=off` turns the GPU off.
    pub fn new(software: bool) -> Option<Gpu> {
        let env = std::env::var("AMB_GPU").unwrap_or_default();
        if env == "off" || env == "0" {
            return None;
        }
        let software = software || env == "software";
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu && !software {
            return None;
        }
        // As large as the adapter allows: a preview image is one buffer, and
        // a kernel binds up to 8 storage buffers.
        let limits = adapter.limits();
        if limits.max_storage_buffers_per_shader_stage < 8 {
            return None;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("amb-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .ok()?;
        Some(Gpu {
            device,
            queue,
            info,
            kernels: std::sync::OnceLock::new(),
        })
    }

    /// The adapter's name and backend, e.g. "Apple M2 (Metal)".
    pub fn name(&self) -> String {
        format!("{} ({:?})", self.info.name, self.info.backend)
    }

    /// The largest image (in pixels) one buffer can hold.
    pub fn max_pixels(&self) -> usize {
        let limits = self.device.limits();
        let bytes = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size.min(u32::MAX as u64) as u32);
        bytes as usize / (3 * 4)
    }

    pub(crate) fn upload(&self, data: &[f32]) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pixels"),
                contents: bytemuck::cast_slice(data),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            })
    }

    /// A read-only storage buffer holding `value` (stage parameters).
    pub(crate) fn storage<T: bytemuck::Pod>(&self, value: &T) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("params"),
                contents: bytemuck::bytes_of(value),
                usage: wgpu::BufferUsages::STORAGE,
            })
    }

    /// A zeroed storage buffer of `len` floats.
    pub(crate) fn buffer(&self, len: usize) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scratch"),
            size: (len.max(1) * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// The compiled kernels (compiled on first use).
    pub(crate) fn kernels(&self) -> &crate::develop::Kernels {
        self.kernels
            .get_or_init(|| crate::develop::Kernels::new(self))
    }

    /// Compile the kernels now, so the first render doesn't wait for them.
    pub fn warm_up(&self) {
        self.kernels();
    }

    /// Read a buffer of `len` floats back to the CPU.
    pub(crate) fn download(&self, buffer: &wgpu::Buffer, len: usize) -> Vec<f32> {
        let size = (len * 4) as u64;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
        self.queue.submit([encoder.finish()]);
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait)
            .expect("the GPU stopped responding");
        let out = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
        staging.unmap();
        out
    }
}

/// A compiled compute kernel: one WGSL entry point. Every entry point of a
/// shader file shares its layout: storage buffers at bindings 0, 1, ...,
/// each read-only or read-write.
pub(crate) struct Kernel {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

/// Threads per workgroup in every kernel (a flat 1-D dispatch).
pub(crate) const GROUP: u32 = 256;

impl Kernel {
    /// `read_only[i]`: whether binding `i` is read-only.
    pub(crate) fn new(
        gpu: &Gpu,
        label: &str,
        source: &str,
        entry: &str,
        read_only: &[bool],
    ) -> Kernel {
        let entries: Vec<wgpu::BindGroupLayoutEntry> = read_only
            .iter()
            .enumerate()
            .map(|(i, ro)| wgpu::BindGroupLayoutEntry {
                binding: i as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: *ro },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &entries,
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
        Kernel { pipeline, layout }
    }

    /// Record a dispatch over `items` threads, with `buffers` bound at 0, 1, ...
    pub(crate) fn dispatch(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        buffers: &[&wgpu::Buffer],
        items: usize,
    ) {
        let entries: Vec<wgpu::BindGroupEntry> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        let groups = (items as u32).div_ceil(GROUP);
        // Large dispatches wrap into a second dimension (65535 groups per axis).
        let x = groups.min(65535);
        pass.dispatch_workgroups(x, groups.div_ceil(x), 1);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A device for one test. Tests take turns: several devices at once
    /// crash Direct3D 12 on Windows ARM.
    pub(crate) struct TestGpu {
        gpu: Gpu,
        _turn: std::sync::MutexGuard<'static, ()>,
    }

    impl std::ops::Deref for TestGpu {
        type Target = Gpu;
        fn deref(&self) -> &Gpu {
            &self.gpu
        }
    }

    /// A software GPU (lavapipe or llvmpipe) is enough for the tests; with
    /// none at all, they are skipped with a note.
    pub(crate) fn gpu() -> Option<TestGpu> {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let Some(gpu) = Gpu::new(true) else {
            eprintln!("no GPU adapter (install mesa-vulkan-drivers for lavapipe): skipped");
            return None;
        };
        Some(TestGpu { gpu, _turn: turn })
    }

    #[test]
    fn runs_a_kernel_and_reads_it_back() {
        let Some(gpu) = gpu() else { return };
        eprintln!("GPU: {}", gpu.name());
        const SRC: &str = "
            @group(0) @binding(0) var<storage, read_write> px: array<f32>;
            struct P { gain: f32, n: u32 }
            @group(0) @binding(1) var<storage, read> p: P;
            @compute @workgroup_size(256)
            fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
                let i = id.x + id.y * n.x * 256u;
                if (i >= p.n) { return; }
                px[i] = px[i] * p.gain;
            }";
        let k = Kernel::new(&gpu, "gain", SRC, "main", &[false, true]);
        let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let buf = gpu.upload(&data);
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct P {
            gain: f32,
            n: u32,
        }
        let params = gpu.storage(&P { gain: 2.5, n: 1000 });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        k.dispatch(&gpu, &mut enc, &[&buf, &params], 1000);
        gpu.queue.submit([enc.finish()]);
        let out = gpu.download(&buf, 1000);
        assert_eq!(out[999], 999.0 * 2.5);
        assert_eq!(out[1], 2.5);
    }
}
