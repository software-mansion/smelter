use std::collections::HashMap;

use compositor_api::types as api;
use compositor_render::{Frame, FrameData, FrameSet, OutputId, Resolution};
use js_sys::Object;
use tracing::error;
use wasm_bindgen::JsValue;

use super::{
    types::{self, to_js_error},
    wgpu::pad_to_256,
};

#[derive(Default)]
pub struct OutputDownloader {
    buffers: HashMap<OutputId, wgpu::Buffer>,
}

impl OutputDownloader {
    pub fn download_outputs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        outputs: FrameSet<OutputId>,
    ) -> Result<types::FrameSet, JsValue> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        for (id, frame) in outputs.frames.iter() {
            let FrameData::Rgba8UnormWgpuTexture(texture) = &frame.data else {
                panic!("Expected Rgba8UnormWgpuTexture");
            };

            let buffer = self
                .buffers
                .entry(id.clone())
                .or_insert_with(|| Self::create_buffer(device, frame.resolution));
            Self::ensure_buffer(buffer, device, frame.resolution);
            Self::copy_texture_to_buffer(texture, buffer, &mut encoder);
        }
        queue.submit(Some(encoder.finish()));

        let mut pending_downloads = Vec::with_capacity(outputs.frames.len());
        for (id, buffer) in self.buffers.iter_mut() {
            let (map_complete_sender, map_complete_receiver) = crossbeam_channel::bounded(1);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    if let Err(err) = map_complete_sender.send(result) {
                        error!("channel send error: {err}")
                    }
                });
            pending_downloads.push((id.clone(), map_complete_receiver));
        }

        device.poll(wgpu::Maintain::Wait);

        let output_data = js_sys::Map::new();
        for (id, map_complete_receiver) in pending_downloads {
            map_complete_receiver.recv().unwrap().map_err(to_js_error)?;
            let frame = outputs.frames.get(&id).unwrap();
            let buffer = self.buffers.get(&id).unwrap();
            let frame_object = Self::create_frame_object(frame, buffer)?;
            output_data.set(&id.to_string().into(), &frame_object);
            buffer.unmap();
        }

        Ok(types::FrameSet {
            pts_ms: outputs.pts.as_millis() as f64,
            frames: output_data,
        })
    }

    pub fn remove_output(&mut self, output_id: &OutputId) {
        self.buffers.remove(output_id);
    }

    fn create_frame_object(frame: &Frame, buffer: &wgpu::Buffer) -> Result<Object, JsValue> {
        let buffer_view = buffer.slice(..).get_mapped_range();
        let resolution = api::Resolution {
            width: frame.resolution.width,
            height: frame.resolution.height,
        };
        let format = match frame.data {
            FrameData::Rgba8UnormWgpuTexture(_) => types::FrameFormat::RgbaBytes,
            _ => return Err(JsValue::from_str("Unsupported output frame format")),
        };
        let mut data: Vec<u8> = Vec::with_capacity(4 * resolution.width * resolution.height);
        for chunk in buffer_view.chunks(pad_to_256(4 * resolution.width as u32) as usize) {
            data.extend(&chunk[..(4 * frame.resolution.width)]);
        }

        let frame = Object::new();
        frame.set("resolution", serde_wasm_bindgen::to_value(&resolution)?)?;
        frame.set("format", serde_wasm_bindgen::to_value(&format)?)?;
        frame.set("data", wasm_bindgen::Clamped(data))?;

        return Ok(frame);
    }

    fn create_buffer(device: &wgpu::Device, resolution: Resolution) -> wgpu::Buffer {
        let size = pad_to_256(4 * resolution.width as u32) * resolution.height as u32;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }

    fn ensure_buffer(buffer: &mut wgpu::Buffer, device: &wgpu::Device, resolution: Resolution) {
        let size = pad_to_256(4 * resolution.width as u32) * resolution.height as u32;
        if buffer.size() != size as u64 {
            *buffer = Self::create_buffer(device, resolution);
        }
    }

    fn copy_texture_to_buffer(
        texture: &wgpu::Texture,
        buffer: &wgpu::Buffer,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let size = texture.size();
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pad_to_256(4 * size.width)),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
    }
}

trait ObjectExt {
    fn set<T: Into<JsValue>>(&self, key: &str, value: T) -> Result<(), JsValue>;
}

impl ObjectExt for Object {
    fn set<T: Into<JsValue>>(&self, key: &str, value: T) -> Result<(), JsValue> {
        js_sys::Reflect::set(self, &JsValue::from_str(key), &value.into())?;
        Ok(())
    }
}
