use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Frame, Id};
use egui_wgpu::{WgpuConfiguration, WgpuSetupExisting, wgpu};
use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};

use playback::Playback;
use video_view::VideoView;

mod demuxer;
mod playback;
mod video_view;

struct App {
    render_state: egui_wgpu::RenderState,
    video_view: VideoView,
    paused: bool,
    playback: Option<Playback>,
    last_tick: Instant,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc.wgpu_render_state.clone().unwrap();

        Self {
            video_view: VideoView::new(&render_state.device),
            render_state,
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

        let state = &self.render_state;
        self.playback = Some(Playback::new(&state.device, &state.queue, &path));
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
            self.video_view.update(&self.render_state, &frame);
        }
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        egui::Area::new(Id::unique("controls"))
            .anchor(Align2::LEFT_BOTTOM, [10.0, -10.0])
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(Color32::from_black_alpha(153))
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;

                            if ui.button("Open").clicked() {
                                self.open_file();
                            }

                            let label = if self.paused { "Play" } else { "Pause" };
                            if ui.button(label).clicked() {
                                self.paused = !self.paused;
                            }

                            if let Some(playback) = &self.playback
                                && let Some(size) = self.video_view.size()
                            {
                                ui.colored_label(
                                    Color32::WHITE,
                                    format!(
                                        "{:.2}s | frame {} | {}x{}",
                                        playback.time.as_secs_f64(),
                                        playback.frame_number,
                                        size.width,
                                        size.height,
                                    ),
                                );
                            }
                        });
                    });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.advance_playback();

        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                self.video_view.ui(ui);
                self.controls_ui(ui);
            });

        ui.ctx().request_repaint();
    }
}

pub fn run() {
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
