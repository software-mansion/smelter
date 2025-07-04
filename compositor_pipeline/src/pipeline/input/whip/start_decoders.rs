use crate::{
    pipeline::{
        self,
        decoder::{
            start_audio_decoder_thread, start_video_decoder_thread, AudioDecoderOptions,
            OpusDecoderOptions, VideoDecoderOptions,
        },
        input::whip::{
            depayloader::{AudioDepayloader, RolloverState, VideoDepayloader},
            start_forwarding_thread,
        },
        whip_whep::{error::WhipServerError, WhipInputState},
        EncodedChunk, PipelineCtx, VideoDecoder,
    },
    queue::PipelineEvent,
};

use compositor_render::InputId;
use rtp::codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Sender;
use webrtc::rtp_transceiver::{PayloadType, RTCRtpTransceiver};

use super::{depayloader::Depayloader, DecodedDataSender};

#[derive(Clone, Debug, Default)]
struct CodecsPayloadTypes {
    h264: Vec<PayloadType>,
    vp8: Vec<PayloadType>,
    vp9: Vec<PayloadType>,
    opus: Vec<PayloadType>,
}

pub type WhipInputDecoders =
    HashMap<PayloadType, (Sender<PipelineEvent<EncodedChunk>>, Arc<Mutex<Depayloader>>)>;

struct WhipDecodersBuilder {
    decoders: WhipInputDecoders,
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    decoded_data_sender: DecodedDataSender,
}

impl WhipDecodersBuilder {
    fn new(
        ctx: &Arc<PipelineCtx>,
        inputs: &WhipInputState,
        input_id: InputId,
    ) -> Result<WhipDecodersBuilder, WhipServerError> {
        let decoded_data_sender = inputs
            .get_input_connection_options(input_id.clone())?
            .decoded_data_sender;

        Ok(Self {
            decoders: HashMap::new(),
            ctx: ctx.clone(),
            input_id,
            decoded_data_sender,
        })
    }

    fn build(self) -> WhipInputDecoders {
        self.decoders
    }

    fn add_h264(&mut self, payload_types: &Vec<PayloadType>) -> Result<(), WhipServerError> {
        let (whip_client_to_bridge_sender, bridge_to_decoder_receiver) =
            start_forwarding_thread(self.input_id.clone());

        let decoder = pipeline::VideoDecoder::FFmpegH264;
        start_video_decoder_thread(
            VideoDecoderOptions { decoder },
            &self.ctx,
            bridge_to_decoder_receiver,
            self.decoded_data_sender.frame_sender.clone(),
            self.input_id.clone(),
            false,
        )?;
        let depayloader = Arc::new(Mutex::new(Depayloader {
            video: Some(VideoDepayloader::H264 {
                depayloader: H264Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            }),
            audio: None,
        }));
        for payload_type in payload_types {
            self.decoders.insert(
                *payload_type,
                (whip_client_to_bridge_sender.clone(), depayloader.clone()),
            );
        }
        Ok(())
    }

    #[cfg(feature = "vk-video")]
    fn add_h264_vulcan(&mut self, payload_types: &Vec<PayloadType>) -> Result<(), WhipServerError> {
        let (whip_client_to_bridge_sender, bridge_to_decoder_receiver) =
            start_forwarding_thread(self.input_id.clone());

        let decoder = pipeline::VideoDecoder::VulkanVideoH264;
        start_video_decoder_thread(
            VideoDecoderOptions { decoder },
            &self.ctx,
            bridge_to_decoder_receiver,
            self.decoded_data_sender.frame_sender.clone(),
            self.input_id.clone(),
            false,
        )?;
        let depayloader = Arc::new(Mutex::new(Depayloader {
            video: Some(VideoDepayloader::H264 {
                depayloader: H264Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            }),
            audio: None,
        }));
        for payload_type in payload_types {
            self.decoders.insert(
                *payload_type,
                (whip_client_to_bridge_sender.clone(), depayloader.clone()),
            );
        }
        Ok(())
    }

    fn add_vp8(&mut self, payload_types: &Vec<PayloadType>) -> Result<(), WhipServerError> {
        let (whip_client_to_bridge_sender, bridge_to_decoder_receiver) =
            start_forwarding_thread(self.input_id.clone());

        start_video_decoder_thread(
            VideoDecoderOptions {
                decoder: pipeline::VideoDecoder::FFmpegVp8,
            },
            &self.ctx,
            bridge_to_decoder_receiver,
            self.decoded_data_sender.frame_sender.clone(),
            self.input_id.clone(),
            false,
        )?;
        let depayloader = Arc::new(Mutex::new(Depayloader {
            video: Some(VideoDepayloader::VP8 {
                depayloader: Vp8Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            }),
            audio: None,
        }));
        for payload_type in payload_types {
            self.decoders.insert(
                *payload_type,
                (whip_client_to_bridge_sender.clone(), depayloader.clone()),
            );
        }
        Ok(())
    }

