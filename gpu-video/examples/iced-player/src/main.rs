use std::{sync::Arc, time::Instant};

use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};
use iced_wgpu::{
    Engine, Renderer,
    graphics::{Shell, Viewport},
    wgpu,
};
use iced_widget::{Widget, button, container, row, shader, stack, text};
use iced_winit::{
    conversion,
    core::{Alignment, Color, Event, Length::Fill, Size, Theme, mouse, renderer, shell, window},
    runtime::user_interface::{self, UserInterface},
    winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        keyboard::ModifiersState,
        window::{Window, WindowAttributes, WindowId},
    },
};

use playback::Playback;
use video_view::VideoView;

mod demuxer;
mod playback;
mod video_view;

#[derive(Debug, Clone)]
enum Message {
    Open,
    TogglePause,
}

struct App {
    device: wgpu::Device,
    queue: wgpu::Queue,
    video_view: VideoView,
    paused: bool,
    playback: Option<Playback>,
    last_tick: Instant,
}

impl App {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            video_view: VideoView::new(&device),
            device,
            queue,
            paused: false,
            playback: None,
            last_tick: Instant::now(),
        }
    }

    fn open_file(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("MP4", &["mp4"])
            .pick_file();
        let Some(path) = picked else {
            return;
        };

        self.playback = Some(Playback::new(&self.device, &self.queue, &path));
        self.last_tick = Instant::now();
    }

    fn advance_playback(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;

        if let Some(playback) = &mut self.playback
            && !self.paused
            && let Some(frame) = playback.next_frame(elapsed)
        {
            self.video_view.update(&self.device, &self.queue, &frame);
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Open => self.open_file(),
            Message::TogglePause => self.paused = !self.paused,
        }
    }

    fn view(&self) -> impl Widget<Message, Theme, Renderer> {
        let status = self
            .playback
            .as_ref()
            .zip(self.video_view.size())
            .map(|(playback, size)| {
                text!(
                    "{:.2}s | frame {} | {}x{}",
                    playback.time.as_secs_f64(),
                    playback.frame_number,
                    size.width,
                    size.height,
                )
                .color(Color::WHITE)
            });

        let controls = container(
            row![
                button("Open").on_press(Message::Open),
                button(if self.paused { "Play" } else { "Pause" }).on_press(Message::TogglePause),
                status,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(10)
        .style(|_| container::background(Color::from_rgba8(0, 0, 0, 0.6)));

        stack![
            shader(&self.video_view).width(Fill).height(Fill),
            container(controls)
                .align_left(Fill)
                .align_bottom(Fill)
                .padding(10),
        ]
    }
}

struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    renderer: Renderer,
    viewport: Viewport,
    cursor: mouse::Cursor,
    modifiers: ModifiersState,
    cache: user_interface::Cache,
    events: Vec<Event>,
    resized: bool,
}

impl WindowState {
    fn new(
        event_loop: &ActiveEventLoop,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        app: &App,
    ) -> Self {
        let window = Arc::new(
            event_loop
                .create_window(WindowAttributes::default().with_title("gpu-video iced player"))
                .unwrap(),
        );
        let surface = instance.create_surface(window.clone()).unwrap();
        let capabilities = surface.get_capabilities(adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);

        let engine = Engine::new(
            adapter,
            app.device.clone(),
            app.queue.clone(),
            format,
            None,
            Shell::headless(),
        );
        let renderer = Renderer::new(engine, renderer::Settings::default());

        let mut state = Self {
            viewport: Viewport::with_physical_size(Size::new(1, 1), renderer::Scale::default()),
            window,
            surface,
            format,
            renderer,
            cursor: mouse::Cursor::Unavailable,
            modifiers: ModifiersState::default(),
            cache: user_interface::Cache::new(),
            events: Vec::new(),
            resized: false,
        };
        state.resize(app);
        state
    }

    fn resize(&mut self, app: &App) {
        let size = self.window.inner_size();
        self.viewport = Viewport::with_physical_size(
            Size::new(size.width, size.height),
            renderer::Scale {
                window: self.window.scale_factor() as f32,
                application: 1.0,
            },
        );
        self.surface.configure(
            &app.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
                color_space: wgpu::SurfaceColorSpace::Auto,
            },
        );
        self.resized = false;
    }

    fn redraw(&mut self, app: &mut App) {
        app.advance_playback();

        if self.resized {
            self.resize(app);
        }

        let wgpu::CurrentSurfaceTexture::Success(frame) = self.surface.get_current_texture() else {
            self.window.request_redraw();
            return;
        };
        let view = frame.texture.create_view(&Default::default());

        let mut interface = UserInterface::build(
            app.view(),
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let (state, _) = interface.update(
            &self.window,
            &shell::Waker::noop(),
            &[Event::Window(
                window::Event::RedrawRequested(Instant::now()),
            )],
            self.cursor,
            &mut self.renderer,
            &mut shell::Bus::new(),
        );
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            match conversion::mouse_interaction(mouse_interaction) {
                Some(icon) => {
                    self.window.set_cursor(icon);
                    self.window.set_cursor_visible(true);
                }
                None => self.window.set_cursor_visible(false),
            }
        }
        interface.draw(
            &mut self.renderer,
            &Theme::Dark,
            &renderer::Style::default(),
            self.cursor,
        );
        self.cache = interface.into_cache();

        self.renderer.present(
            Some(Color::BLACK),
            frame.texture.format(),
            &view,
            &self.viewport,
        );
        app.queue.present(frame);

        self.window.request_redraw();
    }

    fn process_events(&mut self, app: &mut App) {
        if self.events.is_empty() {
            return;
        }

        let mut interface = UserInterface::build(
            app.view(),
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = shell::Bus::new();
        let _ = interface.update(
            &self.window,
            &shell::Waker::noop(),
            &self.events,
            self.cursor,
            &mut self.renderer,
            &mut messages,
        );
        self.events.clear();
        self.cache = interface.into_cache();

        for message in messages {
            app.update(message);
        }
    }
}

struct Runner {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    app: App,
    window: Option<WindowState>,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.window = Some(WindowState::new(
                event_loop,
                &self.instance,
                &self.adapter,
                &self.app,
            ));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.window else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => state.redraw(&mut self.app),
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = mouse::Cursor::Available(conversion::cursor_position(
                    position,
                    state.viewport.scale_factor(),
                ));
            }
            WindowEvent::ModifiersChanged(modifiers) => state.modifiers = modifiers.state(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                state.resized = true;
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }

        if let Some(event) =
            conversion::window_event(event, state.viewport.scale_factor(), state.modifiers)
        {
            state.events.push(event);
        }
        state.process_events(&mut self.app);
    }
}

fn main() {
    tracing_subscriber::fmt().init();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut runner = Runner {
        instance,
        adapter,
        app: App::new(device, queue),
        window: None,
    };
    event_loop.run_app(&mut runner).unwrap();
}
