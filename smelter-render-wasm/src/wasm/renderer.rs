use glyphon::fontdb::Source;
use smelter_api as api;
use smelter_render::{
    InputId, OutputFrameFormat, OutputId, RegistryType, RendererId, RendererOptions, RendererSpec,
};
use wasm_bindgen::JsValue;

use super::{
    InputFrameSet,
    input::RendererInputs,
    output::RendererOutputs,
    types::{self, OutputFrameSet, WgpuCtx},
};

pub(super) struct Renderer {
    renderer: smelter_render::Renderer,
    inputs: RendererInputs,
    outputs: RendererOutputs,
}

impl Renderer {
    pub fn new(
        upload_frames_with_copy_external: bool,
        options: RendererOptions,
    ) -> Result<Self, JsValue> {
        let renderer = smelter_render::Renderer::new(options).map_err(types::to_js_error)?;
        let inputs = RendererInputs::new(upload_frames_with_copy_external);
        let outputs = RendererOutputs::default();

        Ok(Self {
            renderer,
            inputs,
            outputs,
        })
    }

    pub async fn render(&mut self, inputs: InputFrameSet) -> Result<OutputFrameSet, JsValue> {
        let ctx = self.wgpu_ctx();
        let pts = inputs.pts;
        let frame_set = self.inputs.create_input_frames(&ctx, inputs).await?;

        let outputs = self
            .renderer
            .render(frame_set)
            .map_err(types::to_js_error)?;
        let output_frames = self.outputs.process_output_frames(&ctx, outputs)?;
        Ok(OutputFrameSet {
            pts,
            frames: output_frames,
        })
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
        self.inputs.remove_input(&input_id);
    }

    pub fn unregister_output(&mut self, output_id: String) {
        let output_id = OutputId(output_id.into());
        self.renderer.unregister_output(&output_id);
        self.outputs.remove_output(&output_id);
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

    fn wgpu_ctx(&self) -> WgpuCtx {
        let (device, queue) = self.renderer.wgpu_ctx();
        WgpuCtx {
            device: device.as_ref().clone(),
            queue: queue.as_ref().clone(),
        }
    }
}
