use std::sync::mpsc;

use half::f16;
use wgpu::{BufferUsages, util::DeviceExt};

#[repr(C)]
#[derive(Copy, Clone)]
struct PickParams {
    uv: [f32; 2],
    _pad: [f32; 2],
}

fn pick_params_as_bytes(params: &PickParams) -> &[u8] {
    // Reinterpret packed uniform as raw bytes for queue upload.
    unsafe {
        core::slice::from_raw_parts(
            (params as *const PickParams).cast::<u8>(),
            core::mem::size_of::<PickParams>(),
        )
    }
}

pub struct GpuPicker {
    pick_params_buffer: wgpu::Buffer,
    pick_output_buffer: wgpu::Buffer,
    pick_readback_buffer: wgpu::Buffer,
    pick_bind_group: wgpu::BindGroup,
    pick_pipeline: wgpu::ComputePipeline,
}

impl GpuPicker {
    pub fn new(
        device: &wgpu::Device,
        r_view: &wgpu::TextureView,
        g_view: &wgpu::TextureView,
        b_view: &wgpu::TextureView,
    ) -> Self {
        let pick_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pick-params-buffer"),
            contents: pick_params_as_bytes(&PickParams {
                uv: [0.0, 0.0],
                _pad: [0.0, 0.0],
            }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let pick_output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick-output-buffer"),
            size: 16,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pick_readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick-readback-buffer"),
            size: 16,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let pick_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("pick-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pick_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let pick_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pick-bind-group"),
            layout: &pick_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(r_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(g_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(b_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pick_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: pick_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: pick_output_buffer.as_entire_binding(),
                },
            ],
        });
        let pick_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pick-rgb-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("pick_rgb.wgsl").into()),
        });
        let pick_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pick-pipeline-layout"),
            bind_group_layouts: &[Some(&pick_bind_group_layout)],
            immediate_size: 0,
        });
        let pick_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pick-pipeline"),
            layout: Some(&pick_pipeline_layout),
            module: &pick_shader,
            entry_point: Some("cs_main"),
            cache: None,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });

        Self {
            pick_params_buffer,
            pick_output_buffer,
            pick_readback_buffer,
            pick_bind_group,
            pick_pipeline,
        }
    }

    pub fn pick_rgb(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uv: [f32; 2],
    ) -> Option<[f32; 3]> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pick-and-readback"),
        });
        self.encode_pick(&mut encoder, queue, uv);
        encoder.copy_buffer_to_buffer(
            &self.pick_output_buffer,
            0,
            &self.pick_readback_buffer,
            0,
            16,
        );
        queue.submit([encoder.finish()]);

        let slice = self.pick_readback_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(Ok(())) = rx.recv() else {
            return None;
        };

        let bytes = slice.get_mapped_range();
        let read_f32 = |offset: usize| -> f32 {
            let mut raw = [0u8; 4];
            raw.copy_from_slice(&bytes[offset..offset + 4]);
            f32::from_ne_bytes(raw)
        };
        let rgb = [read_f32(0), read_f32(4), read_f32(8)];
        drop(bytes);
        self.pick_readback_buffer.unmap();
        Some(rgb)
    }

    fn encode_pick(&self, encoder: &mut wgpu::CommandEncoder, queue: &wgpu::Queue, uv: [f32; 2]) {
        queue.write_buffer(
            &self.pick_params_buffer,
            0,
            pick_params_as_bytes(&PickParams {
                uv,
                _pad: [0.0, 0.0],
            }),
        );

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pick-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pick_pipeline);
        pass.set_bind_group(0, &self.pick_bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

pub struct OutputPicker {
    format: wgpu::TextureFormat,
    readback_buffer: wgpu::Buffer,
}

impl OutputPicker {
    const ROW_BYTES: u64 = 256;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output-pick-readback-buffer"),
            size: Self::ROW_BYTES,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            format,
            readback_buffer,
        }
    }

    pub fn pick_rgb(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        xy: [u32; 2],
    ) -> Option<[f32; 3]> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("output-pick-copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: xy[0],
                    y: xy[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(Self::ROW_BYTES as u32),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let slice = self.readback_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(Ok(())) = rx.recv() else {
            return None;
        };

        let bytes = slice.get_mapped_range();
        let rgb = match self.format {
            wgpu::TextureFormat::Rgba16Float => {
                let read_f16 = |offset: usize| -> f32 {
                    let mut raw = [0u8; 2];
                    raw.copy_from_slice(&bytes[offset..offset + 2]);
                    f16::from_bits(u16::from_ne_bytes(raw)).to_f32()
                };
                [read_f16(0), read_f16(2), read_f16(4)]
            }
            wgpu::TextureFormat::Rgba32Float => {
                let read_f32 = |offset: usize| -> f32 {
                    let mut raw = [0u8; 4];
                    raw.copy_from_slice(&bytes[offset..offset + 4]);
                    f32::from_ne_bytes(raw)
                };
                [read_f32(0), read_f32(4), read_f32(8)]
            }
            _ => {
                drop(bytes);
                self.readback_buffer.unmap();
                return None;
            }
        };
        drop(bytes);
        self.readback_buffer.unmap();
        Some(rgb)
    }
}
