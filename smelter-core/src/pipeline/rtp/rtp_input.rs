use std::sync::{Arc, atomic::AtomicBool};

use crossbeam_channel::{Receiver, bounded};
use smelter_render::Frame;
use tracing::{Level, debug, span, trace, warn};
use webrtc::{
    rtcp::{self, header::PacketType, sender_report::SenderReport},
    rtp,
};
use webrtc_util::Unmarshal;

use self::{tcp_server::start_tcp_server_thread, udp::start_udp_reader_thread};

use crate::{
    pipeline::{
        decoder::{
            fdk_aac::FdkAacDecoder, ffmpeg_h264::FfmpegH264Decoder, ffmpeg_vp8::FfmpegVp8Decoder,
            ffmpeg_vp9::FfmpegVp9Decoder, libopus::OpusDecoder, vulkan_h264::VulkanH264Decoder,
        },
        input::Input,
        rtp::{
            RtpJitterBuffer, RtpJitterBufferInitOptions,
            depayloader::DepayloaderOptions,
            rtp_input::{
                rtp_audio_thread::{
                    RtpAudioThread, RtpAudioThreadOptions, RtpAudioTrackThreadHandle,
                },
                rtp_video_thread::{RtpVideoThread, RtpVideoTrackThreadHandle},
            },
            util::BindToPortError,
        },
    },
    queue::QueueDataReceiver,
    thread_utils::InitializableThread,
};

use crate::prelude::*;

mod rollover_state;
mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

pub(super) mod jitter_buffer;
pub(super) mod rtcp_sync;

pub struct RtpInput {
    should_close: Arc<AtomicBool>,
}

impl RtpInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: RtpInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));

        let (port, raw_packets_receiver) = match opts.transport_protocol {
            RtpInputTransportProtocol::Udp => {
                start_udp_reader_thread(&input_ref, &opts, should_close.clone())?
            }
            RtpInputTransportProtocol::TcpServer => {
                start_tcp_server_thread(&input_ref, &opts, should_close.clone())?
            }
        };

        let (video_handle, video_frames_receiver) =
            Self::start_video_thread(&ctx, &input_ref, opts.video)?;

        let (audio_handle, audio_samples_receiver) =
            Self::start_audio_thread(&ctx, &input_ref, opts.audio)?;

        let jitter_buffer_init = RtpJitterBufferInitOptions::new(&ctx, opts.jitter_buffer);

        // TODO: this could ran on the same thread as tcp/udp socket
        Self::start_rtp_demuxer_thread(
            ctx,
            &input_ref,
            jitter_buffer_init,
            raw_packets_receiver,
            audio_handle,
            video_handle,
        );

        Ok((
            Input::Rtp(Self { should_close }),
            InputInitInfo::Rtp { port: Some(port) },
            QueueDataReceiver {
                video: video_frames_receiver,
                audio: audio_samples_receiver,
            },
        ))
    }

    fn start_rtp_demuxer_thread(
        ctx: Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        jitter_buffer_init: RtpJitterBufferInitOptions,
        receiver: Receiver<bytes::Bytes>,
        audio: Option<RtpAudioTrackThreadHandle>,
        video: Option<RtpVideoTrackThreadHandle>,
    ) {
        let input_ref = input_ref.clone();
        std::thread::Builder::new()
            .name(format!("Depayloading thread for input: {input_ref}"))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "RTP demuxer", input_id = input_ref.to_string()).entered();
                run_rtp_demuxer_thread(ctx, jitter_buffer_init, receiver, video, audio)
            })
            .unwrap();
    }

    #[allow(clippy::type_complexity)]
    fn start_video_thread(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        options: Option<VideoDecoderOptions>,
    ) -> Result<
        (
            Option<RtpVideoTrackThreadHandle>,
            Option<Receiver<PipelineEvent<Frame>>>,
        ),
        DecoderInitError,
    > {
        let Some(options) = options else {
            return Ok((None, None));
        };

        let (sender, receiver) = bounded(5);
        let handle = match options {
            VideoDecoderOptions::FfmpegH264 => RtpVideoThread::<FfmpegH264Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::H264, sender),
            )?,
            VideoDecoderOptions::FfmpegVp8 => RtpVideoThread::<FfmpegVp8Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::Vp8, sender),
            )?,
            VideoDecoderOptions::FfmpegVp9 => RtpVideoThread::<FfmpegVp9Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::Vp9, sender),
            )?,
            VideoDecoderOptions::VulkanH264 => {
                if !ctx.graphics_context.has_vulkan_decoder_support() {
                    return Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder);
                }
                RtpVideoThread::<VulkanH264Decoder>::spawn(
                    input_ref.clone(),
                    (ctx.clone(), DepayloaderOptions::H264, sender),
                )?
            }
        };
        Ok((Some(handle), Some(receiver)))
    }

    #[allow(clippy::type_complexity)]
    fn start_audio_thread(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        options: Option<RtpAudioOptions>,
    ) -> Result<
        (
            Option<RtpAudioTrackThreadHandle>,
            Option<Receiver<PipelineEvent<InputAudioSamples>>>,
        ),
        DecoderInitError,
    > {
        let Some(options) = options else {
            return Ok((None, None));
        };

        let (sender, receiver) = bounded(5);
        let handle = match options {
            RtpAudioOptions::Opus => RtpAudioThread::<OpusDecoder>::spawn(
                input_ref,
                RtpAudioThreadOptions {
                    ctx: ctx.clone(),
                    sample_rate: 48_000,
                    decoder_options: (),
                    depayloader_options: DepayloaderOptions::Opus,
                    decoded_samples_sender: sender,
                },
            )?,
            RtpAudioOptions::FdkAac {
                asc,
                raw_asc,
                depayloader_mode,
            } => RtpAudioThread::<FdkAacDecoder>::spawn(
                input_ref,
                RtpAudioThreadOptions {
                    ctx: ctx.clone(),
                    sample_rate: asc.sample_rate,
                    decoder_options: FdkAacDecoderOptions { asc: Some(raw_asc) },
                    depayloader_options: DepayloaderOptions::Aac(depayloader_mode, asc),
                    decoded_samples_sender: sender,
                },
            )?,
        };
        Ok((Some(handle), Some(receiver)))
    }
}

