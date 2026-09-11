use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

use clap::Parser;
use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::player::renderer::Renderer;

mod decoder;
mod renderer;

const FRAMES_BUFFER_LEN: usize = 3;

#[derive(Parser, Clone)]
#[command(version, about, long_about=None)]
struct Args {
    /// an .h264 file to play
    filename: PathBuf,

    /// framerate to play the video at
    framerate: u64,
}

struct FrameWithPts {
    frame: wgpu::Texture,
    /// Presentation timestamp
    pts: Duration,
}

struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    current_frame: FrameWithPts,
    next_frame: Option<FrameWithPts>,
    frame_receiver: Receiver<FrameWithPts>,
    start_timestamp: Instant,
}

impl AppState {
    fn new(args: Args, event_loop: &ActiveEventLoop) -> Self {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("gpu-video example player")
                    .with_resizable(false),
            )
            .unwrap();
        let window = Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::default()))
            .into_iter()
            .find(|a| {
                a.video_adapter_info()
                    .is_some_and(|info| info.decode_capabilities.h264.is_some())
            })
            .unwrap();
        let (device, queue) = adapter
            .request_device_with_video_support(&VideoDeviceDescriptor::default())
            .unwrap();

        let surface = instance.create_surface(window.clone()).unwrap();

        let (tx, rx) = mpsc::sync_channel(FRAMES_BUFFER_LEN);
        let device_clone = device.clone();

        std::thread::spawn(move || {
            let file = std::fs::File::open(&args.filename).expect("open file");
            decoder::run_decoder(tx, args.framerate, device_clone, file);
        });

        let renderer = Renderer::new(surface, adapter, device, queue, &window);
        let current_frame = rx.recv().unwrap();

        let _ = window.request_inner_size(PhysicalSize::new(
            current_frame.frame.size().width,
            current_frame.frame.size().height,
        ));

        let start_timestamp = Instant::now();

        Self {
            renderer,
            window,
            current_frame,
            next_frame: None,
            frame_receiver: rx,
            start_timestamp,
        }
    }
}

struct App {
    args: Args,
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        self.state = Some(AppState::new(self.args.clone(), event_loop));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(AppState {
            window,
            renderer,
            current_frame,
            next_frame,
            frame_receiver,
            start_timestamp,
        }) = &mut self.state
        else {
            return;
        };
        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        ..
                    },
                ..
            }
            | WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                window.request_redraw();
                if next_frame.is_none() {
                    if let Ok(f) = frame_receiver.try_recv() {
                        *next_frame = Some(f);
                    }
                }

                let current_pts = Instant::now() - *start_timestamp;
                if let Some(next_frame_pts) = next_frame.as_ref().map(|f| f.pts) {
                    if next_frame_pts < current_pts {
                        *current_frame = next_frame.take().unwrap();
                    }
                }

                let _ = window.request_inner_size(PhysicalSize::new(
                    current_frame.frame.size().width,
                    current_frame.frame.size().height,
                ));

                renderer.render(&current_frame.frame, window);
            }

            WindowEvent::Resized(new_size) => renderer.resize(new_size),
            _ => {}
        }
    }
}

pub fn run() {
    let args = Args::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let mut app = App { args, state: None };

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).unwrap();
}
