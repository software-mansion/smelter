use eframe::egui;
use egui_wgpu::{RenderState, wgpu};
use gpu_video::{
    OutputFrame, VideoTexture, WgpuNv12ToRgbaConverter,
    parameters::{ColorRange, ColorSpace, WgpuConverterParameters},
};

struct OutputTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    id: egui::TextureId,
}

pub struct VideoView {
    converter: WgpuNv12ToRgbaConverter,
    texture: Option<OutputTexture>,
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

    fn output_texture(
        &mut self,
        state: &RenderState,
        frame: &OutputFrame<VideoTexture>,
    ) -> wgpu::TextureView {
        let size = frame.data.size();
        if let Some(output) = &self.texture
            && output.texture.size() == size
        {
            return output.view.clone();
        }

        let texture = state.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let mut renderer = state.renderer.write();
        if let Some(output) = self.texture.take() {
            renderer.free_texture(&output.id);
        }
        let id = renderer.register_native_texture(&state.device, &view, wgpu::FilterMode::Linear);
        self.texture = Some(OutputTexture {
            texture,
            view: view.clone(),
            id,
        });

        view
    }

    pub fn update(&mut self, state: &RenderState, frame: &OutputFrame<VideoTexture>) {
        let view = self.output_texture(state, frame);
        let bind_group = self.converter.create_input_bind_group(frame).unwrap();
        let mut encoder = state.device.create_command_encoder(&Default::default());
        self.converter.convert(&mut encoder, &bind_group, &view);
        state.queue.submit([encoder.finish()]);
    }

    pub fn size(&self) -> Option<wgpu::Extent3d> {
        self.texture.as_ref().map(|output| output.texture.size())
    }

    pub fn ui(&self, ui: &mut egui::Ui) {
        if let Some(output) = &self.texture {
            let size = egui::vec2(
                output.texture.width() as f32,
                output.texture.height() as f32,
            );
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new((output.id, size)).shrink_to_fit());
            });
        }
    }
}
