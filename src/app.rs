use std::sync::Arc;
#[cfg(not(target_os = "macos"))]
use std::thread;

use exr::prelude::{FlatSamples, read_all_data_from_file};
use wgpu::util::DeviceExt;
use wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages, wgt::TextureViewDescriptor};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowId},
};

#[cfg(target_os = "macos")]
use crate::macos_edr::macos_edr_headroom;
#[cfg(target_os = "windows")]
use crate::windows_hdr::windows_hdr_headroom;
use crate::{
    picker::GpuPicker, renderer::RenderPipelineState, stats::print_channel_stats,
    text_overlay::TextOverlay,
};
#[cfg(target_os = "macos")]
use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};

fn f32_slice_as_bytes(data: &[f32]) -> &[u8] {
    // Reinterpret f32 channel values as raw bytes for GPU upload.
    unsafe { core::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 4) }
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

fn spawn_channel_stats_thread(channel_r: Vec<f32>, channel_g: Vec<f32>, channel_b: Vec<f32>) {
    #[cfg(target_os = "macos")]
    {
        let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(
            DispatchQoS::Background,
        ));
        queue.exec_async(move || {
            print_channel_stats("R", &channel_r);
            print_channel_stats("G", &channel_g);
            print_channel_stats("B", &channel_b);
        });
    }

    #[cfg(not(target_os = "macos"))]
    thread::spawn(move || {
        print_channel_stats("R", &channel_r);
        print_channel_stats("G", &channel_g);
        print_channel_stats("B", &channel_b);
    });
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
    text_overlay: TextOverlay,
    cursor_pos: Option<PhysicalPosition<f64>>,
    left_mouse_down: bool,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    edr_probe_frame: u64,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    edr_last_current: f32,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    edr_last_potential: f32,
}

impl State {
    async fn new(display: OwnedDisplayHandle, window: Arc<Window>, exr_path: &str) -> Self {
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

        let (r_view, g_view, b_view) = {
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
            spawn_channel_stats_thread(channel_r, channel_g, channel_b);

            (r_view, g_view, b_view)
        };

        let renderer = RenderPipelineState::new(&device, surface_format, &r_view, &g_view, &b_view);
        let picker = GpuPicker::new(&device, &r_view, &g_view, &b_view);
        let text_overlay = TextOverlay::new(
            &device,
            &queue,
            surface_format,
            size.width.max(1),
            size.height.max(1),
        );

        let state = Self {
            instance,
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            renderer,
            picker,
            text_overlay,
            cursor_pos: None,
            left_mouse_down: false,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            edr_probe_frame: 0,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            edr_last_current: f32::NAN,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            edr_last_potential: f32::NAN,
        };

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
        self.configure_surface();
        self.text_overlay
            .resize(&self.queue, self.size.width.max(1), self.size.height.max(1));
    }

    fn update_cursor_pos(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_pos = Some(position);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn probe_platform_hdr_after_present(&mut self) {
        const EDR_PRINT_DELTA: f32 = 0.01;
        const EDR_SCALE_DELTA: f32 = 0.005;
        self.edr_probe_frame += 1;

        return;

        #[cfg(target_os = "macos")]
        let probe = ("macOS EDR", macos_edr_headroom(&self.window));
        #[cfg(target_os = "windows")]
        let probe = ("Windows HDR", windows_hdr_headroom(&self.window));

        let Some((current, potential)) = probe.1 else {
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
                "{} headroom: current={:.3} potential={:.3} -> output_scale={:.3}",
                probe.0, current, potential, self.renderer.output_scale
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

    fn pick_uv(&self) -> Option<[f32; 2]> {
        let cursor_pos = self.cursor_pos?;
        if self.size.width == 0 || self.size.height == 0 {
            return None;
        }

        let max_x = self.size.width.saturating_sub(1) as f64;
        let max_y = self.size.height.saturating_sub(1) as f64;
        let clamped_x = cursor_pos.x.clamp(0.0, max_x);
        let clamped_y = cursor_pos.y.clamp(0.0, max_y);

        let u = clamped_x / self.size.width as f64;
        let v = clamped_y / self.size.height as f64;
        Some([u as f32, v as f32])
    }

    fn update_picked_color_overlay(&mut self) {
        let Some(uv) = self.pick_uv() else {
            return;
        };
        if let Some([r, g, b]) = self.picker.pick_rgb(&self.device, &self.queue, uv) {
            let text = format!("RGB: ({r:.2}, {g:.2}, {b:.2})");
            self.text_overlay.set_text(&self.queue, &text);
        }
    }

    fn render(&mut self) {
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

        self.renderer.render(&mut renderpass);
        self.text_overlay.render(&mut renderpass);
        drop(renderpass);

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.probe_platform_hdr_after_present();
    }
}

pub struct App {
    state: Option<State>,
    exr_path: String,
}

impl App {
    pub fn new(exr_path: String) -> Self {
        Self {
            state: None,
            exr_path,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
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
                state.get_window().request_redraw();
            }
            WindowEvent::Resized(size) => {
                state.resize(size);
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.update_cursor_pos(position);
                if state.left_mouse_down {
                    state.update_picked_color_overlay();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                state.left_mouse_down = true;
                state.update_picked_color_overlay();
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
