use std::{
    any::Any,
    panic::AssertUnwindSafe,
    ptr::NonNull,
    sync::{Arc, Mutex},
};

use crate::{
    DecoderEvent, EncodedInputChunk, EncodedOutputChunk, OutputFrame, VideoTranscoderError,
    backends::video_toolbox::{
        VTEncoder,
        decoders_h264::VTDecoderH264,
        encoder::{H264Codec, H265Codec, MetalInputFrame},
        error::{VTEncoderError, VTTranscoderError},
        metal_interop::{
            PlaneTextures, SendSyncCVBuffer, SyncCache, plane_textures_from_pixel_buffer,
        },
    },
    device::{DecoderParameters, Rational, VideoParameters},
    parameters::DecoderUsage,
    transcoder::{
        AnyEncoderParameters, TranscodedChunk, TranscoderOutputParameters, TranscoderParameters,
        VideoTranscoderBackend,
    },
};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal::{self as mtl, MTLCommandBuffer, MTLCommandQueue, MTLDevice, MTLSharedEvent};

mod resize;

enum AnyEncoder {
    H264(VTEncoder<H264Codec>),
    H265(VTEncoder<H265Codec>),
}

impl AnyEncoder {
    fn submit_when_event_reaches(
        &mut self,
        buffer: cf::CFRetained<cv::CVBuffer>,
        pts: Option<u64>,
        event: &ProtocolObject<dyn mtl::MTLSharedEvent>,
        value: u64,
        check_input_ready: impl FnOnce() -> Result<(), VTEncoderError> + 'static,
    ) -> Result<(), VTEncoderError> {
        match self {
            AnyEncoder::H264(encoder) => encoder.submit_when_event_reaches(
                buffer,
                pts,
                false,
                event,
                value,
                check_input_ready,
            ),
            AnyEncoder::H265(encoder) => encoder.submit_when_event_reaches(
                buffer,
                pts,
                false,
                event,
                value,
                check_input_ready,
            ),
        }
    }

    fn flush(&mut self) -> Result<(), VTEncoderError> {
        match self {
            AnyEncoder::H264(encoder) => encoder.flush(),
            AnyEncoder::H265(encoder) => encoder.flush(),
        }
    }

    fn acquire_input_frame(&self) -> Result<MetalInputFrame, VTEncoderError> {
        match self {
            AnyEncoder::H264(encoder) => encoder.acquire_input_frame(),
            AnyEncoder::H265(encoder) => encoder.acquire_input_frame(),
        }
    }
}

struct Backend {
    resizer: resize::Resizer,
    encoder: AnyEncoder,
}

impl Backend {
    fn new(
        device: &ProtocolObject<dyn mtl::MTLDevice>,
        params: &TranscoderOutputParameters,
        input_framerate: Rational,
        max_in_flight_submissions: u32,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
    ) -> Result<Self, VTTranscoderError> {
        let input_parameters = VideoParameters {
            width: params.output_width,
            height: params.output_height,
            target_framerate: input_framerate,
        };

        // TODO: propagate the decoded stream's color space and range to the encoders instead of
        // relying on the caller's output parameters (which default to unspecified/limited)
        let encoder = match params.encoder_parameters {
            AnyEncoderParameters::H264(output_parameters) => {
                AnyEncoder::H264(VTEncoder::new_metal(
                    input_parameters,
                    output_parameters,
                    max_in_flight_submissions,
                    device,
                    on_chunk_callback,
                )?)
            }
            AnyEncoderParameters::H265(output_parameters) => {
                AnyEncoder::H265(VTEncoder::new_metal(
                    input_parameters,
                    output_parameters,
                    max_in_flight_submissions,
                    device,
                    on_chunk_callback,
                )?)
            }
        };

        let resizer = resize::Resizer::new(params, device)?;

        Ok(Self { encoder, resizer })
    }

    fn encode_resize(
        &mut self,
        cmd: &ProtocolObject<dyn mtl::MTLCommandBuffer>,
        input_planes: &PlaneTextures,
        output_planes: &PlaneTextures,
    ) {
        self.resizer
            .encode_y(cmd, &input_planes.y, &output_planes.y);
        self.resizer
            .encode_uv(cmd, &input_planes.uv, &output_planes.uv);
    }
}

pub(crate) struct Transcoder {
    decoder: VTDecoderH264<SendSyncCVBuffer>,
    pipeline: Arc<Mutex<ResizeEncodePipeline>>,
}

struct DecodedPlanesKeptAlive(PlaneTextures);

// Safety: the wrapped textures are only held to keep the decoded pixel buffer alive and are never
// accessed through this wrapper, only retained and released, which is thread-safe.
unsafe impl Send for DecodedPlanesKeptAlive {}
unsafe impl Sync for DecodedPlanesKeptAlive {}

struct ResizeEncodePipeline {
    decoder_texture_cache: SyncCache,
    backends: Vec<Backend>,
    _device: Retained<ProtocolObject<dyn mtl::MTLDevice>>,
    queue: Retained<ProtocolObject<dyn mtl::MTLCommandQueue>>,
    resize_finished_event: Retained<ProtocolObject<dyn mtl::MTLSharedEvent>>,
    next_event_value: u64,
    error: Option<VTTranscoderError>,
    panic: Option<Box<dyn Any + Send>>,
}

