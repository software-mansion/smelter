use eframe::egui;
use egui_wgpu::{RenderState, wgpu};
use gpu_video::{
    OutputFrame, VideoTexture, WgpuNv12ToRgbaConverter,
    parameters::{ColorRange, ColorSpace, WgpuConverterParameters},
};

pub struct VideoView {
    converter: WgpuNv12ToRgbaConverter,
    texture: Option<(wgpu::Texture, egui::TextureId)>,
}

impl VideoView {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            converter: WgpuNv12ToRgbaConverter::new(
                device,
                WgpuConverterParameters {
                    color_space: ColorSpace::BT709,
                    color_range: ColorRange::Limited,
                },
            )
            .unwrap(),
            texture: None,
        }
    }

    pub fn update(&mut self, render_state: &RenderState, frame: &OutputFrame<VideoTexture>) {
        let device = &render_state.device;
        let size = wgpu::Extent3d {
            width: frame.data.width(),
            height: frame.data.height(),
            depth_or_array_layers: 1,
        };

        if self.texture.as_ref().is_none_or(|(t, _)| t.size() != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());

            let mut renderer = render_state.renderer.write();
            let id = match &self.texture {
                Some((_, id)) => {
                    renderer.update_egui_texture_from_wgpu_texture(
                        device,
                        &view,
                        wgpu::FilterMode::Linear,
                        *id,
                    );
                    *id
                }
                None => renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            };
            self.texture = Some((texture, id));
        }

        let (texture, _) = self.texture.as_ref().unwrap();
        let bind_group = self.converter.create_input_bind_group(frame).unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        self.converter.convert(
            &mut encoder,
            &bind_group,
            &texture.create_view(&Default::default()),
        );
        render_state.queue.submit([encoder.finish()]);
    }

    pub fn ui(&self, ui: &mut egui::Ui) {
        if let Some((texture, id)) = &self.texture {
            let size = egui::vec2(texture.width() as f32, texture.height() as f32);
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new((*id, size)).shrink_to_fit());
            });
        }
    }
}
