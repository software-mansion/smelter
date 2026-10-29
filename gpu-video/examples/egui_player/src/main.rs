use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

use eframe::egui;
use egui_wgpu::{WgpuConfiguration, WgpuSetupExisting, wgpu};
use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};

use controls::Controls;
use decoder::{Decoder, FrameWithPts};
use demuxer::Mp4Demuxer;
use video_renderer::{VideoCallback, VideoRenderer};

mod controls;
mod decoder;
mod demuxer;
mod video_renderer;

struct Playback {
    demuxer: Mp4Demuxer,
    decoder: Decoder,
    eof: bool,
    current_frame: Option<FrameWithPts>,
    frame_buffer: VecDeque<FrameWithPts>,
    frame_number: u64,
    time: Duration,
}

impl Playback {
    const FRAME_BUFFER_SIZE: usize = 3;

    fn new(demuxer: Mp4Demuxer, decoder: Decoder) -> Self {
        Self {
            demuxer,
            decoder,
            eof: false,
            current_frame: None,
            frame_buffer: VecDeque::new(),
            frame_number: 0,
            time: Duration::ZERO,
        }
    }

    fn advance(&mut self, elapsed: Duration) {
        self.time += elapsed;

        while self.frame_buffer.len() < Self::FRAME_BUFFER_SIZE {
            let Some(frame) = self.next_frame() else {
                break;
            };
            self.frame_buffer.push_back(frame);
        }

        if let Some(next_frame) = self.frame_buffer.pop_front_if(|f| f.pts <= self.time) {
            self.frame_number += 1;
            self.current_frame = Some(next_frame);
        }
    }

    fn next_frame(&mut self) -> Option<FrameWithPts> {
        loop {
            if let Some(frame) = self.decoder.try_frame() {
                return Some(frame);
            }

            if self.eof {
                return None;
            }

            match self.demuxer.next_chunk() {
                Some(chunk) => self.decoder.decode(chunk),
                None => {
                    self.decoder.flush();
                    self.eof = true;
                }
            }
        }
    }
}

struct App {
    device: wgpu::Device,
    queue: wgpu::Queue,
    controls: Controls,
    playback: Option<Playback>,
    last_tick: Instant,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc.wgpu_render_state.as_ref().unwrap();
        let video_renderer = VideoRenderer::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(video_renderer);

        Self {
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            controls: Controls { paused: false },
            playback: None,
            last_tick: Instant::now(),
        }
    }

    fn open_file(&mut self, path: PathBuf) {
        let demuxer = Mp4Demuxer::new(&path);
        let decoder = Decoder::new(&self.device, &self.queue);
        self.playback = Some(Playback::new(demuxer, decoder));
        self.last_tick = Instant::now();
    }

    fn advance_playback(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;

        if let Some(playback) = &mut self.playback
            && !self.controls.paused
        {
            playback.advance(elapsed);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.advance_playback();

        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                if let Some(current_frame) = self
                    .playback
                    .as_ref()
                    .and_then(|p| p.current_frame.as_ref())
                {
                    let frame = &current_frame.frame;
                    let available = ui.max_rect();
                    let scale = f32::min(
                        available.width() / frame.width() as f32,
                        available.height() / frame.height() as f32,
                    );
                    let rect = egui::Rect::from_center_size(
                        available.center(),
                        egui::vec2(frame.width() as f32, frame.height() as f32) * scale,
                    );

                    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                        rect,
                        VideoCallback {
                            frame: frame.clone(),
                        },
                    ));
                }

                if let Some(path) = self.controls.ui(ui, self.playback.as_ref()) {
                    self.open_file(path);
                }
            });

        ui.ctx().request_repaint();
    }
}

fn main() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("gpu-video egui player"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: WgpuConfiguration {
            wgpu_setup: WgpuSetupExisting {
                instance,
                adapter,
                device,
                queue,
            }
            .into(),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "gpu-video egui player",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .unwrap();
}
