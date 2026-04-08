use std::path::Path;
use std::sync::Arc;
#[cfg(not(target_os = "macos"))]
use std::thread;

use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureView};
use wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages, wgt::TextureViewDescriptor};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowId},
};

use crate::platform;
use crate::{
    blit::BlitPipeline,
    picker::{GpuPicker, OutputPicker},
    renderer::RenderPipelineState,
    stats::print_channel_stats,
    text_overlay::TextOverlay,
};
#[cfg(target_os = "macos")]
use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};

fn f32_slice_as_bytes(data: &[f32]) -> &[u8] {
    // Reinterpret f32 channel values as raw bytes for GPU upload.
    unsafe { core::slice::from_raw_parts(data.as_ptr().cast::<u8>(), std::mem::size_of_val(data)) }
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
            print_channel_stats("R/Y", &channel_r);
            print_channel_stats("G/Cb", &channel_g);
            print_channel_stats("B/Cr", &channel_b);
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
    debug_picker: OutputPicker,
    blitter: BlitPipeline,
    display_texture: wgpu::Texture,
    display_texture_view: wgpu::TextureView,
    debug_texture: wgpu::Texture,
    debug_texture_view: wgpu::TextureView,
    text_overlay: TextOverlay,
    cursor_pos: Option<PhysicalPosition<f64>>,
    picked_source_rgb: Option<[f32; 3]>,
    picked_debug_rgb: Option<[f32; 3]>,
    left_mouse_down: bool,
    hdr: crate::display_hdr::DisplayHDR,
}

impl State {
    const DEBUG_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba32Float;

    async fn new(display: OwnedDisplayHandle, window: Arc<Window>, image_path: &str) -> Self {
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

        let (r_view, g_view, b_view) = open_image(&device, &queue, Path::new(image_path)).unwrap();

        let mut renderer = RenderPipelineState::new(
            &device,
            surface_format,
            Self::DEBUG_TEXTURE_FORMAT,
            &r_view,
            &g_view,
            &b_view,
            image_path.ends_with(".yuv"),
        );
        let hdr = platform::get_hdr_params(&window).unwrap();
        renderer.update_shader_params(&queue);
        renderer.hdr = hdr;
        let picker = GpuPicker::new(&device, &r_view, &g_view, &b_view);
        let (display_texture, display_texture_view) = create_display_texture(
            &device,
            size.width.max(1),
            size.height.max(1),
            surface_format,
        );
        let (debug_texture, debug_texture_view) = create_display_texture(
            &device,
            size.width.max(1),
            size.height.max(1),
            Self::DEBUG_TEXTURE_FORMAT,
        );
        let debug_picker = OutputPicker::new(&device, Self::DEBUG_TEXTURE_FORMAT);
        let blitter = BlitPipeline::new(&device, surface_format, &display_texture_view);
        let text_overlay = TextOverlay::new(
            &device,
            &queue,
            surface_format,
            size.width.max(1),
            size.height.max(1),
        );

        let state = Self {
            instance,
            device,
            queue,
            size,
            surface,
            surface_format,
            renderer,
            picker,
            debug_picker,
            blitter,
            display_texture,
            display_texture_view,
            debug_texture,
            debug_texture_view,
            text_overlay,
            cursor_pos: None,
            picked_source_rgb: None,
            picked_debug_rgb: None,
            left_mouse_down: false,
            hdr,
            window,
        };

        state.configure_surface();
        println!("controls: left-click = pick RGB at cursor");
        println!("controls: [ = exposure * 0.9, ] = exposure * 1.1");
        println!("controls: T = cycle tone mapping");
        println!("controls: F = toggle fullscreen");
        let mut state = state;
        state.refresh_overlay_text();

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
            width: self.size.width.max(1),
            height: self.size.height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();
        let (display_texture, display_texture_view) = create_display_texture(
            &self.device,
            self.size.width.max(1),
            self.size.height.max(1),
            self.surface_format,
        );
        let (debug_texture, debug_texture_view) = create_display_texture(
            &self.device,
            self.size.width.max(1),
            self.size.height.max(1),
            Self::DEBUG_TEXTURE_FORMAT,
        );
        self.display_texture = display_texture;
        self.display_texture_view = display_texture_view;
        self.debug_texture = debug_texture;
        self.debug_texture_view = debug_texture_view;
        self.blitter
            .update_source(&self.device, &self.display_texture_view);
        self.text_overlay
            .resize(&self.queue, self.size.width.max(1), self.size.height.max(1));
    }

