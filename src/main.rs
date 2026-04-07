use clap::Parser;
use winit::event_loop::{ControlFlow, EventLoop};

mod app;
mod blit;
mod display_hdr;
mod macos_edr;
mod picker;
mod platform;
mod renderer;
mod stats;
mod text_overlay;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
struct Args {
    /// Input .exr image path
    #[arg(value_name = "INPUT_EXR")]
    input: String,
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = app::App::new(args.input);
    event_loop.run_app(&mut app).unwrap();
}
