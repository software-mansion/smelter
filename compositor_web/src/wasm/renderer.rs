use std::sync::Arc;

use compositor_api::types as api;
use compositor_render::{
    image::ImageSpec, shader::ShaderSpec, InputId, OutputFrameFormat, OutputId, RegistryType,
    RendererId, RendererOptions, RendererSpec,
};
use glyphon::fontdb::Source;
use wasm_bindgen::JsValue;

use super::{input_uploader::InputUploader, output_downloader::OutputDownloader, types};

pub(super) struct Renderer {
    renderer: compositor_render::Renderer,
    input_uploader: InputUploader,
    output_downloader: OutputDownloader,
}

impl Renderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        options: RendererOptions,
    ) -> Result<Self, JsValue> {
        let (renderer, _) = compositor_render::Renderer::new(RendererOptions {
            wgpu_ctx: Some((device, queue)),
            ..options
        })
        .map_err(types::to_js_error)?;
        let input_uploader = InputUploader::default();
        let output_downloader = OutputDownloader::default();

        Ok(Self {
            renderer,
            input_uploader,
            output_downloader,
        })
    }

    pub fn render(&mut self, input: types::FrameSet) -> Result<types::FrameSet, JsValue> {
        let (device, queue) = self.renderer.wgpu_ctx();
        let frame_set = self.input_uploader.upload(&device, &queue, input)?;

        let outputs = self
            .renderer
            .render(frame_set)
            .map_err(types::to_js_error)?;
        self.output_downloader
            .download_outputs(&device, &queue, outputs)
    }

    pub fn update_scene(
        &mut self,
        output_id: String,
        resolution: api::Resolution,
        scene: api::Component,
    ) -> Result<(), JsValue> {
        self.renderer
            .update_scene(
                OutputId(output_id.into()),
                resolution.into(),
                OutputFrameFormat::RgbaWgpuTexture,
                scene.try_into().map_err(types::to_js_error)?,
            )
            .map_err(types::to_js_error)
    }

    pub fn register_input(&mut self, input_id: String) {
        self.renderer.register_input(InputId(input_id.into()));
    }

    pub async fn register_renderer(
        &mut self,
        renderer_id: String,
        spec: RendererSpec,
    ) -> Result<(), JsValue> {
        self.renderer
            .register_renderer(RendererId(renderer_id.into()), spec)
            .map_err(types::to_js_error)
    }

    pub async fn register_font(&mut self, font: Source) {
        self.renderer.register_font(font);
    }

    pub fn unregister_input(&mut self, input_id: String) {
        let input_id = InputId(input_id.into());
        self.renderer.unregister_input(&input_id);
        self.input_uploader.remove_input(&input_id);
    }

    pub fn unregister_output(&mut self, output_id: String) {
        let output_id = OutputId(output_id.into());
        self.renderer.unregister_output(&output_id);
        self.output_downloader.remove_output(&output_id);
    }

    pub fn unregister_renderer(
        &mut self,
        renderer_id: String,
        registry: RegistryType,
    ) -> Result<(), JsValue> {
        self.renderer
            .unregister_renderer(&RendererId(renderer_id.into()), registry)
            .map_err(types::to_js_error)
    }
}
