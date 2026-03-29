use wgpu::{BufferUsages, TextureFormat, util::DeviceExt};

use crate::display_hdr::DisplayHDR;

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum ToneMapMode {
    Off = 0,
    Aces = 1,
    RenoAces = 2,
    GranTurismo7 = 3,
    Reinhard = 4,
    Neutwo = 5,
}

impl ToneMapMode {
    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Aces => "aces",
            Self::RenoAces => "reno-aces",
            Self::GranTurismo7 => "gt7",
            Self::Reinhard => "reinhard",
            Self::Neutwo => "neutwo",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Aces,
            Self::Aces => Self::RenoAces,
            Self::RenoAces => Self::GranTurismo7,
            Self::GranTurismo7 => Self::Reinhard,
            Self::Reinhard => Self::Neutwo,
            Self::Neutwo => Self::Off,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct ShaderParams {
    exposure: f32,
    sdr_white_vs_input: f32,
    peak_luma_vs_sdr_white: f32,
    tone_map_mode: u32,
}

fn shader_params_as_bytes(params: &ShaderParams) -> &[u8] {
    // Reinterpret packed uniform as bytes for queue upload.
    unsafe {
        core::slice::from_raw_parts(
            (params as *const ShaderParams).cast::<u8>(),
            core::mem::size_of::<ShaderParams>(),
        )
    }
}

pub struct RenderPipelineState {
    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    pub hdr: DisplayHDR,
    pub exposure: f32,
    pub tone_map_mode: ToneMapMode,
}

impl RenderPipelineState {
    pub fn new(
        device: &wgpu::Device,
        surface_format: TextureFormat,
        r_view: &wgpu::TextureView,
        g_view: &wgpu::TextureView,
        b_view: &wgpu::TextureView,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rgb-texture-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shader-params-buffer"),
            contents: shader_params_as_bytes(&ShaderParams::default()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rgb-texture-bind-group"),
            layout: &bind_group_layout,
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
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("r-to-rgb-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("r_to_rgb.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("r-to-rgb-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("r-to-rgb-pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
        });

        Self {
            bind_group,
            render_pipeline,
            params_buffer,
            tone_map_mode: ToneMapMode::Off,
            hdr: DisplayHDR::default(),
            exposure: 1.,
        }
    }

    pub fn update_shader_params(&self, queue: &wgpu::Queue) {
        let params = ShaderParams {
            exposure: self.exposure,
            tone_map_mode: self.tone_map_mode as u32,
            sdr_white_vs_input: self.hdr.sdr_white_vs_input,
            peak_luma_vs_sdr_white: self.hdr.peak_luma_vs_sdr_white,
        };
        queue.write_buffer(&self.params_buffer, 0, shader_params_as_bytes(&params));
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
