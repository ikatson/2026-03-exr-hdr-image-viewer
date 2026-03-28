use std::sync::{Arc, mpsc};

use clap::Parser;
#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::AnyObject};
use wgpu::{
    BufferUsages, Extent3d, TextureDescriptor, TextureFormat, TextureUsages, util::DeviceExt,
    wgt::TextureViewDescriptor,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowId},
};
#[cfg(target_os = "macos")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

const PERCENTILES: [f64; 13] = [
    10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 99.0, 99.5, 99.9, 100.0,
];

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
struct Args {
    /// Input .exr image path
    #[arg(value_name = "INPUT_EXR")]
    input: String,
}

fn print_channel_stats(name: &str, data: &[f32]) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut finite_count = 0usize;
    let mut non_finite_count = 0usize;

    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
            sum += v as f64;
            finite_count += 1;
        } else {
            non_finite_count += 1;
        }
    }

    if finite_count == 0 {
        panic!("channel {name} contains no finite samples");
    }

    let avg = sum / finite_count as f64;
    let mut sq_diff_sum = 0.0f64;
    let mut sorted_values = Vec::with_capacity(finite_count);

    for &v in data {
        if !v.is_finite() {
            continue;
        }
        let diff = v as f64 - avg;
        sq_diff_sum += diff * diff;
        sorted_values.push(v);
    }
    sorted_values.sort_by(|a, b| a.total_cmp(b));

    let stddev = (sq_diff_sum / finite_count as f64).sqrt();
    println!(
        "channel={name} n={finite_count} non_finite={non_finite_count} min={min:.6} max={max:.6} avg={avg:.6} stddev={stddev:.6}"
    );
    println!("+-----+----------------+-----------+");
    println!("| pct | x (below this) | below_n   |");
    println!("+-----+----------------+-----------+");

    for &p in &PERCENTILES {
        let rank = ((p / 100.0) * (finite_count.saturating_sub(1)) as f64).round() as usize;
        let x = sorted_values[rank.min(finite_count - 1)];
        let below_n = sorted_values.partition_point(|v| *v <= x);
        println!("| {:>4.1}%| {:>14.6} | {:>9} |", p, x, below_n);
    }
    println!("+-----+----------------+-----------+");
}

fn f32_slice_as_bytes(data: &[f32]) -> &[u8] {
    // Reinterpret f32 channel values as raw bytes for GPU upload.
    unsafe { core::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 4) }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct ShaderParams {
    exposure: f32,
    output_scale: f32,
    tone_map_mode: u32, // 0: passthrough, 1: ACES
    _pad0: u32,
}

fn shader_params_as_bytes(params: &ShaderParams) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(
            (params as *const ShaderParams).cast::<u8>(),
            core::mem::size_of::<ShaderParams>(),
        )
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PickParams {
    uv: [f32; 2],
    _pad: [f32; 2],
}

fn pick_params_as_bytes(params: &PickParams) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(
            (params as *const PickParams).cast::<u8>(),
            core::mem::size_of::<PickParams>(),
        )
    }
}

struct GpuPicker {
    pick_params_buffer: wgpu::Buffer,
    pick_output_buffer: wgpu::Buffer,
    pick_readback_buffer: wgpu::Buffer,
    pick_bind_group: wgpu::BindGroup,
    pick_pipeline: wgpu::ComputePipeline,
}

