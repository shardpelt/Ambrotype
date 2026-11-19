// SPDX-License-Identifier: GPL-3.0-or-later
//! The compiled compute shaders the operations run on.

use crate::Gpu;
use std::collections::HashMap;

/// The compiled kernels, all of one layout.
pub(crate) struct Kernels {
    pub(super) layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    /// Made when first used: most models need only a few.
    pipelines: std::sync::Mutex<HashMap<&'static str, wgpu::ComputePipeline>>,
    /// Bound where a kernel takes fewer inputs.
    pub(super) empty: wgpu::Buffer,
}

const ENTRIES: [&str; 14] = [
    "copy",
    "fill",
    "binary",
    "unary",
    "matmul",
    "conv",
    "conv_direct",
    "deconv",
    "pad",
    "reduce",
    "softmax",
    "rms_norm",
    "max_pool",
    "gather",
];

impl Kernels {
    pub(crate) fn new(gpu: &Gpu) -> Kernels {
        let buffer = |i: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding: i,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let entries = [
            buffer(0, false),
            buffer(1, true),
            buffer(2, true),
            buffer(3, true),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nn"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nn"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nn"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/nn.wgsl").into()),
        });
        let pipelines = std::sync::Mutex::new(HashMap::new());
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Kernels {
            layout,
            pipeline_layout,
            module,
            pipelines,
            empty,
        }
    }

    pub(super) fn pipeline(&self, gpu: &Gpu, entry: &str) -> wgpu::ComputePipeline {
        let name = ENTRIES
            .iter()
            .find(|e| **e == entry)
            .expect("a kernel in nn.wgsl");
        let mut made = self.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        made.entry(name)
            .or_insert_with(|| {
                gpu.device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(name),
                        layout: Some(&self.pipeline_layout),
                        module: &self.module,
                        entry_point: Some(name),
                        compilation_options: Default::default(),
                        cache: None,
                    })
            })
            .clone()
    }
}
