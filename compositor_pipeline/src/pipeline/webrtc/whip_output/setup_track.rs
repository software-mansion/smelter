use rand::Rng;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, span, trace, Instrument, Level};
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender},
    stats::StatsReportType,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder, vulkan_h264::VulkanH264Encoder,
        },
        rtp::payloader::{PayloadedCodec, PayloaderOptions},
        webrtc::{
            handle_keyframe_requests::handle_keyframe_requests,
            whip_output::{
                track_task_audio::{WhipAudioTrackThread, WhipAudioTrackThreadOptions},
                track_task_video::{WhipVideoTrackThread, WhipVideoTrackThreadOptions},
                PeerConnection,
            },
        },
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

use super::{
    track_task_audio::WhipAudioTrackThreadHandle, track_task_video::WhipVideoTrackThreadHandle,
    WhipClientTrack, WhipOutputError,
};

pub trait MatchCodecCapability {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool;
}

impl MatchCodecCapability for VideoEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase()
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase()
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase()
            }
        }
    }
}

impl MatchCodecCapability for AudioEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            AudioEncoderOptions::Opus(_opt) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase()
            }
            AudioEncoderOptions::FdkAac(_) => false,
        }
    }
}

pub async fn setup_video_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    encoder_preferences: Vec<VideoEncoderOptions>,
) -> Result<(WhipVideoTrackThreadHandle, WhipClientTrack), WhipOutputError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;

    let Some((options, codec_params)) = encoder_preferences.iter().find_map(|encoder_options| {
        let supported = supported_codecs.iter().find_map(|codec_params| {
            match encoder_options.matches(&codec_params.capability) {
                true => Some(codec_params.clone()),
                false => None,
            }
        })?;
        Some((encoder_options.clone(), supported.clone()))
    }) else {
        return Err(WhipOutputError::NoVideoCodecNegotiated);
    };

    let track = Arc::new(TrackLocalStaticRTP::new(
        codec_params.capability.clone(),
        "video".to_string(),
        "webrtc-rs".to_string(),
    ));

    rtc_sender.replace_track(Some(track.clone())).await?;

    fn payloader_options(codec: PayloadedCodec, payload_type: u8, ssrc: u32) -> PayloaderOptions {
        PayloaderOptions {
            codec,
            payload_type,
            clock_rate: 90_000,
            mtu: 1200,
            ssrc,
        }
    }

    let ssrc = match rtc_sender_params.encodings.first() {
        Some(e) => e.ssrc,
        None => rand::thread_rng().gen::<u32>(),
    };
    let (sender, receiver) = mpsc::channel(1000);
    let handle = match options {
        VideoEncoderOptions::FfmpegH264(options) => {
            WhipVideoTrackThread::<FfmpegH264Encoder>::spawn(
                output_id.clone(),
                WhipVideoTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    payloader_options: payloader_options(
                        PayloadedCodec::H264,
                        codec_params.payload_type,
                        ssrc,
                    ),
                    chunks_sender: sender,
                },
            )
        }
        VideoEncoderOptions::VulkanH264(options) => {
            WhipVideoTrackThread::<VulkanH264Encoder>::spawn(
                output_id.clone(),
                WhipVideoTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    payloader_options: payloader_options(
                        PayloadedCodec::H264,
                        codec_params.payload_type,
                        ssrc,
                    ),
                    chunks_sender: sender,
                },
            )
        }
        VideoEncoderOptions::FfmpegVp8(options) => WhipVideoTrackThread::<FfmpegVp8Encoder>::spawn(
            output_id.clone(),
            WhipVideoTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Vp8,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
        VideoEncoderOptions::FfmpegVp9(options) => WhipVideoTrackThread::<FfmpegVp9Encoder>::spawn(
            output_id.clone(),
            WhipVideoTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Vp9,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
    }?;

    handle_keyframe_requests(
        ctx,
        rtc_sender.clone(),
        handle.keyframe_request_sender.clone(),
    );

    Ok((handle, WhipClientTrack { receiver, track }))
}