    fn update_cursor_pos(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_pos = Some(position);
    }

    fn pick_coords(&self) -> Option<([f32; 2], [u32; 2])> {
        let cursor_pos = self.cursor_pos?;
        if self.size.width == 0 || self.size.height == 0 {
            return None;
        }

        let max_x = self.size.width.saturating_sub(1) as f64;
        let max_y = self.size.height.saturating_sub(1) as f64;
        let clamped_x = cursor_pos.x.clamp(0.0, max_x);
        let clamped_y = cursor_pos.y.clamp(0.0, max_y);

        let uv = [
            (clamped_x / self.size.width as f64) as f32,
            (clamped_y / self.size.height as f64) as f32,
        ];
        let xy = [clamped_x.floor() as u32, clamped_y.floor() as u32];
        Some((uv, xy))
    }

    fn tone_map_label(&self) -> &'static str {
        match self.renderer.tone_map_mode {
            crate::renderer::ToneMapMode::Off => "OFF",
            crate::renderer::ToneMapMode::Aces => "ACES",
            crate::renderer::ToneMapMode::GranTurismo7 => "GT7",
            crate::renderer::ToneMapMode::Reinhard => "RH",
            crate::renderer::ToneMapMode::Neutwo => "NEU",
            crate::renderer::ToneMapMode::RenoAces => "RENO-ACES",
            crate::renderer::ToneMapMode::AgX => "AGX",
            crate::renderer::ToneMapMode::Bt2390 => "BT2390",
        }
    }

    fn refresh_overlay_text(&mut self) {
        let mut text = format!(
            "EXP {:.2} TM {} PEAK {:.2}",
            self.renderer.exposure,
            self.tone_map_label(),
            self.renderer.hdr.peak_luma_nits,
        );
        if let Some([r, g, b]) = self.picked_source_rgb {
            text.push_str(&format!(" IN {:.2} {:.2} {:.2}", r, g, b));
        }
        if let Some([r, g, b]) = self.picked_debug_rgb {
            text.push_str(&format!(" DBG {:.2} {:.2} {:.2}", r, g, b));
        }
        self.text_overlay.set_text(&self.queue, &text);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn probe_display_hdr(&mut self) {
        let display_hdr = platform::get_hdr_params(&self.window).unwrap();
        if display_hdr == self.hdr {
            return;
        }
        self.hdr = display_hdr;
        self.renderer.hdr = display_hdr;
        self.renderer.update_shader_params(&self.queue);
    }

    fn adjust_exposure(&mut self, factor: f32) {
        self.renderer.exposure = (self.renderer.exposure * factor).max(0.001);
        self.renderer.update_shader_params(&self.queue);
        self.refresh_overlay_text();
    }

    fn cycle_tone_map(&mut self) {
        self.renderer.tone_map_mode = self.renderer.tone_map_mode.next();
        self.renderer.update_shader_params(&self.queue);
        self.refresh_overlay_text();
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

    fn update_picked_color_overlay(&mut self) {
        let Some((uv, xy)) = self.pick_coords() else {
            return;
        };
        if let Some([r, g, b]) = self.picker.pick_rgb(&self.device, &self.queue, uv) {
            self.picked_source_rgb = Some([r, g, b]);
        }
        if let Some([r, g, b]) =
            self.debug_picker
                .pick_rgb(&self.device, &self.queue, &self.debug_texture, xy)
        {
            self.picked_debug_rgb = Some([r, g, b]);
        }
        self.refresh_overlay_text();
    }

    fn render(&mut self) {
        if self.size.width == 0 || self.size.height == 0 {
            return;
        }
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("wgpu surface validation error; skipping frame");
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.configure_surface();
                return;
            }
        };
        self.probe_display_hdr();

        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene-pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.display_texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.debug_texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        self.renderer.render(&mut renderpass);
        drop(renderpass);

        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present-pass"),
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

        self.blitter.render(&mut renderpass);
        self.text_overlay.render(&mut renderpass);
        drop(renderpass);

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
    }
}