impl GpuPicker {
    fn new(
        device: &wgpu::Device,
        r_view: &wgpu::TextureView,
        g_view: &wgpu::TextureView,
        b_view: &wgpu::TextureView,
    ) -> Self {
        const PICK_RESULT_SIZE: u64 = 16;
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
            size: PICK_RESULT_SIZE,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pick_readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick-readback-buffer"),
            size: PICK_RESULT_SIZE,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
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

    fn pick_rgb(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uv: [f32; 2],
    ) -> Option<[f32; 3]> {
        const PICK_RESULT_SIZE: u64 = 16;
        queue.write_buffer(
            &self.pick_params_buffer,
            0,
            pick_params_as_bytes(&PickParams { uv, _pad: [0.0, 0.0] }),
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pick-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("pick-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pick_pipeline);
            pass.set_bind_group(0, &self.pick_bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.pick_output_buffer,
            0,
            &self.pick_readback_buffer,
            0,
            PICK_RESULT_SIZE,
        );
        queue.submit([encoder.finish()]);

        let slice = self.pick_readback_buffer.slice(..PICK_RESULT_SIZE);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(Ok(())) = rx.recv() else {
            return None;
        };

        let mapped = slice.get_mapped_range();
        if mapped.len() < 16 {
            drop(mapped);
            self.pick_readback_buffer.unmap();
            return None;
        }
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&mapped[..16]);
        drop(mapped);
        self.pick_readback_buffer.unmap();

        let r = f32::from_ne_bytes(bytes[0..4].try_into().unwrap());
        let g = f32::from_ne_bytes(bytes[4..8].try_into().unwrap());
        let b = f32::from_ne_bytes(bytes[8..12].try_into().unwrap());
        Some([r, g, b])
    }
}

struct RenderPipelineState {
    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    exposure: f32,
    tone_map_enabled: bool,
    output_scale: f32,
}

impl RenderPipelineState {
    fn new(
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
        let exposure = 1.0f32;
        let tone_map_enabled = true;
        let output_scale = 1.6f32;
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shader-params-buffer"),
            contents: shader_params_as_bytes(&ShaderParams {
                exposure,
                output_scale,
                tone_map_mode: u32::from(tone_map_enabled),
                _pad0: 0,
            }),
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
            exposure,
            tone_map_enabled,
            output_scale,
        }
    }

    fn update_shader_params(&self, queue: &wgpu::Queue) {
        let params = ShaderParams {
            exposure: self.exposure,
            output_scale: self.output_scale,
            tone_map_mode: u32::from(self.tone_map_enabled),
            _pad0: 0,
        };
        queue.write_buffer(&self.params_buffer, 0, shader_params_as_bytes(&params));
    }

    fn print_render_params(&self) {
        println!(
            "exposure={:.4} tone_map={} output_scale={:.3}",
            self.exposure, self.tone_map_enabled, self.output_scale
        );
    }
}

fn pick_hdr_surface_format(cap: &wgpu::SurfaceCapabilities) -> TextureFormat {
    let preferred = [TextureFormat::Rgba16Float, TextureFormat::Rgba32Float];
    preferred
        .iter()
        .find_map(|fmt| cap.formats.iter().copied().find(|f| f == fmt))
        .unwrap_or_else(|| {
            panic!(
                "No HDR surface format available. Supported formats: {:?}",
                cap.formats
            )
        })
}

#[cfg(target_os = "macos")]
fn macos_edr_headroom(window: &Window) -> Option<(f32, f32)> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };

    let ns_view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    if ns_view.is_null() {
        return None;
    }

    let ns_window: *mut AnyObject = unsafe { msg_send![ns_view, window] };
    if ns_window.is_null() {
        return None;
    }

    let ns_screen: *mut AnyObject = unsafe { msg_send![ns_window, screen] };
    if ns_screen.is_null() {
        return None;
    }

    let current: f64 =
        unsafe { msg_send![ns_screen, maximumExtendedDynamicRangeColorComponentValue] };
    let potential: f64 =
        unsafe { msg_send![ns_screen, maximumPotentialExtendedDynamicRangeColorComponentValue] };
    Some((current as f32, potential as f32))
}

struct State {
    instance: wgpu::Instance,
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    renderer: RenderPipelineState,
    picker: GpuPicker,
    image_width: u32,
    image_height: u32,
    cursor_pos: Option<PhysicalPosition<f64>>,
    left_mouse_down: bool,
    #[cfg(target_os = "macos")]
    edr_probe_frame: u64,
    #[cfg(target_os = "macos")]
    edr_last_current: f32,
    #[cfg(target_os = "macos")]
    edr_last_potential: f32,
}

