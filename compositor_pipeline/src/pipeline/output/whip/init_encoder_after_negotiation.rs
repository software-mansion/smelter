use webrtc::rtp_transceiver::PayloadType;

use crate::pipeline::encoder::{AudioEncoderOptions, Encoder, EncoderOptions, VideoEncoderOptions};

use super::{
    packet_stream::PacketStream,
    payloader::{AudioPayloaderOptions, Payloader, VideoPayloaderOptions},
    WhipCtx, WhipError,
};

pub fn create_encoder_and_packet_stream(
    whip_ctx: WhipCtx,
    video_encoder_options: Option<VideoEncoderOptions>,
    video_payload_type: Option<PayloadType>,
    audio_encoder_options: Option<AudioEncoderOptions>,
    audio_payload_type: Option<PayloadType>,
) -> Result<(Encoder, PacketStream), WhipError> {
    let Ok((encoder, packets_receiver)) = Encoder::new(
        &whip_ctx.output_id,
        EncoderOptions {
            video: video_encoder_options.clone(),
            audio: audio_encoder_options.clone(),
        },
        &whip_ctx.pipeline_ctx,
    ) else {
        return Err(WhipError::CannotInitEncoder);
    };

    let video_payloader_options = match (video_encoder_options, video_payload_type) {
        (Some(encoder), Some(payload_type)) => Some(VideoPayloaderOptions {
            encoder_options: encoder,
            payload_type,
        }),
        (_, _) => None,
    };

    let audio_payloader_options = match (audio_encoder_options, audio_payload_type) {
        (Some(encoder), Some(payload_type)) => Some(AudioPayloaderOptions {
            encoder_options: encoder,
            payload_type,
        }),
        (_, _) => None,
    };

    let payloader = Payloader::new(video_payloader_options, audio_payloader_options);
    let packet_stream = PacketStream::new(packets_receiver, payloader, 1400);

    Ok((encoder, packet_stream))
}