fn create_display_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("display-texture"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[format],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

pub struct App {
    state: Option<State>,
    img_path: String,
}

impl App {
    pub fn new(exr_path: String) -> Self {
        Self {
            state: None,
            img_path: exr_path,
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
            &self.img_path,
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
                    PhysicalKey::Code(KeyCode::KeyT) => state.cycle_tone_map(),
                    PhysicalKey::Code(KeyCode::KeyF) => state.toggle_fullscreen(),
                    _ => {}
                }
            }
            _ => (),
        }
    }
}

fn open_yuv(
    device: &Device,
    queue: &Queue,
    path: &Path,
    w: u16,
    h: u16,
) -> anyhow::Result<(TextureView, TextureView, TextureView)> {
    let w = w as u32;
    let h = h as u32;
    let data = std::fs::read(path)?;

    let y_len = w * h * 2;
    let u_len = w / 2 * h / 2 * 2;
    let v_len = u_len;
    assert_eq!(data.len(), (y_len + u_len + v_len) as usize);

    // TODO: this is horrible, but I need to see the picture, so hacking to work with existing f32 code
    fn u16_as_f32(b: &[u8]) -> Vec<f32> {
        let mut output = Vec::new();
        for chunk in b.as_chunks::<2>().0 {
            output.push(u16::from_le_bytes(*chunk) as f32);
        }
        output
    }

    let mut data = &data[..];
    let y = u16_as_f32(data.split_off(..y_len as usize).unwrap());
    let u = u16_as_f32(data.split_off(..u_len as usize).unwrap());
    let v = u16_as_f32(data.split_off(..v_len as usize).unwrap());

    let y_tx = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("y"),
            size: Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::R32Float],
        },
        Default::default(),
        f32_slice_as_bytes(&y),
    );

    let u_tx = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("y"),
            size: Extent3d {
                width: w / 2,
                height: h / 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::R32Float],
        },
        Default::default(),
        f32_slice_as_bytes(&u),
    );

    let v_tx = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("y"),
            size: Extent3d {
                width: w / 2,
                height: h / 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::R32Float],
        },
        Default::default(),
        f32_slice_as_bytes(&v),
    );

    spawn_channel_stats_thread(y, u, v);

    Ok((
        y_tx.create_view(&TextureViewDescriptor {
            label: Some("y view"),
            format: Some(TextureFormat::R32Float),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        }),
        u_tx.create_view(&TextureViewDescriptor {
            label: Some("y view"),
            format: Some(TextureFormat::R32Float),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        }),
        v_tx.create_view(&TextureViewDescriptor {
            label: Some("y view"),
            format: Some(TextureFormat::R32Float),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        }),
    ))
}

fn open_image(
    device: &Device,
    queue: &Queue,
    path: &Path,
) -> anyhow::Result<(TextureView, TextureView, TextureView)> {
    if path.extension().is_some_and(|ext| ext == "yuv") {
        return open_yuv(device, queue, path, 3840, 2160);
    }

    let (r_view, g_view, b_view) = {
        let image = image::ImageReader::open(path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        let image_width = image.width();
        let image_height = image.height();
        let rgb = image.into_rgb32f();
        let mut channel_r = Vec::with_capacity(rgb.len());
        let mut channel_g = Vec::with_capacity(rgb.len());
        let mut channel_b = Vec::with_capacity(rgb.len());

        if rgb.color_space().transfer == image::metadata::CicpTransferCharacteristics::Linear
            || path.extension().is_some_and(|ext| ext == "exr")
        {
            for [r, g, b] in rgb.as_chunks::<3>().0 {
                channel_r.push(*r);
                channel_g.push(*g);
                channel_b.push(*b);
            }
        } else {
            for [r, g, b] in rgb.as_chunks::<3>().0 {
                channel_r.push(r.powf(2.2));
                channel_g.push(g.powf(2.2));
                channel_b.push(b.powf(2.2));
            }
        }

        if path.extension().is_some_and(|e| e == "exr") {
            dbg!(::exr::prelude::read_all_data_from_file(path).unwrap());
        }

        let create_channel_view = |label: &'static str, values: &[f32]| {
            let texture = device.create_texture_with_data(
                queue,
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
    Ok((r_view, g_view, b_view))
}