pub async fn setup_audio_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    pc: PeerConnection,
    encoder_preferences: Vec<AudioEncoderOptions>,
) -> Result<(WhipAudioTrackThreadHandle, WhipClientTrack), WhipOutputError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);

    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;
    let Some((options, codec_params)) = encoder_preferences.iter().find_map(|encoder_options| {
        let supported = supported_codecs.iter().find_map(|codec_params| {
            match encoder_options.matches(&codec_params.capability) {
                true => Some(codec_params.clone()),
                false => None,
            }
        })?;
        Some((encoder_options.clone(), supported.clone()))
    }) else {
        return Err(WhipOutputError::NoAudioCodecNegotiated);
    };

    let track = Arc::new(TrackLocalStaticRTP::new(
        codec_params.capability.clone(),
        "audio".to_string(),
        "webrtc-rs".to_string(),
    ));

    rtc_sender.replace_track(Some(track.clone())).await?;

    fn payloader_options(codec: PayloadedCodec, payload_type: u8, ssrc: u32) -> PayloaderOptions {
        PayloaderOptions {
            codec,
            payload_type,
            clock_rate: 48_000,
            mtu: 1200,
            ssrc,
        }
    }

    let ssrc = match rtc_sender_params.encodings.first() {
        Some(e) => e.ssrc,
        None => rand::thread_rng().gen::<u32>(),
    };
    let (sender, receiver) = mpsc::channel(1000);
    let handle = match options {
        AudioEncoderOptions::Opus(options) => WhipAudioTrackThread::<OpusEncoder>::spawn(
            output_id.clone(),
            WhipAudioTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Opus,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
        AudioEncoderOptions::FdkAac(_options) => {
            return Err(WhipOutputError::UnsupportedCodec("aac"))
        }
    }?;

    handle_packet_loss_requests(
        ctx,
        pc,
        rtc_sender.clone(),
        handle.packet_loss_sender.clone(),
        ssrc,
    );

    Ok((handle, WhipClientTrack { receiver, track }))
}

// Identifiers used in stats HashMap returnet by RTCPeerConnection::get_stats()
const RTC_OUTBOUND_RTP_AUDIO_STREAM: &str = "RTCOutboundRTPAudioStream_";
const RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM: &str = "RTCRemoteInboundRTPAudioStream_";

fn handle_packet_loss_requests(
    ctx: &Arc<PipelineCtx>,
    pc: PeerConnection,
    rtc_sender: Arc<RTCRtpSender>,
    packet_loss_sender: watch::Sender<i32>,
    ssrc: u32,
) {
    let mut cumulative_packets_sent: u64 = 0;
    let mut cumulative_packets_lost: u64 = 0;

    let span = span!(Level::DEBUG, "Packet loss handle");

    ctx.tokio_rt.spawn(
        async move {
            loop {
                if let Err(e) = rtc_sender.read_rtcp().await {
                    debug!(%e, "Error while reading rtcp.");
                }
            }
        }
        .instrument(span.clone()),
    );

    ctx.tokio_rt.spawn(
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let stats = pc.get_stats().await.reports;
                let outbound_id = String::from(RTC_OUTBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();
                let remote_inbound_id =
                    String::from(RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();

                let outbound_stats = match stats.get(&outbound_id) {
                    Some(StatsReportType::OutboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("OutboundRTP report is empty!");
                        continue;
                    }
                };

                let remote_inbound_stats = match stats.get(&remote_inbound_id) {
                    Some(StatsReportType::RemoteInboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("RemoteInboundRTP report is empty!");
                        continue;
                    }
                };

                let packets_sent: u64 = outbound_stats.packets_sent;
                // This can be lower than 0 in case of duplicates
                let packets_lost: u64 = i64::max(remote_inbound_stats.packets_lost, 0) as u64;

                let packet_loss_percentage = calculate_packet_loss_percentage(
                    packets_sent,
                    packets_lost,
                    cumulative_packets_sent,
                    cumulative_packets_lost,
                );
                if packet_loss_sender.send(packet_loss_percentage).is_err() {
                    debug!("Packet loss channel closed.");
                }
                cumulative_packets_sent = packets_sent;
                cumulative_packets_lost = packets_lost;
            }
        }
        .instrument(span),
    );
}

fn calculate_packet_loss_percentage(
    packets_sent: u64,
    packets_lost: u64,
    cumulative_packets_sent: u64,
    cumulative_packets_lost: u64,
) -> i32 {
    let packets_sent_since_last_report = packets_sent - cumulative_packets_sent;
    let packets_lost_since_last_report = packets_lost - cumulative_packets_lost;

    // I don't want the system to panic in case of some bug
    let packet_loss_percentage: i32 = if packets_sent_since_last_report != 0 {
        let mut loss =
            100.0 * packets_lost_since_last_report as f64 / packets_sent_since_last_report as f64;
        // loss is rounded up to the nearest multiple of 5
        loss = f64::ceil(loss / 5.0) * 5.0;
        loss as i32
    } else {
        0
    };

    trace!(
        packets_sent_since_last_report,
        packets_lost_since_last_report,
        packet_loss_percentage,
    );
    packet_loss_percentage
}
