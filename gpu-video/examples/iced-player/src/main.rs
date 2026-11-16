use std::time::Duration;

use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};
use iced::time::Instant;
use iced::widget::{button, container, row, shader, stack, text};
use iced::{Center, Color, Fill, Widget};
use iced_custom_wgpu::{WgpuContext, wgpu};

use playback::Playback;
use video_view::VideoView;

#[path = "../../egui-player/src/demuxer.rs"]
mod demuxer;
mod playback;
mod video_view;

#[derive(Debug, Clone)]
enum Message {
    Open,
    TogglePause,
    Tick(Instant),
}

struct App {
    device: wgpu::Device,
    queue: wgpu::Queue,
    video_view: VideoView,
    paused: bool,
    playback: Option<Playback>,
    last_tick: Option<Instant>,
}

impl App {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            video_view: VideoView::new(device, queue),
            paused: false,
            playback: None,
            last_tick: None,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Open => self.open_file(),
            Message::TogglePause => self.paused = !self.paused,
            Message::Tick(now) => self.advance_playback(now),
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
        self.last_tick = None;
    }

    fn advance_playback(&mut self, now: Instant) {
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |last_tick| now - last_tick);
        self.last_tick = Some(now);

        if let Some(playback) = &mut self.playback
            && !self.paused
            && let Some(frame) = playback.next_frame(elapsed)
        {
            self.video_view.update(&frame);
        }
    }

    fn view(&self) -> impl Widget<Message> + '_ {
        let label = if self.paused { "Play" } else { "Pause" };
        let status = match (&self.playback, self.video_view.size()) {
            (Some(playback), Some(size)) => format!(
                "{:.2}s | frame {} | {}x{}",
                playback.time.as_secs_f64(),
                playback.frame_number,
                size.width,
                size.height,
            ),
            _ => String::new(),
        };
        let controls = row![
            button("Open").on_press(Message::Open),
            button(label).on_press(Message::TogglePause),
            text(status).color(Color::WHITE),
        ]
        .spacing(10)
        .align_y(Center);

        let controls = container(container(controls).padding(10).style(|_| {
            container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.6))
        }))
        .padding(10)
        .width(Fill)
        .align_bottom(Fill);

        container(stack![
            shader(&self.video_view).width(Fill).height(Fill),
            controls
        ])
        .style(|_| container::Style::default().background(Color::BLACK))
    }
}

fn main() -> iced::Result {
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

    let context = WgpuContext {
        instance,
        adapter,
        device: device.clone(),
        queue: queue.clone(),
    };

    iced_custom_wgpu::application(move || App::new(&device, &queue), App::update, App::view)
        .title("gpu-video iced player")
        .run(context)
}