    fn add_vp9(&mut self, payload_types: &Vec<PayloadType>) -> Result<(), WhipServerError> {
        let (whip_client_to_bridge_sender, bridge_to_decoder_receiver) =
            start_forwarding_thread(self.input_id.clone());

        start_video_decoder_thread(
            VideoDecoderOptions {
                decoder: pipeline::VideoDecoder::FFmpegVp9,
            },
            &self.ctx,
            bridge_to_decoder_receiver,
            self.decoded_data_sender.frame_sender.clone(),
            self.input_id.clone(),
            false,
        )?;
        let depayloader = Arc::new(Mutex::new(Depayloader {
            video: Some(VideoDepayloader::VP9 {
                depayloader: Vp9Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            }),
            audio: None,
        }));
        for payload_type in payload_types {
            self.decoders.insert(
                *payload_type,
                (whip_client_to_bridge_sender.clone(), depayloader.clone()),
            );
        }
        Ok(())
    }

    fn add_opus(&mut self, payload_types: Vec<PayloadType>) -> Result<(), WhipServerError> {
        let (whip_client_to_bridge_sender, bridge_to_decoder_receiver) =
            start_forwarding_thread(self.input_id.clone());

        start_audio_decoder_thread(
            AudioDecoderOptions::Opus(OpusDecoderOptions {
                forward_error_correction: false,
            }),
            self.ctx.mixing_sample_rate,
            bridge_to_decoder_receiver,
            self.decoded_data_sender.input_samples_sender.clone(),
            self.input_id.clone(),
            false,
        )?;

        let depayloader = Arc::new(Mutex::new(Depayloader {
            video: None,
            audio: Some(AudioDepayloader::Opus {
                depayloader: OpusPacket,
                rollover_state: RolloverState::default(),
            }),
        }));

        for payload_type in payload_types {
            self.decoders.insert(
                payload_type,
                (whip_client_to_bridge_sender.clone(), depayloader.clone()),
            );
        }
        Ok(())
    }
}

pub async fn start_decoders_threads(
    ctx: &Arc<PipelineCtx>,
    inputs: &WhipInputState,
    input_id: InputId,
    video_transceiver: Arc<RTCRtpTransceiver>,
    audio_transceiver: Arc<RTCRtpTransceiver>,
    video_decoder_preferences: Vec<VideoDecoder>,
) -> Result<WhipInputDecoders, WhipServerError> {
    let negotiated_codecs = get_codec_map(video_transceiver, audio_transceiver).await;
    let mut whip_decoders_setup = WhipDecodersBuilder::new(ctx, inputs, input_id)?;

    let mut h264_decoder_started = false;
    for video_decoder in video_decoder_preferences {
        match video_decoder {
            VideoDecoder::FFmpegH264 => {
                if !h264_decoder_started && !negotiated_codecs.h264.is_empty() {
                    whip_decoders_setup.add_h264(&negotiated_codecs.h264)?;
                    h264_decoder_started = true;
                }
            }
            VideoDecoder::FFmpegVp8 => {
                if !negotiated_codecs.vp8.is_empty() {
                    whip_decoders_setup.add_vp8(&negotiated_codecs.vp8)?;
                }
            }
            VideoDecoder::FFmpegVp9 => {
                if !negotiated_codecs.vp9.is_empty() {
                    whip_decoders_setup.add_vp9(&negotiated_codecs.vp9)?;
                }
            }
            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanVideoH264 => {
                if !h264_decoder_started
                    && ctx.graphics_context.vulkan_ctx.is_some()
                    && !negotiated_codecs.h264.is_empty()
                {
                    whip_decoders_setup.add_h264_vulcan(&negotiated_codecs.h264)?;
                    h264_decoder_started = true;
                }
            }
        }
    }

    if !negotiated_codecs.opus.is_empty() {
        whip_decoders_setup.add_opus(negotiated_codecs.opus)?;
    }

    if whip_decoders_setup.decoders.is_empty() {
        return Err(WhipServerError::CodecNegotiationError(
            "None of negotiated codecs are supported in the Smelter!".to_string(),
        ));
    }
    Ok(whip_decoders_setup.build())
}

async fn get_codec_map(
    video_transceiver: Arc<RTCRtpTransceiver>,
    audio_transceiver: Arc<RTCRtpTransceiver>,
) -> CodecsPayloadTypes {
    let mut codec_payload_types = CodecsPayloadTypes::default();

    let video_receiver = video_transceiver.receiver().await;
    let mut codecs = video_receiver.get_parameters().await.codecs;

    let audio_receiver = audio_transceiver.receiver().await;
    codecs.extend(audio_receiver.get_parameters().await.codecs);

    for codec in codecs {
        match codec.capability.mime_type.to_lowercase().as_str() {
            "video/h264" => codec_payload_types.h264.push(codec.payload_type),
            "video/vp8" => codec_payload_types.vp8.push(codec.payload_type),
            "video/vp9" => codec_payload_types.vp9.push(codec.payload_type),
            "audio/opus" => codec_payload_types.opus.push(codec.payload_type),
            _ => {}
        }
    }
    codec_payload_types
}
