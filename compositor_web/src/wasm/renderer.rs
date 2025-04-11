use compositor_api::types as api;
use compositor_render::{
    InputId, OutputFrameFormat, OutputId, RegistryType, RendererId, RendererOptions, RendererSpec,
};
use glyphon::fontdb::Source;
use wasm_bindgen::JsValue;
use web_sys::OffscreenCanvasRenderingContext2d;

use super::{
    input_uploader::InputUploader, output_downloader::OutputDownloader, types, wgpu::WgpuContext,
};

pub(super) struct Renderer {
    renderer: compositor_render::Renderer,
    input_uploader: InputUploader,
    output_downloader: OutputDownloader,
    wgpu_ctx: WgpuContext,
}

impl Renderer {
    pub fn new(wgpu_ctx: WgpuContext, options: RendererOptions) -> Result<Self, JsValue> {
        let output_downloader = OutputDownloader::new(&wgpu_ctx);
        let (renderer, _) = compositor_render::Renderer::new(RendererOptions {
            wgpu_ctx: Some((wgpu_ctx.device.clone(), wgpu_ctx.queue.clone())),
            ..options
        })
        .map_err(types::to_js_error)?;
        let input_uploader = InputUploader::default();

        Ok(Self {
            renderer,
            input_uploader,
            output_downloader,
            wgpu_ctx,
        })
    }

    pub fn render(&mut self, input: types::FrameSet) -> Result<(), JsValue> {
        let frame_set = self.input_uploader.upload(&self.wgpu_ctx, input)?;
        let outputs = self
            .renderer
            .render(frame_set)
            .map_err(types::to_js_error)?;
        self.output_downloader
            .download_outputs(&self.wgpu_ctx, outputs)
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

    pub fn register_output(&mut self, output_id: String, ctx: OffscreenCanvasRenderingContext2d) {
        self.output_downloader
            .add_output(OutputId(output_id.into()), ctx);
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
