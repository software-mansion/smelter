use std::{collections::HashMap, sync::Arc, time::Duration};

use bytes::Bytes;
use compositor_render::{Frame, FrameData, FrameSet, InputId, YuvPlanes};
use wasm_bindgen::JsValue;

use super::types;

#[derive(Default)]
pub struct InputUploader {
    textures: HashMap<InputId, Texture>,
}

impl InputUploader {
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: types::FrameSet,
    ) -> Result<FrameSet<InputId>, JsValue> {
        let pts = Duration::from_millis(input.pts_ms as u64);
        let mut frames = HashMap::new();
        for frame in input.frames.entries() {
            let frame: types::Frame = frame?.try_into()?;
            self.upload_input_frame(device, queue, &frame);

            let data = match frame.format {
                types::FrameFormat::RgbaBytes => FrameData::Rgba8UnormWgpuTexture(
                    self.textures.get(&frame.id).unwrap().texture.clone(),
                ),
                types::FrameFormat::YuvBytes => {
                    FrameData::PlanarYuv420(Self::create_yuv_planes(&frame))
                }
            };

            frames.insert(
                frame.id,
                Frame {
                    data,
                    resolution: frame.resolution,
                    pts,
                },
            );
        }

        Ok(FrameSet { frames, pts })
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.textures.remove(input_id);
    }

    fn upload_input_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &types::Frame,
    ) {
        match frame.format {
            types::FrameFormat::RgbaBytes => {
                let size = wgpu::Extent3d {
                    width: frame.resolution.width as u32,
                    height: frame.resolution.height as u32,
                    depth_or_array_layers: 1,
                };
                let texture = self
                    .textures
                    .entry(frame.id.clone())
                    .or_insert_with(|| Self::create_texture(device, frame));
                if size != texture.size {
                    *texture = Self::create_texture(device, frame);
                }

                queue.write_texture(
                    texture.texture.as_image_copy(),
                    &frame.data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * size.width),
                        rows_per_image: Some(size.height),
                    },
                    size,
                );
            }
            types::FrameFormat::YuvBytes => {}
        }
    }

    fn create_texture(device: &wgpu::Device, frame: &types::Frame) -> Texture {
        let size = wgpu::Extent3d {
            width: frame.resolution.width as u32,
            height: frame.resolution.height as u32,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
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
            label: Some(&format!("Input texture: {}", frame.id)),
        });

        Texture {
            size,
            texture: Arc::new(texture),
        }
    }

    fn create_yuv_planes(frame: &types::Frame) -> YuvPlanes {
        let width = frame.resolution.width;
        let height = frame.resolution.height;

        let y_plane_len = width * height;
        let uv_planes_len = (width * height) / 4;
        let y_plane = &frame.data[..y_plane_len];
        let u_plane = &frame.data[y_plane_len..(y_plane_len + uv_planes_len)];
        let v_plane = &frame.data[(y_plane_len + uv_planes_len)..];

        YuvPlanes {
            y_plane: Bytes::copy_from_slice(y_plane),
            u_plane: Bytes::copy_from_slice(u_plane),
            v_plane: Bytes::copy_from_slice(v_plane),
        }
    }
}

struct Texture {
    size: wgpu::Extent3d,
    texture: Arc<wgpu::Texture>,
}