impl Drop for RtpInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn run_rtp_demuxer_thread(
    ctx: Arc<PipelineCtx>,
    jitter_buffer_init: RtpJitterBufferInitOptions,
    receiver: Receiver<bytes::Bytes>,
    video_handle: Option<RtpVideoTrackThreadHandle>,
    audio_handle: Option<RtpAudioTrackThreadHandle>,
) {
    struct TrackState<Handle> {
        jitter_buffer: RtpJitterBuffer,
        handle: Handle,
        eos_received: bool,
    }

    let mut audio = audio_handle.map(|handle| TrackState {
        jitter_buffer: RtpJitterBuffer::new(
            &ctx,
            jitter_buffer_init.clone(),
            handle.sample_rate,
            Box::new(|_event| {}),
        ),
        handle,
        eos_received: false,
    });
    let mut video = video_handle.map(|handle| TrackState {
        jitter_buffer: RtpJitterBuffer::new(
            &ctx,
            jitter_buffer_init,
            90_000,
            Box::new(|_event| {}),
        ),
        handle,
        eos_received: false,
    });

    let mut audio_ssrc = None;
    let mut video_ssrc = None;

    let maybe_send_video_eos = |video: &mut Option<TrackState<RtpVideoTrackThreadHandle>>| {
        if let Some(video) = video
            && !video.eos_received
        {
            video.eos_received = true;
            let sender = &video.handle.rtp_packet_sender;
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS from RTP video depayloader. Channel closed.");
            }
        }
    };
    let maybe_send_audio_eos = |audio: &mut Option<TrackState<RtpAudioTrackThreadHandle>>| {
        if let Some(audio) = audio
            && !audio.eos_received
        {
            audio.eos_received = true;
            let sender = &audio.handle.rtp_packet_sender;
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS from RTP audio depayloader. Channel closed.");
            }
        }
    };
    loop {
        let Ok(mut buffer) = receiver.recv() else {
            debug!("Closing RTP demuxer thread.");
            break;
        };

        match rtp::packet::Packet::unmarshal(&mut buffer.clone()) {
            // https://datatracker.ietf.org/doc/html/rfc5761#section-4
            //
            // Given these constraints, it is RECOMMENDED to follow the guidelines
            // in the RTP/AVP profile [7] for the choice of RTP payload type values,
            // with the additional restriction that payload type values in the range
            // 64-95 MUST NOT be used.
            Ok(packet) if packet.header.payload_type < 64 || packet.header.payload_type > 95 => {
                if packet.header.payload_type == 96 {
                    video_ssrc.get_or_insert(packet.header.ssrc);
                    if let Some(video) = &mut video {
                        video.jitter_buffer.write_packet(packet);
                        let sender = &video.handle.rtp_packet_sender;
                        while let Some(packet) = video.jitter_buffer.pop_packet() {
                            trace!(?packet, "Received video RTP packet");
                            if sender.send(PipelineEvent::Data(packet)).is_err() {
                                debug!("Channel closed");
                            }
                        }
                    }
                } else if packet.header.payload_type == 97 {
                    audio_ssrc.get_or_insert(packet.header.ssrc);
                    if let Some(audio) = &mut audio {
                        audio.jitter_buffer.write_packet(packet);
                        let sender = &audio.handle.rtp_packet_sender;
                        while let Some(packet) = audio.jitter_buffer.pop_packet() {
                            trace!(?packet, "Received audio RTP packet");
                            if sender.send(PipelineEvent::Data(packet)).is_err() {
                                debug!("Channel closed");
                            }
                        }
                    }
                }
            }
            Ok(_) | Err(_) => {
                match rtcp::packet::unmarshal(&mut buffer) {
                    Ok(rtcp_packets) => {
                        for rtcp_packet in rtcp_packets {
                            let header = rtcp_packet.header();
                            match header.packet_type {
                                PacketType::SenderReport => {
                                    let sender_report = rtcp_packet
                                        .as_any()
                                        .downcast_ref::<SenderReport>()
                                        .unwrap();

                                    if Some(sender_report.ssrc) == audio_ssrc
                                        && let Some(audio) = &mut audio
                                    {
                                        audio.jitter_buffer.on_sender_report(
                                            sender_report.ntp_time,
                                            sender_report.rtp_time,
                                        );
                                    }

                                    if Some(sender_report.ssrc) == video_ssrc
                                        && let Some(video) = &mut video
                                    {
                                        video.jitter_buffer.on_sender_report(
                                            sender_report.ntp_time,
                                            sender_report.rtp_time,
                                        );
                                    }
                                }
                                PacketType::Goodbye => {
                                    for ssrc in rtcp_packet.destination_ssrc() {
                                        if Some(ssrc) == audio_ssrc {
                                            maybe_send_audio_eos(&mut audio)
                                        }
                                        if Some(ssrc) == video_ssrc {
                                            maybe_send_video_eos(&mut video)
                                        }
                                    }
                                }
                                _ => {
                                    debug!(?header, "Received RTCP packet")
                                }
                            }
                        }
                    }
                    Err(err) => {
                        warn!(%err, "Received an unexpected packet, which is not recognized either as RTP or RTCP. Dropping.");
                    }
                }
                continue;
            }
        };
    }
    maybe_send_audio_eos(&mut audio);
    maybe_send_video_eos(&mut video);
}

impl From<BindToPortError> for RtpInputError {
    fn from(value: BindToPortError) -> Self {
        match value {
            BindToPortError::SocketBind(err) => RtpInputError::SocketBind(err),
            BindToPortError::PortAlreadyInUse(port) => RtpInputError::PortAlreadyInUse(port),
            BindToPortError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            } => RtpInputError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            },
        }
    }
}