impl Transcoder {
    pub(crate) fn new(
        params: TranscoderParameters,
        on_chunk_callback: Box<dyn FnMut(TranscodedChunk) + Send>,
    ) -> Result<Self, VTTranscoderError> {
        let device = mtl::MTLCreateSystemDefaultDevice().ok_or(VTTranscoderError::NoMetalDevice)?;
        let queue = device
            .newCommandQueue()
            .ok_or(VTTranscoderError::CommandQueueCreationFailed)?;
        let decoder_texture_cache =
            SyncCache::new_from_mtl(&device, mtl::MTLTextureUsage::ShaderRead)?;

        let resize_finished_event = device
            .newSharedEvent()
            .ok_or(VTTranscoderError::SharedEventCreationFailed)?;

        let max_in_flight_submissions = params.max_in_flight_submissions.unwrap_or(3);
        let on_chunk_callback = Arc::new(Mutex::new(on_chunk_callback));
        let backends = params
            .output_parameters
            .iter()
            .enumerate()
            .map(|(output_index, p)| {
                let on_chunk_callback = on_chunk_callback.clone();
                Backend::new(
                    &device,
                    p,
                    params.input_framerate,
                    max_in_flight_submissions,
                    Box::new(move |chunk| {
                        let Ok(mut callback) = on_chunk_callback.lock() else {
                            return;
                        };
                        callback(TranscodedChunk {
                            output_index,
                            chunk,
                        })
                    }),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let pipeline = Arc::new(Mutex::new(ResizeEncodePipeline {
            decoder_texture_cache,
            backends,
            _device: device,
            queue,
            resize_finished_event,
            next_event_value: 1,
            error: None,
            panic: None,
        }));

        let decoder_pipeline = pipeline.clone();
        let decoder = VTDecoderH264::new_pixel_buffers(
            DecoderParameters {
                corrupted_state_handling: Default::default(),
                usage_flags: DecoderUsage::Transcoding,
                max_in_flight_submissions,
            },
            Box::new(move |frame| {
                let mut pipeline = decoder_pipeline.lock().unwrap();
                if pipeline.panic.is_some() {
                    return;
                }
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    pipeline.resize_and_encode(frame)
                }));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) if pipeline.error.is_none() => pipeline.error = Some(err),
                    Ok(Err(err)) => {
                        tracing::error!("Failed to resize and encode a decoded frame: {err}")
                    }
                    Err(payload) => pipeline.panic = Some(payload),
                }
            }),
        );

        Ok(Self { decoder, pipeline })
    }

    fn take_pipeline_error(&self) -> Result<(), VTTranscoderError> {
        let (panic, error) = {
            let mut pipeline = self.pipeline.lock().unwrap();
            (pipeline.panic.take(), pipeline.error.take())
        };
        if let Some(payload) = panic {
            std::panic::resume_unwind(payload);
        }
        match error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn transcode(&mut self, input: EncodedInputChunk<'_>) -> Result<(), VTTranscoderError> {
        self.take_pipeline_error()?;
        self.decoder
            .process_event(DecoderEvent::DecodeChunk(input))?;
        self.take_pipeline_error()
    }

    fn flush(&mut self) -> Result<(), VTTranscoderError> {
        self.decoder.process_event(DecoderEvent::Flush)?;

        let encoders_flushed = {
            let mut pipeline = self.pipeline.lock().unwrap();
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                pipeline
                    .backends
                    .iter_mut()
                    .map(|backend| backend.encoder.flush())
                    .collect::<Vec<_>>()
            }))
        };

        self.take_pipeline_error()?;
        let encoders_flushed = match encoders_flushed {
            Ok(results) => results,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        encoders_flushed.into_iter().collect::<Result<(), _>>()?;
        Ok(())
    }
}

impl ResizeEncodePipeline {
    fn resize_and_encode(
        &mut self,
        decoded: OutputFrame<SendSyncCVBuffer>,
    ) -> Result<(), VTTranscoderError> {
        let input_planes = Arc::new(DecodedPlanesKeptAlive(plane_textures_from_pixel_buffer(
            &self.decoder_texture_cache,
            &decoded.data.0,
        )?));

        let cmd = self
            .queue
            .commandBuffer()
            .ok_or(VTTranscoderError::CommandBufferCreationFailed)?;

        let mut encoder_inputs = Vec::with_capacity(self.backends.len());
        for backend in &mut self.backends {
            let output = backend.encoder.acquire_input_frame()?;
            backend.encode_resize(&cmd, &input_planes.0, &output.planes);
            encoder_inputs.push(output);
        }

        let event_value = self.next_event_value;
        self.next_event_value += 1;
        let resize_finished_event = self.resize_finished_event.clone();
        let signal_event_when_completed = block2::RcBlock::new(
            move |_cmd: NonNull<ProtocolObject<dyn mtl::MTLCommandBuffer>>| {
                resize_finished_event.setSignaledValue(event_value);
            },
        );
        unsafe { cmd.addCompletedHandler(block2::RcBlock::as_ptr(&signal_event_when_completed)) };
        cmd.commit();

        for (backend, input) in self.backends.iter_mut().zip(encoder_inputs) {
            let cmd = cmd.clone();
            let input_planes = input_planes.clone();
            backend.encoder.submit_when_event_reaches(
                input.buffer,
                decoded.metadata.pts,
                &self.resize_finished_event,
                event_value,
                move || {
                    let _decoded_planes_kept_alive_until_resize_finished = input_planes;
                    if cmd.status() == mtl::MTLCommandBufferStatus::Error {
                        let description = cmd.error().map(|e| e.to_string()).unwrap_or_default();
                        return Err(VTEncoderError::InputPreparationFailed(description));
                    }
                    Ok(())
                },
            )?;
        }

        Ok(())
    }
}

impl VideoTranscoderBackend for Transcoder {
    fn transcode(&mut self, input: EncodedInputChunk<'_>) -> Result<(), VideoTranscoderError> {
        Transcoder::transcode(self, input).map_err(Into::into)
    }

    fn flush(&mut self) -> Result<(), VideoTranscoderError> {
        Transcoder::flush(self).map_err(Into::into)
    }
}
