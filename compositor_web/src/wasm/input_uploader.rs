use std::{collections::HashMap, sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, FrameSet, InputId};
use wasm_bindgen::JsValue;

use super::types;

#[derive(Default)]
pub struct InputUploader {
    textures: HashMap<InputId, Arc<wgpu::Texture>>,
}

impl InputUploader {
    pub fn upload(
        &mut self,
        wgpu_ctx: &super::WgpuContext,
        input: types::FrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        let pts = Duration::from_millis(input.pts_ms as u64);
        let mut frames = HashMap::new();
        for frame in input.frames.entries() {
            // TODO: MP4 are not calculated correctly
            let types::InputFrame { id, frame, .. } = frame?.try_into()?;
            let size = wgpu::Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            };

            let texture = self.texture(&id, &wgpu_ctx.device, size);
            wgpu_ctx.queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::ImageBitmap(frame),
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
