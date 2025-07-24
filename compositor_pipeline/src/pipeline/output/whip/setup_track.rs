use compositor_render::OutputId;
use rand::Rng;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, span, trace, warn, Instrument, Level};
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender},
    stats::StatsReportType,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::pipeline::{
    encoder::{
        ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder, ffmpeg_vp9::FfmpegVp9Encoder,
        opus::OpusEncoder, AudioEncoderOptions, VideoEncoderOptions,
    },
    output::whip::{track_task_audio::spawn_audio_track_thread, PeerConnection},
    rtp::payloader::{PayloadedCodec, PayloaderOptions},
    PipelineCtx,
};

use super::{
    track_task_audio::WhipAudioTrackThreadHandle,
    track_task_video::{spawn_video_track_thread, WhipVideoTrackThreadHandle},
    AudioWhipOptions, VideoWhipOptions, WhipError, WhipSenderTrack,
};

pub trait MatchCodecCapability {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool;
}

impl MatchCodecCapability for VideoEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            VideoEncoderOptions::H264(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase()
            }
            VideoEncoderOptions::Vp8(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase()
            }
            VideoEncoderOptions::Vp9(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase()
            }
        }
    }
}

impl MatchCodecCapability for AudioEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            AudioEncoderOptions::Opus(opt) => {
                let codec_match =
                    capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase();

                let line = &capability.sdp_fmtp_line;
                let fec_negotiated = line.contains("useinbandfec=1");
                let fec_match = fec_negotiated == opt.forward_error_correction;

                codec_match && fec_match
            }
            AudioEncoderOptions::Aac(_) => false,
        }
    }
}

pub async fn setup_video_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    options: &VideoWhipOptions,
) -> Result<(WhipVideoTrackThreadHandle, WhipSenderTrack), WhipError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;

    let Some((options, codec_params)) =
        options
            .encoder_preferences
            .iter()
            .find_map(|encoder_options| {
                let supported = supported_codecs.iter().find_map(|codec_params| {
                    match encoder_options.matches(&codec_params.capability) {
                        true => Some(codec_params.clone()),
                        false => None,
                    }
                })?;
                Some((encoder_options.clone(), supported.clone()))
            })
    else {
        return Err(WhipError::NoVideoCodecNegotiated);
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
        VideoEncoderOptions::H264(options) => spawn_video_track_thread::<FfmpegH264Encoder>(
            ctx.clone(),
            output_id.clone(),
            options,
            payloader_options(PayloadedCodec::H264, codec_params.payload_type, ssrc),
            sender,
        ),
        VideoEncoderOptions::Vp8(options) => spawn_video_track_thread::<FfmpegVp8Encoder>(
            ctx.clone(),
            output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Vp8, codec_params.payload_type, ssrc),
            sender,
        ),
        VideoEncoderOptions::Vp9(options) => spawn_video_track_thread::<FfmpegVp9Encoder>(
            ctx.clone(),
            output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Vp9, codec_params.payload_type, ssrc),
            sender,
        ),
    }?;

    handle_keyframe_requests(
        ctx,
        rtc_sender.clone(),
        handle.keyframe_request_sender.clone(),
    );

    Ok((handle, WhipSenderTrack { receiver, track }))
}

fn handle_keyframe_requests(
    ctx: &Arc<PipelineCtx>,
    sender: Arc<RTCRtpSender>,
    keyframe_sender: crossbeam_channel::Sender<()>,
) {
    ctx.tokio_rt.spawn(async move {
        loop {
            if let Ok((packets, _)) = sender.read_rtcp().await {
                for packet in packets {
                    if packet
                        .as_any()
                        .downcast_ref::<PictureLossIndication>()
                        .is_some()
                    {
                        info!("Request keyframe");
                        if let Err(err) = keyframe_sender.send(()) {
                            warn!(%err, "Failed to send keyframe request to the encoder.");
                        };
                    }
                }
            } else {
                debug!("Failed to read RTCP packets from the sender.");
            }
        }
    });
}

pub async fn setup_audio_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    pc: PeerConnection,
    options: &AudioWhipOptions,
) -> Result<(WhipAudioTrackThreadHandle, WhipSenderTrack), WhipError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);

    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;
    let Some((options, codec_params)) =
        options
            .encoder_preferences
            .iter()
            .find_map(|encoder_options| {
                let supported = supported_codecs.iter().find_map(|codec_params| {
                    match encoder_options.matches(&codec_params.capability) {
                        true => Some(codec_params.clone()),
                        false => None,
                    }
                })?;
                Some((encoder_options.clone(), supported.clone()))
            })
    else {
        return Err(WhipError::NoAudioCodecNegotiated);
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
        AudioEncoderOptions::Opus(options) => spawn_audio_track_thread::<OpusEncoder>(
            ctx.clone(),
            output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Opus, codec_params.payload_type, ssrc),
            sender,
        ),
        AudioEncoderOptions::Aac(_options) => return Err(WhipError::UnsupportedCodec("aac")),
    }?;

    handle_packet_loss_requests(
        ctx,
        pc,
        rtc_sender.clone(),
        handle.packet_loss_sender.clone(),
        ssrc,
    );

    Ok((handle, WhipSenderTrack { receiver, track }))
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
    let mut cumulative_packets_sent_report: u64 = 0;
    let mut cumulative_packets_lost_report: u64 = 0;

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
                // TODO: change that to 30s before merging
                tokio::time::sleep(Duration::from_secs(10)).await;
                let stats = pc.get_stats().await.reports;
                let outbound_id = String::from(RTC_OUTBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();
                let remote_inbound_id =
                    String::from(RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();

                let outbound_stats = match stats.get(&outbound_id) {
                    Some(report) => match report {
                        StatsReportType::OutboundRTP(report) => report,
                        _ => {
                            error!("Invalid report type for given key! (This should not happen)");
                            continue;
                        }
                    },
                    None => {
                        debug!("OutboundRTP report is empty!");
                        continue;
                    }
                };

                let remote_inbound_stats = match stats.get(&remote_inbound_id) {
                    Some(report) => match report {
                        StatsReportType::RemoteInboundRTP(report) => report,
                        _ => {
                            error!("Invalid report type for given key! (This should not happen)");
                            continue;
                        }
                    },
                    None => {
                        debug!("OutboundRTP report is empty!");
                        continue;
                    }
                };

                let packets_sent: u64 = outbound_stats.packets_sent;
                // This can be lower than 0 in case of duplicates
                let packets_lost: u64 = if remote_inbound_stats.packets_lost < 0 {
                    0
                } else {
                    remote_inbound_stats.packets_lost as u64
                };

                let packets_sent_since_last_report = packets_sent - cumulative_packets_sent_report;
                let packets_lost_since_last_report = packets_lost - cumulative_packets_lost_report;

                // I don't want the system to panic in case of some bug
                let packet_loss_percentage: i32 = if packets_sent_since_last_report != 0 {
                    let mut loss = 100.0 * packets_lost_since_last_report as f64
                        / packets_sent_since_last_report as f64;
                    // loss is rounded up to the nearest multiple of 5
                    loss = f64::ceil(loss / 5.0) * 5.0;
                    loss as i32
                } else {
                    0
                };

                cumulative_packets_sent_report = packets_sent;
                cumulative_packets_lost_report = packets_lost;

                trace!(
                    packets_sent_since_last_report,
                    packets_lost_since_last_report,
                    packet_loss_percentage,
                );
                if packet_loss_sender.send(packet_loss_percentage).is_err() {
                    debug!("Packet loss channel closed.");
                }
            }
        }
        .instrument(span),
    );
}
