use std::sync::{Arc, Mutex};

use crate::{
    EncodedInputChunk, EncodedOutputChunk, OutputFrame, VideoTranscoderError,
    backends::video_toolbox::{
        VTEncoder,
        decoder::{Completion, VTDecoder},
        encoder::{H264Codec, H265Codec, MetalInputFrame},
        error::{VTDecoderError, VTEncoderError, VTTranscoderError},
        metal_interop::{
            PlaneTextures, SendSyncCVBuffer, SyncCache, plane_textures_from_pixel_buffer,
        },
    },
    device::{Rational, VideoParameters},
    frame_sorter::{DecodeResult, FrameSorter},
    parameters::DecoderUsage,
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::{AccessUnit, H264Parser},
        reference_manager::{DecodeInformation, ReferenceContext},
    },
    transcoder::{
        AnyEncoderParameters, TranscoderOutputParameters, TranscoderParameters,
        VideoTranscoderBackend,
    },
};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_video as cv;
use objc2_metal::{self as mtl, MTLCommandBuffer, MTLCommandQueue, MTLDevice};
use objc2_video_toolbox as vt;

mod resize;

enum AnyEncoder {
    H264(VTEncoder<H264Codec>),
    H265(VTEncoder<H265Codec>),
}

impl AnyEncoder {
    fn encode_pixel_buffer(
        &mut self,
        buffer: &cv::CVBuffer,
        pts: Option<u64>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        match self {
            AnyEncoder::H264(encoder) => encoder.encode_pixel_buffer(buffer, pts, force_idr),
            AnyEncoder::H265(encoder) => encoder.encode_pixel_buffer(buffer, pts, force_idr),
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
    ) -> Result<Self, VTTranscoderError> {
        let input_parameters = VideoParameters {
            width: params.output_width,
            height: params.output_height,
            target_framerate: input_framerate,
        };

        // TODO: propagate the decoded stream's color space and range to the encoders instead of
        // relying on the caller's output parameters (which default to unspecified/limited)
        let encoder = match params.encoder_parameters {
            AnyEncoderParameters::H264(output_parameters) => AnyEncoder::H264(
                VTEncoder::new_metal(input_parameters, output_parameters, device)?,
            ),
            AnyEncoderParameters::H265(output_parameters) => AnyEncoder::H265(
                VTEncoder::new_metal(input_parameters, output_parameters, device)?,
            ),
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
    parser: H264Parser,
    reference_context: ReferenceContext,
    decoder: VTDecoder,
    decoded: Arc<Mutex<DecodedFrames>>,
    decoder_texture_cache: SyncCache,
    frame_sorter: FrameSorter<SendSyncCVBuffer>,
    backends: Vec<Backend>,
    _device: Retained<ProtocolObject<dyn mtl::MTLDevice>>,
    queue: Retained<ProtocolObject<dyn mtl::MTLCommandQueue>>,
}

impl Transcoder {
    pub(crate) fn new(params: TranscoderParameters) -> Result<Self, VTTranscoderError> {
        let device = mtl::MTLCreateSystemDefaultDevice().ok_or(VTTranscoderError::NoMetalDevice)?;
        let queue = device
            .newCommandQueue()
            .ok_or(VTTranscoderError::CommandQueueCreationFailed)?;
        let decoder_texture_cache =
            SyncCache::new_from_mtl(&device, mtl::MTLTextureUsage::ShaderRead)?;

        let backends = params
            .output_parameters
            .iter()
            .map(|p| Backend::new(&device, p, params.input_framerate))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            parser: H264Parser::new_avcc_output(),
            reference_context: ReferenceContext::default(),
            decoder: VTDecoder::new(DecoderUsage::Transcoding, true),
            decoded: Arc::default(),
            decoder_texture_cache,
            frame_sorter: FrameSorter::default(),
            backends,
            _device: device,
            queue,
        })
    }

    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VTTranscoderError> {
        let parsed = self.parser.parse(input.data, input.pts)?;
        let sorted = self.aus_to_sorted(parsed)?;
        self.resize_and_encode(sorted)
    }

    fn flush(&mut self) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VTTranscoderError> {
        let frames = self.parser.flush()?;
        let mut sorted = self.aus_to_sorted(frames)?;
        sorted.append(&mut self.frame_sorter.flush());
        self.resize_and_encode(sorted)
    }

    fn aus_to_sorted(
        &mut self,
        aus: Vec<AccessUnit>,
    ) -> Result<Vec<OutputFrame<SendSyncCVBuffer>>, VTTranscoderError> {
        if std::mem::take(&mut self.decoded.lock().unwrap().decode_failed) {
            self.reference_context.mark_corrupted_state();
        }

        let instructions = compile_to_decoder_instructions(&mut self.reference_context, aus)?;

        for instruction in instructions {
            match instruction {
                DecoderInstruction::Sps { sps, raw_bytes } => {
                    self.decoder.process_sps(sps, raw_bytes)
                }
                DecoderInstruction::Pps { pps, raw_bytes } => {
                    self.decoder.process_pps(pps, raw_bytes)
                }
                DecoderInstruction::Decode { decode_info, .. } => {
                    self.submit(decode_info, false)?
                }
                DecoderInstruction::Idr { decode_info, .. } => self.submit(decode_info, true)?,
                DecoderInstruction::Drop { .. } => {}
            }
        }

        self.decoder.wait_for_pending_frames()?;

        let decoded = std::mem::take(&mut self.decoded.lock().unwrap().frames);

        Ok(self.frame_sorter.put_frames(decoded))
    }

    fn submit(
        &mut self,
        decode_info: DecodeInformation,
        is_idr: bool,
    ) -> Result<(), VTDecoderError> {
        let decoded = self.decoded.clone();

        self.decoder.submit(
            decode_info,
            is_idr,
            vt::VTDecodeFrameFlags::empty(),
            move |completion| {
                let mut decoded = decoded.lock().unwrap();
                match completion {
                    Completion::Frame(frame) => decoded.frames.push(DecodeResult {
                        frame: SendSyncCVBuffer(frame.frame),
                        metadata: frame.metadata,
                    }),
                    Completion::Dropped => {}
                    Completion::Failed => decoded.decode_failed = true,
                }
            },
        )
    }

    fn resize_and_encode(
        &mut self,
        frames: Vec<OutputFrame<SendSyncCVBuffer>>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VTTranscoderError> {
        let mut outputs = Vec::with_capacity(frames.len());

        for decoded in frames {
            let input_planes =
                plane_textures_from_pixel_buffer(&self.decoder_texture_cache, &decoded.data.0)?;
            let encoder_inputs = self.resize(&input_planes)?;

            let mut frame = Vec::with_capacity(self.backends.len());
            for (backend, input) in self.backends.iter_mut().zip(encoder_inputs) {
                frame.push(backend.encoder.encode_pixel_buffer(
                    &input.buffer,
                    decoded.metadata.pts,
                    false,
                )?);
            }
            outputs.push(frame);
        }

        Ok(outputs)
    }

    fn resize(
        &mut self,
        input_planes: &PlaneTextures,
    ) -> Result<Vec<MetalInputFrame>, VTTranscoderError> {
        let cmd = self
            .queue
            .commandBuffer()
            .ok_or(VTTranscoderError::CommandBufferCreationFailed)?;

        let mut encoder_inputs = Vec::with_capacity(self.backends.len());
        for backend in &mut self.backends {
            let output = backend.encoder.acquire_input_frame()?;
            backend.encode_resize(&cmd, input_planes, &output.planes);
            encoder_inputs.push(output);
        }

        cmd.commit();
        cmd.waitUntilCompleted();

        if cmd.status() == mtl::MTLCommandBufferStatus::Error {
            let description = cmd.error().map(|e| e.to_string()).unwrap_or_default();
            return Err(VTTranscoderError::ResizeFailed(description));
        }

        Ok(encoder_inputs)
    }
}

#[derive(Default)]
struct DecodedFrames {
    frames: Vec<DecodeResult<SendSyncCVBuffer>>,
    decode_failed: bool,
}

impl VideoTranscoderBackend for Transcoder {
    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError> {
        Transcoder::transcode(self, input).map_err(Into::into)
    }

    fn flush(&mut self) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError> {
        Transcoder::flush(self).map_err(Into::into)
    }
}
