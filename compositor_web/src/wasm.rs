use std::sync::{Arc, Mutex};

use bytes::Bytes;
use compositor_api::types::{Component, ImageSpec, Resolution, ShaderSpec};
use compositor_render::{
    image::{ImageSource, ImageType},
    RegistryType, RendererSpec,
};
use glyphon::fontdb::Source;
use tracing_subscriber::{layer::SubscriberExt, Registry};
use tracing_wasm::WASMLayer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvasRenderingContext2d;
use wgpu::WgpuContext;

mod input_uploader;
mod output_downloader;
mod renderer;
mod types;
mod wgpu;

// Executed during WASM module init
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    Ok(())
}

#[wasm_bindgen]
pub async fn create_renderer(options: JsValue) -> Result<SmelterRenderer, JsValue> {
    let options = types::from_js_value::<types::RendererOptions>(options)?;

    let mut logger_config = tracing_wasm::WASMLayerConfigBuilder::new();
    logger_config.set_max_level(options.logger_level.into());
    let _ = tracing::subscriber::set_global_default(
        Registry::default().with(WASMLayer::new(logger_config.build())),
    );

    let wgpu_ctx = WgpuContext::new().await?;
    let renderer = renderer::Renderer::new(wgpu_ctx, options.into())?;
    Ok(SmelterRenderer(Mutex::new(renderer)))
}

#[wasm_bindgen]
pub struct SmelterRenderer(Mutex<renderer::Renderer>);

#[wasm_bindgen]
impl SmelterRenderer {
    pub fn render(&self, input: types::FrameSet) -> Result<(), JsValue> {
        let mut renderer = self.0.lock().unwrap();
        renderer.render(input)
    }

    pub fn update_scene(
        &self,
        output_id: String,
        resolution: JsValue,
        scene: JsValue,
    ) -> Result<(), JsValue> {
        let resolution = types::from_js_value::<Resolution>(resolution)?;
        let scene = types::from_js_value::<Component>(scene)?;

        let mut renderer = self.0.lock().unwrap();
        renderer.update_scene(output_id, resolution, scene)
    }

    pub fn register_input(&self, input_id: String) {
        let mut renderer = self.0.lock().unwrap();
        renderer.register_input(input_id)
    }

    pub fn register_output(&self, output_id: String, ctx: OffscreenCanvasRenderingContext2d) {
        let mut renderer = self.0.lock().unwrap();
        renderer.register_output(output_id, ctx);
    }

    pub async fn register_image(
        &self,
        renderer_id: String,
        image_spec: JsValue,
    ) -> Result<(), JsValue> {
        let image_spec = types::from_js_value::<ImageSpec>(image_spec)?;
        let (url, image_type) = match image_spec {
            ImageSpec::Png { url, .. } => (url, ImageType::Png),
            ImageSpec::Jpeg { url, .. } => (url, ImageType::Jpeg),
            ImageSpec::Svg {
                url, resolution, ..
            } => (
                url,
                ImageType::Svg {
                    resolution: resolution.map(Into::into),
                },
            ),
            ImageSpec::Gif { url, .. } => (url, ImageType::Gif),
        };
        let Some(url) = url else {
            return Err(JsValue::from_str("Expected `url` field in image spec"));
        };

        let bytes = download(&url).await?;
        let image_spec = compositor_render::image::ImageSpec {
            src: ImageSource::Bytes { bytes },
            image_type,
        };

        let mut renderer = self.0.lock().unwrap();
        renderer
            .register_renderer(renderer_id, RendererSpec::Image(image_spec))
            .await
    }

    pub async fn register_shader(
        &self,
        shader_id: String,
        shader_spec: JsValue,
    ) -> Result<(), JsValue> {
        let shader_spec = types::from_js_value::<ShaderSpec>(shader_spec)?;
        let mut renderer = self.0.lock().unwrap();
        renderer
            .register_renderer(
                shader_id,
                shader_spec.try_into().map_err(types::to_js_error)?,
            )
            .await
    }

    pub async fn register_font(&self, font_url: String) -> Result<(), JsValue> {
        let bytes = download(&font_url).await?;
        let mut renderer = self.0.lock().unwrap();
        renderer
            .register_font(Source::Binary(Arc::new(bytes)))
            .await;

        Ok(())
    }

    pub fn unregister_input(&self, input_id: String) {
        let mut renderer = self.0.lock().unwrap();
        renderer.unregister_input(input_id)
    }

    pub fn unregister_output(&self, output_id: String) {
        let mut renderer = self.0.lock().unwrap();
        renderer.unregister_output(output_id)
    }

    pub fn unregister_image(&self, renderer_id: String) -> Result<(), JsValue> {
        let mut renderer = self.0.lock().unwrap();
        renderer.unregister_renderer(renderer_id, RegistryType::Image)
    }

    pub fn unregister_shader(&self, renderer_id: String) -> Result<(), JsValue> {
        let mut renderer = self.0.lock().unwrap();
        renderer.unregister_renderer(renderer_id, RegistryType::Shader)
    }
}

async fn download(url: &str) -> Result<Bytes, JsValue> {
    let resp = reqwest::get(url).await.map_err(types::to_js_error)?;
    let resp = resp.error_for_status().map_err(types::to_js_error)?;
    resp.bytes().await.map_err(types::to_js_error)
}
