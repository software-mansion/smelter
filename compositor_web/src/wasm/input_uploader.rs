use std::{collections::HashMap, sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, FrameSet, InputId};
use futures::future::join_all;
use js_sys::Object;
use tracing::error;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::VideoFrameCopyToOptions;

use super::types::{self, ObjectExt};

pub struct InputUploader {
    textures: HashMap<InputId, Arc<wgpu::Texture>>,
    use_copy_external: bool,
}

impl InputUploader {
    pub fn new(use_copy_external: bool) -> Self {
        Self {
            textures: HashMap::default(),
            use_copy_external,
        }
    }

    pub async fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: types::FrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        match self.use_copy_external {
            true => self.upload_with_copy_external(device, queue, input),
            false => self.upload_with_cpu_copy(device, queue, input).await,
        }
    }

    fn upload_with_copy_external(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: types::FrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        let pts = Duration::from_millis(input.pts_ms as u64);
        let mut frames = HashMap::with_capacity(input.frames.size() as usize);
        for frame in input.frames.entries() {
            // TODO: MP4 are not calculated correctly
            let types::InputFrame { id, frame, .. } = frame?.try_into()?;
            let size = size_from_video_frame(&frame);
            let texture = self.texture(&id, device, size);

            queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::VideoFrame(frame),
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

            frames.insert(
                id,
                Frame {
                    data: FrameData::Rgba8UnormWgpuTexture(texture),
                    resolution: size.into(),
                    pts,
                },
            );
        }

        Ok(FrameSet { frames, pts })
    }

    async fn upload_with_cpu_copy(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: types::FrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        let pts = Duration::from_millis(input.pts_ms as u64);
        let mut pending_frames = Vec::with_capacity(input.frames.size() as usize);
        for frame in input.frames.entries() {
            // TODO: MP4 are not calculated correctly
            let types::InputFrame { id, frame, .. } = frame?.try_into()?;
            let size = size_from_video_frame(&frame);
            pending_frames.push(async move { (id, get_frame_data(frame).await, size) });
        }

        let mut frames = HashMap::with_capacity(pending_frames.len());
        for (id, data, size) in join_all(pending_frames).await {
            let texture = self.texture(&id, device, size);
            queue.write_texture(
                texture.as_image_copy(),
                &data?,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * size.width),
                    rows_per_image: Some(size.height),
                },
                size,
            );

            frames.insert(
                id,
                Frame {
                    data: FrameData::Rgba8UnormWgpuTexture(texture),
                    resolution: size.into(),
                    pts,
                },
            );
        }

        Ok(FrameSet { frames, pts })
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.textures.remove(input_id);
    }

    fn texture(
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
    rgba_layout.set("offset", 0)?;
    rgba_layout.set("stride", rect.width() * 4.0)?;
    let rgba_layout = js_sys::Array::of1(&rgba_layout);

    let options = VideoFrameCopyToOptions::new();
    options.set("format", "RGBA")?;
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

fn size_from_video_frame(frame: &web_sys::VideoFrame) -> wgpu::Extent3d {
    // `visible_rect` is `None` when frame is detached
    let rect = frame.visible_rect().unwrap();
    wgpu::Extent3d {
        width: rect.width() as u32,
        height: rect.height() as u32,
        depth_or_array_layers: 1,
    }
}