impl State {
    async fn new(display: OwnedDisplayHandle, window: Arc<Window>, exr_path: &str) -> State {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let size = window.inner_size();

        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = pick_hdr_surface_format(&cap);
        println!(
            "surface selected: format={:?} present_modes={:?} alpha_modes={:?}",
            surface_format, cap.present_modes, cap.alpha_modes
        );

        let (image_width, image_height, r_view, g_view, b_view) = {
            use ::exr::prelude::*;
            let img = read_all_data_from_file(exr_path).unwrap();
            let first_layer = &img.layer_data[0];
            let channels = &first_layer.channel_data.list;
            let image_width: u32 = first_layer.size.width().try_into().unwrap();
            let image_height: u32 = first_layer.size.height().try_into().unwrap();

            let channel_values = |name: &str| -> Vec<f32> {
                let channel = channels
                    .iter()
                    .find(|c| c.name.eq(name))
                    .unwrap_or_else(|| {
                        panic!("missing EXR channel: {name}");
                    });
                let data = match &channel.sample_data.levels_as_slice()[0] {
                    FlatSamples::F32(data) => data.as_slice(),
                    _ => panic!("unsupported sample format for channel: {name}"),
                };
                print_channel_stats(name, data);
                data.to_vec()
            };

            let channel_r = channel_values("R");
            let channel_g = channel_values("G");
            let channel_b = channel_values("B");
            let expected_len = (image_width * image_height) as usize;
            assert_eq!(channel_r.len(), expected_len, "unexpected R channel size");
            assert_eq!(channel_g.len(), expected_len, "unexpected G channel size");
            assert_eq!(channel_b.len(), expected_len, "unexpected B channel size");

            let create_channel_view = |label: &'static str, values: &[f32]| {
                let texture = device.create_texture_with_data(
                    &queue,
                    &TextureDescriptor {
                        label: Some(label),
                        size: Extent3d {
                            width: image_width,
                            height: image_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: TextureFormat::R32Float,
                        usage: TextureUsages::TEXTURE_BINDING,
                        view_formats: &[TextureFormat::R32Float],
                    },
                    wgpu::wgt::TextureDataOrder::default(),
                    f32_slice_as_bytes(values),
                );
                texture.create_view(&TextureViewDescriptor {
                    label: Some(label),
                    format: Some(TextureFormat::R32Float),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: Some(TextureUsages::TEXTURE_BINDING),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                })
            };
            let r_view = create_channel_view("r-channel-view", &channel_r);
            let g_view = create_channel_view("g-channel-view", &channel_g);
            let b_view = create_channel_view("b-channel-view", &channel_b);

            (
                image_width,
                image_height,
                r_view,
                g_view,
                b_view,
            )
        };

        let renderer = RenderPipelineState::new(&device, surface_format, &r_view, &g_view, &b_view);
        let picker = GpuPicker::new(&device, &r_view, &g_view, &b_view);

        let state = State {
            instance,
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            renderer,
            picker,
            image_width,
            image_height,
            cursor_pos: None,
            left_mouse_down: false,
            #[cfg(target_os = "macos")]
            edr_probe_frame: 0,
            #[cfg(target_os = "macos")]
            edr_last_current: f32::NAN,
            #[cfg(target_os = "macos")]
            edr_last_potential: f32::NAN,
        };

        // Configure surface for the first time
        state.configure_surface();
        println!("controls: left-click = pick RGB at cursor");
        println!("controls: [ = exposure * 0.9, ] = exposure * 1.1");
        println!("controls: T = toggle ACES tone mapping");
        println!("controls: -/= adjust ACES output scale (HDR peak)");
        println!("controls: F = toggle fullscreen");
        println!(
            "initial params: exposure={:.4} tone_map={} output_scale={:.3}",
            state.renderer.exposure, state.renderer.tone_map_enabled, state.renderer.output_scale
        );

        state
    }

    fn get_window(&self) -> &Window {
        &self.window
    }

    fn configure_surface(&self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            // Request compatibility with the sRGB-format texture view we‘re going to create later.
            view_formats: vec![self.surface_format],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        // reconfigure the surface
        self.configure_surface();
    }

    fn update_cursor_pos(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_pos = Some(position);
    }

    #[cfg(target_os = "macos")]
    fn probe_macos_edr_after_present(&mut self) {
        const EDR_QUERY_INTERVAL_FRAMES: u64 = 30;
        const EDR_PRINT_DELTA: f32 = 0.01;
        const EDR_SCALE_DELTA: f32 = 0.005;
        self.edr_probe_frame += 1;
        if self.edr_probe_frame % EDR_QUERY_INTERVAL_FRAMES != 0 {
            return;
        }

        let Some((current, potential)) = macos_edr_headroom(&self.window) else {
            return;
        };

        let new_scale = current.max(1.0);
        if (new_scale - self.renderer.output_scale).abs() > EDR_SCALE_DELTA {
            self.renderer.output_scale = new_scale;
            self.renderer.update_shader_params(&self.queue);
        }

        let changed = self.edr_last_current.is_nan()
            || self.edr_last_potential.is_nan()
            || (current - self.edr_last_current).abs() > EDR_PRINT_DELTA
            || (potential - self.edr_last_potential).abs() > EDR_PRINT_DELTA;
        if changed {
            println!(
                "macOS EDR headroom: current={:.3} potential={:.3} -> output_scale={:.3}",
                current, potential, self.renderer.output_scale
            );
            self.edr_last_current = current;
            self.edr_last_potential = potential;
        }
    }

    fn adjust_exposure(&mut self, factor: f32) {
        self.renderer.exposure = (self.renderer.exposure * factor).max(0.001);
        self.renderer.update_shader_params(&self.queue);
        self.renderer.print_render_params();
    }

    fn toggle_tone_map(&mut self) {
        self.renderer.tone_map_enabled = !self.renderer.tone_map_enabled;
        self.renderer.update_shader_params(&self.queue);
        self.renderer.print_render_params();
    }

