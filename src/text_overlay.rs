use wgpu::{
    BufferUsages, Extent3d, TextureFormat, TextureUsages, util::DeviceExt,
    wgt::TextureViewDescriptor,
};

const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;
const GLYPH_SCALE: u32 = 2;
const GLYPH_ADVANCE: u32 = (GLYPH_W + 2) * GLYPH_SCALE;
const MAX_TEXT_CHARS: usize = 96;
const ATLAS_COLUMNS: u32 = 16;
const SPACE_GLYPH_INDEX: u32 = 0;

const GLYPH_CHARSET: &[char] = &[
    ' ', '.', ':', ',', '(', ')', '-', '+', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A',
    'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    'U', 'V', 'W', 'X', 'Y', 'Z',
];

#[repr(C)]
#[derive(Copy, Clone)]
struct TextOverlayParams {
    screen_size: [f32; 2],
    text_size: [f32; 2],
    offset: [f32; 2],
    _pad: [f32; 2],
}

fn text_overlay_params_as_bytes(params: &TextOverlayParams) -> &[u8] {
    // Reinterpret packed uniform as bytes for queue upload.
    unsafe {
        core::slice::from_raw_parts(
            (params as *const TextOverlayParams).cast::<u8>(),
            core::mem::size_of::<TextOverlayParams>(),
        )
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct TextBufferCpu {
    text_len: u32,
    _pad: [u32; 3],
    glyph_indices: [u32; MAX_TEXT_CHARS],
    _tail_pad: [u32; 4],
}

fn text_buffer_as_bytes(data: &TextBufferCpu) -> &[u8] {
    // Reinterpret text metadata and glyph indices for storage buffer upload.
    unsafe {
        core::slice::from_raw_parts(
            (data as *const TextBufferCpu).cast::<u8>(),
            core::mem::size_of::<TextBufferCpu>(),
        )
    }
}

pub struct TextOverlay {
    width: u32,
    height: u32,
    render_bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    text_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,
}

impl TextOverlay {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let text_width = GLYPH_ADVANCE * MAX_TEXT_CHARS as u32;
        let text_height = GLYPH_H * GLYPH_SCALE;

        let (atlas_pixels, atlas_w, atlas_h) = build_glyph_atlas_pixels();
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text-glyph-atlas"),
            size: Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[TextureFormat::R8Unorm],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_texture.create_view(&TextureViewDescriptor {
            label: Some("text-glyph-atlas-view"),
            format: Some(TextureFormat::R8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        let params = TextOverlayParams {
            screen_size: [width as f32, height as f32],
            text_size: [text_width as f32, text_height as f32],
            offset: [12.0, 12.0],
            _pad: [0.0, 0.0],
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text-overlay-params"),
            contents: text_overlay_params_as_bytes(&params),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let initial_text = TextBufferCpu {
            text_len: 0,
            _pad: [0; 3],
            glyph_indices: [SPACE_GLYPH_INDEX; MAX_TEXT_CHARS],
            _tail_pad: [0; 4],
        };
        let text_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text-overlay-glyph-indices"),
            contents: text_buffer_as_bytes(&initial_text),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("text-overlay-render-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-overlay-render-bind-group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: text_buffer.as_entire_binding(),
                },
            ],
        });

        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-overlay-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text_overlay.wgsl").into()),
        });
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("text-overlay-pipeline-layout"),
                bind_group_layouts: &[Some(&render_bind_group_layout)],
                immediate_size: 0,
            });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text-overlay-pipeline"),
            layout: Some(&render_pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
        });

        let mut overlay = Self {
            width: text_width,
            height: text_height,
            render_bind_group,
            params_buffer,
            text_buffer,
            render_pipeline,
        };
        overlay.set_text(queue, "RGB: (0.00, 0.00, 0.00)");
        overlay
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        let params = TextOverlayParams {
            screen_size: [width as f32, height as f32],
            text_size: [self.width as f32, self.height as f32],
            offset: [12.0, 12.0],
            _pad: [0.0, 0.0],
        };
        queue.write_buffer(
            &self.params_buffer,
            0,
            text_overlay_params_as_bytes(&params),
        );
    }

    pub fn set_text(&mut self, queue: &wgpu::Queue, text: &str) {
        let mut payload = TextBufferCpu {
            text_len: 0,
            _pad: [0; 3],
            glyph_indices: [SPACE_GLYPH_INDEX; MAX_TEXT_CHARS],
            _tail_pad: [0; 4],
        };

        for (i, ch) in text.chars().take(MAX_TEXT_CHARS).enumerate() {
            payload.glyph_indices[i] = glyph_index_for_char(ch);
            payload.text_len += 1;
        }

        queue.write_buffer(&self.text_buffer, 0, text_buffer_as_bytes(&payload));
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.render_bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

fn glyph_index_for_char(ch: char) -> u32 {
    let upper = ch.to_ascii_uppercase();
    GLYPH_CHARSET
        .iter()
        .position(|c| *c == upper)
        .map(|idx| idx as u32)
        .unwrap_or(SPACE_GLYPH_INDEX)
}

fn build_glyph_atlas_pixels() -> (Vec<u8>, u32, u32) {
    let glyph_count = GLYPH_CHARSET.len() as u32;
    let atlas_rows = glyph_count.div_ceil(ATLAS_COLUMNS);
    let atlas_w = ATLAS_COLUMNS * GLYPH_W;
    let atlas_h = atlas_rows * GLYPH_H;
    let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];

    for (glyph_idx, ch) in GLYPH_CHARSET.iter().enumerate() {
        let glyph_idx = glyph_idx as u32;
        let gx = glyph_idx % ATLAS_COLUMNS;
        let gy = glyph_idx / ATLAS_COLUMNS;
        let base_x = gx * GLYPH_W;
        let base_y = gy * GLYPH_H;

        for row in 0..GLYPH_H {
            let bits = glyph_row_5x7(*ch, row as usize);
            for col in 0..GLYPH_W {
                let bit = (bits >> (GLYPH_W - 1 - col)) & 1;
                if bit == 0 {
                    continue;
                }
                let px = base_x + col;
                let py = base_y + row;
                let idx = (py * atlas_w + px) as usize;
                pixels[idx] = 255;
            }
        }
    }

    (pixels, atlas_w, atlas_h)
}

fn glyph_row_5x7(ch: char, row: usize) -> u8 {
    let c = ch.to_ascii_uppercase();
    match c {
        ' ' => 0,
        '.' => [0, 0, 0, 0, 0, 0, 0b00100][row],
        ':' => [0, 0, 0b00100, 0, 0b00100, 0, 0][row],
        ',' => [0, 0, 0, 0, 0, 0b00100, 0b01000][row],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ][row],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ][row],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0][row],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0][row],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ][row],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ][row],
        '2' => [
            0b11110, 0b00001, 0b00001, 0b11110, 0b10000, 0b10000, 0b11111,
        ][row],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ][row],
        '4' => [
            0b10010, 0b10010, 0b10010, 0b11111, 0b00010, 0b00010, 0b00010,
        ][row],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ][row],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ][row],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ][row],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ][row],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ][row],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ][row],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ][row],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ][row],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ][row],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ][row],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ][row],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ][row],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ][row],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ][row],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ][row],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ][row],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ][row],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ][row],
        'N' => [
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ][row],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ][row],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ][row],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ][row],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ][row],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ][row],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ][row],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ][row],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ][row],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ][row],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ][row],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ][row],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ][row],
        _ => 0,
    }
}
