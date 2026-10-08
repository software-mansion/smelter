use std::{
    collections::HashMap,
    ptr::{NonNull, null_mut},
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_metal as mtl;
use objc2_metal::{MTLSharedEvent, MTLSharedEventListener};
use objc2_video_toolbox as vt;
use wgpu::hal::{Device as _, Queue as _, metal::Api as MtlApi};

use crate::{
    EncodedOutputChunk, InputFrame, VideoEncoderError, VideoTexture,
    backends::video_toolbox::{
        error::{OSStatusExt, VTEncoderError, VTInitError},
        wgpu_api::{
            SendSyncCVBuffer, SyncCache, make_texture_cache, video_texture_from_pixel_buffer,
        },
    },
    device::{EncoderOutputParameters, VideoParameters},
    encoders::{EncodeTexture, WgpuTextureEncoderError, WgpuVideoEncoderBackend},
};

use super::{EncodeCodec, FrameOutput, VTEncoder};

pub(crate) struct VTWgpuEncodeState {
    fence: wgpu::hal::metal::Fence,
    /// this runs frame encode closures
    listener: Retained<MTLSharedEventListener>,
    next_fence_value: u64,
    texture_cache: SyncCache,
    issued_input_textures: Arc<Mutex<HashMap<VideoTexture, SendSyncCVBuffer>>>,
}

struct ListenerFrame {
    session: cf::CFRetained<vt::VTCompressionSession>,
    buffer: cf::CFRetained<cv::CVBuffer>,
    frame_properties: Option<cf::CFRetained<cf::CFDictionary<cf::CFString, cf::CFType>>>,
}

// Safety: VT sessions are not documented as thread-affine (see `Session`), CF retain counts are
// thread-safe, and nothing mutates the buffer or the dictionary after construction.
unsafe impl Send for ListenerFrame {}

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

        let texture_cache = make_texture_cache(
            wgpu_device,
            mtl::MTLTextureUsage(
                mtl::MTLTextureUsage::ShaderWrite.0 | mtl::MTLTextureUsage::RenderTarget.0,
            ),
        )?;

        Ok(Self {
            fence,
            listener: MTLSharedEventListener::new(),
            next_fence_value: 1,
            texture_cache,
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
        let mut encoder = Self::create(
            input_parameters,
            output_parameters,
            max_in_flight_submissions,
            true,
            on_chunk_callback,
        )?;
        encoder.wgpu = Some(VTWgpuEncodeState::new(wgpu_device)?);
        Ok(encoder)
    }

    fn issue_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VTEncoderError> {
        let state = self
            .wgpu
            .as_ref()
            .ok_or(VTEncoderError::NotConfiguredForWgpuInput)?;

        let buffer = self.session.acquire_input_buffer()?;
        let texture = video_texture_from_pixel_buffer(
            &state.texture_cache,
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

        let force_idr = self.prepare_submission(force_idr)?;

        let SendSyncCVBuffer(buffer) = self
            .wgpu
            .as_ref()
            .unwrap()
            .issued_input_textures
            .lock()
            .unwrap()
            .remove(&frame.data.texture)
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;

        let (cm_pts, duration) = self.next_frame_timing();

        let listener_frame = ListenerFrame {
            session: self.session.session.0.clone(),
            buffer,
            frame_properties: Self::frame_properties(force_idr),
        };

        let state = self.wgpu.as_mut().unwrap();
        let value = state.next_fence_value;
        state.next_fence_value += 1;

        {
            let hal_queue =
                unsafe { wgpu_queue.as_hal::<MtlApi>() }.ok_or(VTInitError::NotMetalBackend)?;
            unsafe { hal_queue.submit(&[], &[], (&state.fence, value)) }
                .map_err(WgpuTextureEncoderError::from)?;
        }

        let completion_deadline = self.frame_completion_deadline(self.frame_index - 1);
        let flush_in_progress = self.flush_in_progress.clone();

        let state = self.wgpu.as_ref().unwrap();
        let submitted = self.in_flight.submit(Duration::MAX, |submission_token| {
            let frame_output = FrameOutput::new(
                &self.output_state,
                &self.encode_failed,
                self.session_generation,
                frame.pts,
                submission_token,
            );

            let listener_block = block2::RcBlock::new(
                move |_event: NonNull<ProtocolObject<dyn MTLSharedEvent>>, _value: u64| {
                    let status = unsafe {
                        listener_frame.session.encode_frame_with_output_handler(
                            &listener_frame.buffer,
                            cm_pts,
                            duration,
                            listener_frame
                                .frame_properties
                                .as_ref()
                                .map(|properties| properties.as_ref()),
                            null_mut(),
                            block2::RcBlock::as_ptr(&frame_output.output_block()),
                        )
                    };

                    if let Err(error) = status.osstatus() {
                        frame_output.complete(Err(error.into()));
                        return;
                    }

                    if let Some(completion_deadline) = completion_deadline {
                        let completed =
                            unsafe { listener_frame.session.complete_frames(completion_deadline) };
                        if let Err(error) = completed.osstatus() {
                            tracing::error!("Completing encoded frames failed: {error}");
                        }
                    }

                    if flush_in_progress.load(Ordering::Relaxed) {
                        let completed =
                            unsafe { listener_frame.session.complete_frames(cm::kCMTimeInvalid) };
                        if let Err(error) = completed.osstatus() {
                            tracing::error!("Completing encoded frames failed: {error}");
                        }
                    }
                },
            );

            let shared_event = state
                .fence
                .raw_shared_event()
                .expect("presence was checked when the state was constructed");
            unsafe {
                shared_event.notifyListener_atValue_block(
                    &state.listener,
                    value,
                    block2::RcBlock::as_ptr(&listener_block),
                )
            };

            Ok::<(), VTEncoderError>(())
        });

        self.check_for_panic();
        submitted?;

        Ok(())
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
