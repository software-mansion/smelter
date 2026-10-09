use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use objc2::rc::Retained;
use wgpu::hal::{Device as _, Queue as _, metal::Api as MtlApi};

use crate::{
    EncodedOutputChunk, InputFrame, VideoEncoderError, VideoTexture,
    backends::video_toolbox::{
        error::{VTEncoderError, VTInitError},
        metal_interop::SendSyncCVBuffer,
        wgpu_api::video_texture_from_pixel_buffer,
    },
    device::{EncoderOutputParameters, VideoParameters},
    encoders::{EncodeTexture, WgpuTextureEncoderError, WgpuVideoEncoderBackend},
};

use super::{EncodeCodec, VTEncoder};

pub(crate) struct VTWgpuEncodeState {
    fence: wgpu::hal::metal::Fence,
    next_fence_value: u64,
    issued_input_textures: Arc<Mutex<HashMap<VideoTexture, SendSyncCVBuffer>>>,
}

impl VTWgpuEncodeState {
    fn new(wgpu_device: &wgpu::Device) -> Result<Self, VTEncoderError> {
        let fence = {
            let hal_device =
                unsafe { wgpu_device.as_hal::<MtlApi>() }.ok_or(VTInitError::NotMetalBackend)?;
            unsafe { hal_device.create_fence() }.map_err(WgpuTextureEncoderError::from)?
        };

        if fence.raw_shared_event().is_none() {
            // TODO: may need a fallback sometimes?
            return Err(VTEncoderError::SharedEventUnavailable);
        }

        Ok(Self {
            fence,
            next_fence_value: 1,
            issued_input_textures: Default::default(),
        })
    }
}

impl<C: EncodeCodec> VTEncoder<C> {
    pub(crate) fn new_wgpu(
        wgpu_device: &wgpu::Device,
        input_parameters: VideoParameters,
        output_parameters: EncoderOutputParameters<C::Profile>,
        max_in_flight_submissions: u32,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
    ) -> Result<Self, VTEncoderError> {
        let mut encoder = Self::new_metal(
            input_parameters,
            output_parameters,
            max_in_flight_submissions,
            unsafe {
                wgpu_device
                    .as_hal::<MtlApi>()
                    .ok_or(VTInitError::NotMetalBackend)?
            }
            .raw_device(),
            on_chunk_callback,
        )?;
        encoder.wgpu = Some(VTWgpuEncodeState::new(wgpu_device)?);
        Ok(encoder)
    }

    fn issue_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VTEncoderError> {
        let (Some(state), Some(texture_cache)) = (self.wgpu.as_ref(), self.texture_cache.as_ref())
        else {
            return Err(VTEncoderError::NotConfiguredForWgpuInput);
        };

        let buffer = self.session.acquire_input_buffer()?;
        let texture = video_texture_from_pixel_buffer(
            texture_cache,
            wgpu_device,
            &buffer,
            wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING,
            wgpu::TextureUses::UNINITIALIZED,
            "gpu-video encoder input",
        )?;

        state
            .issued_input_textures
            .lock()
            .unwrap()
            .insert(texture.clone(), SendSyncCVBuffer(buffer));

        let issued_input_textures = state.issued_input_textures.clone();
        Ok(EncodeTexture {
            texture: texture.clone(),
            on_drop: Some(Box::new(move || {
                issued_input_textures.lock().unwrap().remove(&texture);
            })),
        })
    }

    pub(crate) fn submit_texture(
        &mut self,
        wgpu_queue: &wgpu::Queue,
        frame: &InputFrame<EncodeTexture>,
        force_idr: bool,
    ) -> Result<(), VTEncoderError> {
        if self.wgpu.is_none() {
            return Err(VTEncoderError::NotConfiguredForWgpuInput);
        }

        let SendSyncCVBuffer(buffer) = self
            .wgpu
            .as_ref()
            .unwrap()
            .issued_input_textures
            .lock()
            .unwrap()
            .remove(&frame.data.texture)
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;

        let state = self.wgpu.as_mut().unwrap();
        let value = state.next_fence_value;
        state.next_fence_value += 1;

        {
            let hal_queue =
                unsafe { wgpu_queue.as_hal::<MtlApi>() }.ok_or(VTInitError::NotMetalBackend)?;
            unsafe { hal_queue.submit(&[], &[], (&state.fence, value)) }
                .map_err(WgpuTextureEncoderError::from)?;
        }

        let shared_event = Retained::from(
            state
                .fence
                .raw_shared_event()
                .expect("presence was checked when the state was constructed"),
        );

        self.submit_when_event_reaches(
            buffer,
            frame.pts,
            force_idr,
            &shared_event,
            value,
            || Ok(()),
        )
    }
}

impl<C: EncodeCodec> WgpuVideoEncoderBackend for VTEncoder<C> {
    fn encode_texture(
        &mut self,
        _wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError> {
        Ok(self.submit_texture(wgpu_queue, &frame, force_idr)?)
    }

    fn flush(&mut self) -> Result<(), VideoEncoderError> {
        Ok(VTEncoder::flush(self)?)
    }

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError> {
        Ok(self.issue_input_texture(wgpu_device)?)
    }
}