    fn adjust_output_scale(&mut self, factor: f32) {
        self.renderer.output_scale = (self.renderer.output_scale * factor).max(0.1);
        self.renderer.update_shader_params(&self.queue);
        self.renderer.print_render_params();
    }

    fn toggle_fullscreen(&self) {
        if self.window.fullscreen().is_some() {
            self.window.set_fullscreen(None);
            println!("fullscreen=false");
        } else {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
            println!("fullscreen=true");
        }
    }

    fn pick_uv_and_coords(&self) -> Option<([f32; 2], usize, usize)> {
        let Some(cursor_pos) = self.cursor_pos else {
            return None;
        };
        if self.size.width == 0 || self.size.height == 0 {
            return None;
        }

        let max_x = self.size.width.saturating_sub(1) as f64;
        let max_y = self.size.height.saturating_sub(1) as f64;
        let clamped_x = cursor_pos.x.clamp(0.0, max_x);
        let clamped_y = cursor_pos.y.clamp(0.0, max_y);

        let u = clamped_x / self.size.width as f64;
        let v = clamped_y / self.size.height as f64;
        let px = (u * self.image_width.saturating_sub(1) as f64).round() as usize;
        let py = (v * self.image_height.saturating_sub(1) as f64).round() as usize;
        Some(([u as f32, v as f32], px, py))
    }

    fn print_picked_color(&mut self) {
        let Some((uv, px, py)) = self.pick_uv_and_coords() else {
            return;
        };
        let Some([r, g, b]) = self.picker.pick_rgb(&self.device, &self.queue, uv) else {
            println!("pick failed");
            return;
        };
        let cursor_pos = self.cursor_pos.unwrap();
        println!(
            "pick window=({:.1},{:.1}) image=({}, {}) rgb=({:.6}, {:.6}, {:.6})",
            cursor_pos.x, cursor_pos.y, px, py, r, g, b
        );
    }

    fn render(&mut self) {
        // Create texture view.
        // NOTE: We must handle Timeout because the surface may be unavailable
        // (e.g., when the window is occluded on macOS).
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("No error scope registered, so validation errors will panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.configure_surface();
                return;
            }
        };
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        renderpass.set_pipeline(&self.renderer.render_pipeline);
        renderpass.set_bind_group(0, &self.renderer.bind_group, &[]);
        renderpass.draw(0..3, 0..1);
        drop(renderpass);

        // Submit the command in the queue to execute
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        #[cfg(target_os = "macos")]
        self.probe_macos_edr_after_present();
    }
}

struct App {
    state: Option<State>,
    exr_path: String,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window object
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );

        let state = pollster::block_on(State::new(
            event_loop.owned_display_handle(),
            window.clone(),
            &self.exr_path,
        ));
        self.state = Some(state);

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = self.state.as_mut().unwrap();
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                state.render();
                // Emits a new redraw requested event.
                state.get_window().request_redraw();
            }
            WindowEvent::Resized(size) => {
                // Reconfigures the size of the surface. We do not re-render
                // here as this event is always followed up by redraw request.
                state.resize(size);
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.update_cursor_pos(position);
                if state.left_mouse_down {
                    state.print_picked_color();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                state.left_mouse_down = true;
                state.print_picked_color();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                state.left_mouse_down = false;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::BracketLeft) => state.adjust_exposure(0.9),
                    PhysicalKey::Code(KeyCode::BracketRight) => state.adjust_exposure(1.1),
                    PhysicalKey::Code(KeyCode::KeyT) => state.toggle_tone_map(),
                    PhysicalKey::Code(KeyCode::Minus) => state.adjust_output_scale(0.9),
                    PhysicalKey::Code(KeyCode::Equal) => state.adjust_output_scale(1.1),
                    PhysicalKey::Code(KeyCode::KeyF) => state.toggle_fullscreen(),
                    _ => {}
                }
            }
            _ => (),
        }
    }
}

fn main() {
    // wgpu uses `log` for all of our logging, so we initialize a logger with the `env_logger` crate.
    //
    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    env_logger::init();
    let args = Args::parse();

    let event_loop = EventLoop::new().unwrap();

    // When the current loop iteration finishes, immediately begin a new
    // iteration regardless of whether or not new events are available to
    // process. Preferred for applications that want to render as fast as
    // possible, like games.
    event_loop.set_control_flow(ControlFlow::Poll);

    // When the current loop iteration finishes, suspend the thread until
    // another event arrives. Helps keeping CPU utilization low if nothing
    // is happening, which is preferred if the application might be idling in
    // the background.
    // event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        state: None,
        exr_path: args.input,
    };
    event_loop.run_app(&mut app).unwrap();
}
