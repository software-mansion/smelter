use std::{collections::HashMap, sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, FrameSet, InputId};
use futures::future::join_all;
use js_sys::Object;
use tracing::error;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::VideoFrameCopyToOptions;

use crate::types::ObjectExt;

use super::{
    types::{InputFrame, WgpuCtx},
    InputFrameKind, InputFrameSet,
};

pub struct RendererInputs {
    textures: HashMap<InputId, Arc<wgpu::Texture>>,
    use_copy_external: bool,
}

impl RendererInputs {
    pub fn new(use_copy_external: bool) -> Self {
        Self {
            textures: HashMap::default(),
            use_copy_external,
        }
    }

    pub async fn create_input_frames(
        &mut self,
        wgpu_ctx: &WgpuCtx,
        inputs: InputFrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        let pending_uploads = inputs
            .frames
            .into_iter()
            .map(|input| match &input.frame {
                InputFrameKind::VideoFrame(video_frame) => {
                    let size = video_frame.size();
                    let texture = self.ensure_texture(&input.id, &wgpu_ctx.device, size);
                    (input, texture)
                }
                InputFrameKind::HtmlVideoElement(video_element) => {
                    let size = video_element.size();
                    let texture = self.ensure_texture(&input.id, &wgpu_ctx.device, size);
                    (input, texture)
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(input, texture)| async {
                let input_id = input.id.clone();
                let frame =
                    RendererInputs::upload_frame(wgpu_ctx, input, texture, self.use_copy_external)
                        .await?;
                Ok((input_id, frame))
            });

        let mut frames = join_all(pending_uploads)
            .await
            .into_iter()
            .collect::<Result<HashMap<_, _>, JsValue>>()?;

        // TODO: MP4 are not calculated correctly, so we are resetting
        // them to the same value as set for now
        for (_, value) in frames.iter_mut() {
            value.pts = inputs.pts
        }

        Ok(FrameSet {
            frames,
            pts: inputs.pts,
        })
    }

    pub async fn upload_frame(
        wgpu_ctx: &WgpuCtx,
        input: InputFrame,
        texture: Arc<wgpu::Texture>,
        use_copy_external: bool,
    ) -> Result<Frame, JsValue> {
        let InputFrame { pts, frame, .. } = input;
        match frame {
            InputFrameKind::VideoFrame(video_frame) => match use_copy_external {
                true => Self::copy_direct_from_video_frame(wgpu_ctx, video_frame, texture, pts),
                false => {
                    Self::copy_via_cpu_from_video_frame(wgpu_ctx, video_frame, texture, pts).await
                }
            },
            InputFrameKind::HtmlVideoElement(element) => {
                Self::copy_from_video_element(wgpu_ctx, element, texture, pts)
            }
        }
    }

    fn copy_direct_from_video_frame(
        wgpu_ctx: &WgpuCtx,
        video_frame: web_sys::VideoFrame,
        texture: Arc<wgpu::Texture>,
        pts: Duration,
    ) -> Result<Frame, JsValue> {
        wgpu_ctx.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::VideoFrame(video_frame),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            texture.size(),
        );

        Ok(Frame {
            resolution: texture.size().into(),
            data: FrameData::Rgba8UnormWgpuTexture(texture),
            pts,
        })
    }

    async fn copy_via_cpu_from_video_frame(
        wgpu_ctx: &WgpuCtx,
        video_frame: web_sys::VideoFrame,
        texture: Arc<wgpu::Texture>,
        pts: Duration,
    ) -> Result<Frame, JsValue> {
        let size = video_frame.size();
        let frame_data = get_frame_data(video_frame).await?;

        wgpu_ctx.queue.write_texture(
            texture.as_image_copy(),
            &frame_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );

        Ok(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture),
            resolution: size.into(),
            pts,
        })
    }

    fn copy_from_video_element(
        wgpu_ctx: &WgpuCtx,
        video_element: web_sys::HtmlVideoElement,
        texture: Arc<wgpu::Texture>,
        pts: Duration,
    ) -> Result<Frame, JsValue> {
        wgpu_ctx.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLVideoElement(video_element),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: true,
            },
            texture.size(),
        );

        Ok(Frame {
            resolution: texture.size().into(),
            data: FrameData::Rgba8UnormWgpuTexture(texture),
            pts,
        })
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.textures.remove(input_id);
    }

    fn ensure_texture(
        &mut self,
        input_id: &InputId,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) -> Arc<wgpu::Texture> {
        let texture = self
            .textures
            .entry(input_id.clone())
            .or_insert_with(|| Self::create_texture(device, size));
        if size != texture.size() {
            *texture = Self::create_texture(device, size);
        }
        texture.clone()
    }

    fn create_texture(device: &wgpu::Device, size: wgpu::Extent3d) -> Arc<wgpu::Texture> {
        device
            .create_texture(&wgpu::TextureDescriptor {
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                label: None,
            })
            .into()
    }
}

async fn get_frame_data(frame: web_sys::VideoFrame) -> Result<Vec<u8>, JsValue> {
    let rect = frame.visible_rect().unwrap();

    let rgba_layout = Object::new();
    rgba_layout.set("offset", 0);
    rgba_layout.set("stride", rect.width() * 4.0);
    let rgba_layout = js_sys::Array::of1(&rgba_layout);

    let options = VideoFrameCopyToOptions::new();
    options.set("format", "RGBA");
    options.set_layout(&rgba_layout);

    let buffer_size = frame.allocation_size_with_options(&options)? as usize;
    let mut buffer = vec![0; buffer_size];
    let copy_promise = frame.copy_to_with_u8_slice_and_options(&mut buffer, &options);

    let plane_layouts = JsFuture::from(copy_promise).await?;
    if !check_plane_layouts(&rgba_layout, plane_layouts.dyn_ref().unwrap()) {
        error!(layouts = ?plane_layouts, frame = ?frame, "Copied frame's plane layouts do not match expected layouts")
    }
    Ok(buffer)
}

fn check_plane_layouts(expected: &js_sys::Array, received: &js_sys::Array) -> bool {
    if expected.length() != received.length() {
        return false;
    }

    use js_sys::Reflect::get;
    for (expected, received) in expected.iter().zip(received.iter()) {
        if get(&expected, &"offset".into()) != get(&received, &"offset".into()) {
            return false;
        }
        if get(&expected, &"stride".into()) != get(&received, &"stride".into()) {
            return false;
        }
    }

    return true;
}

trait ExtVideoFrame {
    fn size(&self) -> wgpu::Extent3d;
}

impl ExtVideoFrame for web_sys::VideoFrame {
    fn size(&self) -> wgpu::Extent3d {
        // `visible_rect` is `None` when frame is detached
        let rect = self.visible_rect().unwrap();
        wgpu::Extent3d {
            width: rect.width() as u32,
            height: rect.height() as u32,
            depth_or_array_layers: 1,
        }
    }
}

trait ExtHtmlVideoElement {
    fn size(&self) -> wgpu::Extent3d;
}

impl ExtHtmlVideoElement for web_sys::HtmlVideoElement {
    fn size(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.video_width(),
            height: self.video_height(),
            depth_or_array_layers: 1,
        }
    }
}
