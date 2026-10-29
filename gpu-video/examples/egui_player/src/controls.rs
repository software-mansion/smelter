use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, Frame, Id, Ui};

use crate::Playback;

pub struct Controls {
    pub paused: bool,
}

impl Controls {
    pub fn ui(&mut self, ui: &mut Ui, playback: Option<&Playback>) -> Option<PathBuf> {
        let label = if self.paused { "Play" } else { "Pause" };
        let mut picked_file = None;

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
                                picked_file = rfd::FileDialog::new()
                                    .add_filter("MP4", &["mp4"])
                                    .pick_file();
                            }
                            if ui.button(label).clicked() {
                                self.paused = !self.paused;
                            }
                            if let Some(playback) = playback
                                && let Some(current) = &playback.current_frame
                            {
                                ui.colored_label(
                                    Color32::WHITE,
                                    format!(
                                        "{:.2}s | frame {} | {}x{}",
                                        playback.time.as_secs_f64(),
                                        playback.frame_number,
                                        current.frame.width(),
                                        current.frame.height(),
                                    ),
                                );
                            }
                        });
                    });
            });

        picked_file
    }
}
