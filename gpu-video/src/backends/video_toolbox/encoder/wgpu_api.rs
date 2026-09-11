use std::{
    collections::{HashMap, VecDeque},
    ptr::{NonNull, null_mut},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_metal::{MTLSharedEvent, MTLSharedEventListener};
use objc2_video_toolbox as vt;
use wgpu::hal::{Device as _, Queue as _, metal::Api as MtlApi};

use crate::{
    EncodedOutputChunk, InputFrame, VideoEncoderError, VideoTexture,
    backends::video_toolbox::{
        error::{OSStatusError, OSStatusExt, VTEncoderError, VTInitError},
        metal_interop::SendSyncCVBuffer,
        wgpu_api::video_texture_from_pixel_buffer,
    },
    device::{EncoderOutputParameters, VideoParameters},
    encoders::{EncodeTexture, WgpuTextureEncoderError, WgpuVideoEncoderBackend},
};

use super::{CallbackOutput, EncodeCodec, VTEncoder, check_output_status, output_handler};

pub(crate) struct VTWgpuEncodeState {
    fence: wgpu::hal::metal::Fence,
    /// this runs frame encode closures
    listener: Retained<MTLSharedEventListener>,
    next_fence_value: u64,
    issued_input_textures: Arc<Mutex<HashMap<VideoTexture, SendSyncCVBuffer>>>,
    pending: VecDeque<PendingFrame>,
}

struct PendingFrame {
    submitted: mpsc::Receiver<Result<(), OSStatusError>>,
    output: mpsc::Receiver<CallbackOutput>,
    frame_content: SendSyncCVBuffer,
    session_generation: u64,
    cm_pts: cm::CMTime,
    pts: Option<u64>,
    force_idr: bool,
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

        Ok(Self {
            fence,
            listener: MTLSharedEventListener::new(),
            next_fence_value: 1,
            issued_input_textures: Default::default(),
            pending: VecDeque::new(),
        })
    }
}

impl<C: EncodeCodec> VTEncoder<C> {
    pub(crate) fn new_wgpu(
        wgpu_device: &wgpu::Device,
        input_parameters: VideoParameters,
        output_parameters: EncoderOutputParameters<C::Profile>,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
    ) -> Result<Self, VTEncoderError> {
        let mut encoder = Self::new_metal(
            input_parameters,
            output_parameters,
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
        let Some(state) = self.wgpu.as_ref() else {
            return Err(VTEncoderError::NotConfiguredForWgpuInput);
        };

        if self.parameters_changed_mid_stream && !self.inline_stream_params {
            return Err(VTEncoderError::ParametersDiverged);
        }

        let SendSyncCVBuffer(buffer) = state
            .issued_input_textures
            .lock()
            .unwrap()
            .remove(&frame.data.texture)
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;

        let (cm_pts, duration) = self.next_frame_timing();
        let session_generation = self.session_generation;

        let listener_frame = ListenerFrame {
            session: self.session.session.0.clone(),
            buffer: buffer.clone(),
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

        let (submitted_tx, submitted_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();

        let block = block2::RcBlock::new(
            move |_event: NonNull<ProtocolObject<dyn MTLSharedEvent>>, _value: u64| {
                let output_block = output_handler(output_tx.clone());
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
                        block2::RcBlock::as_ptr(&output_block),
                    )
                };

                let _ = submitted_tx.send(status.osstatus());
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
                block2::RcBlock::as_ptr(&block),
            )
        };

        state.pending.push_back(PendingFrame {
            submitted: submitted_rx,
            output: output_rx,
            frame_content: SendSyncCVBuffer(buffer),
            session_generation,
            cm_pts,
            pts: frame.pts,
            force_idr,
        });

        Ok(())
    }

    pub(crate) fn wait_for_encoded_frame(
        &mut self,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        let pending = self
            .wgpu
            .as_mut()
            .ok_or(VTEncoderError::NotConfiguredForWgpuInput)?
            .pending
            .pop_front()
            .ok_or(VTEncoderError::NoPendingFrame)?;

        let result = match pending
            .submitted
            .recv_timeout(std::time::Duration::from_secs(1))
        {
            // Accepted by a session that has since been replaced; its output can't be collected.
            Ok(Ok(())) if pending.session_generation != self.session_generation => {
                return self.reencode_after_invalidation(&pending);
            }
            Ok(Ok(())) => self.finish_frame(&pending),
            Ok(Err(status)) => Err(status.into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(VTEncoderError::SubmissionLost);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Err(VTEncoderError::SubmissionTimeout),
        };

        match result {
            Err(VTEncoderError::OSStatus(OSStatusError::VTInvalidSession)) => {
                self.reencode_after_invalidation(&pending)
            }
            result => result,
        }
    }

    fn reencode_after_invalidation(
        &mut self,
        pending: &PendingFrame,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        // The copy into the input buffer is done, so no listener is needed for the retry.
        let frame_properties = Self::frame_properties(pending.force_idr);
        let frame_properties = frame_properties
            .as_ref()
            .map(|properties| properties.as_ref());
        self.recover_from_invalidated_session(
            pending.session_generation,
            &pending.frame_content.0,
            pending.cm_pts,
            self.frame_duration(),
            frame_properties,
            pending.pts,
        )
    }

    fn finish_frame(
        &mut self,
        pending: &PendingFrame,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        unsafe {
            self.session
                .session
                .complete_frames(pending.cm_pts)
                .osstatus()?
        };

        let output = pending
            .output
            .try_recv()
            .map_err(|_| VTEncoderError::NoEncoderOutput)?;
        let sample = check_output_status(output)?;
        self.collect_output(&sample, pending.pts)
    }
}

impl<C: EncodeCodec> WgpuVideoEncoderBackend for VTEncoder<C> {
    fn encode_texture(
        &mut self,
        _wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
        _timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.submit_texture(wgpu_queue, &frame, force_idr)?;
        let chunk = self.wait_for_encoded_frame()?;
        (self.on_chunk_callback)(chunk);
        Ok(())
    }

    fn flush(&mut self, _timeout: Duration) -> Result<(), VideoEncoderError> {
        Ok(())
    }

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError> {
        Ok(self.issue_input_texture(wgpu_device)?)
    }
}
