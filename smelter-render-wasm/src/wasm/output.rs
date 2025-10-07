use std::collections::HashMap;

use smelter_render::{Frame, FrameData, FrameSet, OutputId, Resolution};
use tracing::error;
use wasm_bindgen::JsValue;

use super::{
    OutputFrame,
    types::{WgpuCtx, to_js_error},
    wgpu::pad_to_256,
};

#[derive(Default)]
pub struct RendererOutputs {
    buffers: HashMap<OutputId, wgpu::Buffer>,
}

impl RendererOutputs {
    pub fn process_output_frames(
        &mut self,
        wgpu_ctx: &WgpuCtx,
        outputs: FrameSet<OutputId>,
    ) -> Result<Vec<OutputFrame>, JsValue> {
        let mut encoder = wgpu_ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for (id, frame) in outputs.frames.iter() {
            let FrameData::Rgba8UnormWgpuTexture(texture) = &frame.data else {
                panic!("Expected Rgba8UnormWgpuTexture");
            };

            let buffer = self
                .buffers
                .entry(id.clone())
                .or_insert_with(|| Self::create_buffer(wgpu_ctx, frame.resolution));
            Self::ensure_buffer(wgpu_ctx, buffer, frame.resolution);
            Self::copy_texture_to_buffer(texture, buffer, &mut encoder);
        }
        wgpu_ctx.queue.submit(Some(encoder.finish()));

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

        wgpu_ctx.device.poll(wgpu::PollType::Wait).unwrap();

        let mut output_data = vec![];
        for (id, map_complete_receiver) in pending_downloads {
            map_complete_receiver.recv().unwrap().map_err(to_js_error)?;
            let frame = outputs.frames.get(&id).unwrap();
            let buffer = self.buffers.get(&id).unwrap();
            output_data.push(Self::create_frame_object(&id, frame, buffer)?);
            buffer.unmap();
        }

        Ok(output_data)
    }

    pub fn remove_output(&mut self, output_id: &OutputId) {
        self.buffers.remove(output_id);
    }

    fn create_frame_object(
        output_id: &OutputId,
        frame: &Frame,
        buffer: &wgpu::Buffer,
    ) -> Result<OutputFrame, JsValue> {
        let buffer_view = buffer.slice(..).get_mapped_range();
        let resolution = Resolution {
            width: frame.resolution.width,
            height: frame.resolution.height,
        };
        let mut data: Vec<u8> = Vec::with_capacity(4 * resolution.width * resolution.height);
        for chunk in buffer_view.chunks(pad_to_256(4 * resolution.width as u32) as usize) {
            data.extend(&chunk[..(4 * frame.resolution.width)]);
        }

        return Ok(OutputFrame {
            output_id: output_id.clone(),
            resolution,
            data: wasm_bindgen::Clamped(data),
        });
    }

    fn create_buffer(wgpu_ctx: &WgpuCtx, resolution: Resolution) -> wgpu::Buffer {
        let size = pad_to_256(4 * resolution.width as u32) * resolution.height as u32;
        wgpu_ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }

    fn ensure_buffer(wgpu_ctx: &WgpuCtx, buffer: &mut wgpu::Buffer, resolution: Resolution) {
        let size = pad_to_256(4 * resolution.width as u32) * resolution.height as u32;
        if buffer.size() != size as u64 {
            *buffer = Self::create_buffer(wgpu_ctx, resolution);
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
