use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
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
            RtpInputEvent, RtpJitterBuffer, RtpJitterBufferSharedContext,
            depayloader::DepayloaderOptions,
            rtp_input::{
                jitter_buffer::RtpJitterBufferMode,
                rtp_audio_thread::{
                    RtpAudioThread, RtpAudioThreadOptions, RtpAudioTrackThreadHandle,
                },
                rtp_video_thread::{RtpVideoThread, RtpVideoTrackThreadHandle},
            },
            util::BindToPortError,
        },
    },
    queue::{QueueInput, QueueTrackOffset, QueueTrackOptions},
    utils::InitializableThread,
};

use crate::prelude::*;

mod rollover_state;
mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

pub(super) mod jitter_buffer;
pub(super) mod rtcp_sync;

/// RTP input - receives RTP/RTCP packets over UDP or TCP, demuxes, and feeds
/// decoded frames/samples into the queue.
///
/// ## Timestamps
///
/// - Jitter buffer (fixed window)
///   - Buffer itself produces timestamps relative to `sync_point`.
///   - At the start it will wait for full window before returning anything.
/// - With offset (`opts.offset = Some(offset)`)
///   - PTS of first frame should be zero
///   - Register track with QueueTrackOffset::FromStart(offset)
///   - Timestamp from jitter buffer need to be normalized before sending them to queue
/// - Without offset (`opts.offset = None`)
///   - PTS of fist frame should be queue_sync_point.elapsed()
///   - Register track with QueueTrackOffset::Pts(Duration::ZERO)
///   - Jitter buffer is already producing correct timestamps
/// - On reconnect
///   - Can only be connected once
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
/// - If other input is required and delays queue by X relative to `queue_sync_point.elapsed()`:
///   - If X is smaller than channel sizes + socket buffers then, this input latency will
///     be artificially increased by X.
///   - If X is larger than channel size + socket buffers then, this input will be intermittently
///     blank and streaming until the other inputs (and queue processing) catch up.
pub struct RtpInput {
    should_close: Arc<AtomicBool>,
}

impl RtpInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: RtpInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Rtp,
        });

        let (port, raw_packets_receiver) = match opts.transport_protocol {
            RtpInputTransportProtocol::Udp => {
                start_udp_reader_thread(&input_ref, &opts, should_close.clone())?
            }
            RtpInputTransportProtocol::TcpServer => {
                start_tcp_server_thread(&input_ref, &opts, should_close.clone())?
            }
        };

        let buffer = opts.buffer_duration.unwrap_or(Duration::from_millis(80));
        let queue_input = QueueInput::new(&ctx, &input_ref, opts.required);

        // - For TCP + offset we don't need any buffer, but shifting
        //   by a constant does not change anything when offset is defined
        // - For TCP + no offset we don't need jitter buffer, just shifting PTS
        //   would be enough, but delay is the same so buffer is fine.
        let jitter_buffer_ctx = RtpJitterBufferSharedContext::new(
            &ctx,
            RtpJitterBufferMode::FixedWindow(buffer),
            // PTS will be relative to this value, they need to normalized
            // for case where offset is defined
            ctx.queue_ctx.sync_point,
        );

        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: opts.video.is_some(),
            audio: opts.audio.is_some(),
            offset: match opts.offset {
                Some(offset) => QueueTrackOffset::FromStart(offset),
                None => QueueTrackOffset::Pts(Duration::ZERO),
            },
        });

        let video_handle = Self::start_video_thread(&ctx, &input_ref, opts.video, video_sender)?;
        let audio_handle = Self::start_audio_thread(&ctx, &input_ref, opts.audio, audio_sender)?;

        // TODO: this could ran on the same thread as tcp/udp socket
        RtpDemuxerThread::spawn(
            ctx,
            &input_ref,
            jitter_buffer_ctx,
            raw_packets_receiver,
            video_handle,
            audio_handle,
            opts.offset.is_some(),
        );

        Ok((
            Input::Rtp(Self { should_close }),
            InputInitInfo::Rtp { port: Some(port) },
            queue_input,
        ))
    }

    fn start_video_thread(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        options: Option<VideoDecoderOptions>,
        frame_sender: Option<Sender<Frame>>,
    ) -> Result<Option<RtpVideoTrackThreadHandle>, DecoderInitError> {
        let (Some(options), Some(frame_sender)) = (options, frame_sender) else {
            return Ok(None);
        };

        let handle = match options {
            VideoDecoderOptions::FfmpegH264 => RtpVideoThread::<FfmpegH264Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::H264, frame_sender),
            )?,
            VideoDecoderOptions::FfmpegVp8 => RtpVideoThread::<FfmpegVp8Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::Vp8, frame_sender),
            )?,
            VideoDecoderOptions::FfmpegVp9 => RtpVideoThread::<FfmpegVp9Decoder>::spawn(
                input_ref.clone(),
                (ctx.clone(), DepayloaderOptions::Vp9, frame_sender),
            )?,
            VideoDecoderOptions::VulkanH264 => {
                if !ctx.graphics_context.has_vulkan_decoder_support() {
                    return Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder);
                }
                RtpVideoThread::<VulkanH264Decoder>::spawn(
                    input_ref.clone(),
                    (ctx.clone(), DepayloaderOptions::H264, frame_sender),
                )?
            }
        };
        Ok(Some(handle))
    }

    fn start_audio_thread(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        options: Option<RtpAudioOptions>,
        samples_sender: Option<Sender<InputAudioSamples>>,
    ) -> Result<Option<RtpAudioTrackThreadHandle>, DecoderInitError> {
        let (Some(options), Some(samples_sender)) = (options, samples_sender) else {
            return Ok(None);
        };

        let handle = match options {
            RtpAudioOptions::Opus => RtpAudioThread::<OpusDecoder>::spawn(
                input_ref,
                RtpAudioThreadOptions {
                    ctx: ctx.clone(),
                    sample_rate: 48_000,
                    decoder_options: (),
                    depayloader_options: DepayloaderOptions::Opus,
                    samples_sender,
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
                    samples_sender,
                },
            )?,
        };
        Ok(Some(handle))
    }
}

impl Drop for RtpInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct RtpDemuxerThread {
    tracks: Vec<TrackState>,
    receiver: Receiver<bytes::Bytes>,
    first_pts: Option<Duration>,
    has_offset: bool,
}

struct TrackState {
    payload_type: u8,
    ssrc: Option<u32>,
    jitter_buffer: RtpJitterBuffer,
    rtp_packet_sender: Sender<PipelineEvent<RtpInputEvent>>,
    eos_sent: bool,
}

impl RtpDemuxerThread {
    fn spawn(
        ctx: Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        jitter_buffer_ctx: RtpJitterBufferSharedContext,
        receiver: Receiver<bytes::Bytes>,
        video_handle: Option<RtpVideoTrackThreadHandle>,
        audio_handle: Option<RtpAudioTrackThreadHandle>,
        has_offset: bool,
    ) {
        let mut tracks: Vec<TrackState> = Vec::new();

        if let Some(handle) = video_handle {
            let stats_sender = ctx.stats_sender.clone();
            let ref_clone = input_ref.clone();
            tracks.push(TrackState {
                payload_type: 96,
                ssrc: None,
                jitter_buffer: RtpJitterBuffer::new(
                    jitter_buffer_ctx.clone(),
                    90_000,
                    Box::new(move |event| {
                        stats_sender
                            .send(RtpInputStatsEvent::VideoRtp(event).into_event(&ref_clone));
                    }),
                ),
                rtp_packet_sender: handle.rtp_packet_sender,
                eos_sent: false,
            });
        }

        if let Some(handle) = audio_handle {
            let stats_sender = ctx.stats_sender.clone();
            let ref_clone = input_ref.clone();
            let sample_rate = handle.sample_rate;
            tracks.push(TrackState {
                payload_type: 97,
                ssrc: None,
                jitter_buffer: RtpJitterBuffer::new(
                    jitter_buffer_ctx,
                    sample_rate,
                    Box::new(move |event| {
                        stats_sender
                            .send(RtpInputStatsEvent::AudioRtp(event).into_event(&ref_clone));
                    }),
                ),
                rtp_packet_sender: handle.rtp_packet_sender,
                eos_sent: false,
            });
        }

        let mut thread = Self {
            tracks,
            receiver,
            first_pts: None,
            has_offset,
        };

        let input_ref = input_ref.clone();
        std::thread::Builder::new()
            .name(format!("Depayloading thread for input: {input_ref}"))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "RTP demuxer", input_id = input_ref.to_string()).entered();
                thread.run();
            })
            .unwrap();
    }

    fn run(&mut self) {
        loop {
            let read_result = self.receiver.recv_timeout(Duration::from_millis(10));
            self.process_rtp_from_jitter_buffer();
            if self.tracks.iter().all(|track| track.eos_sent) {
                debug!("Closing RTP demuxer thread.");
                break;
            }

            let mut buffer = match read_result {
                Ok(buffer) => buffer,
                Err(RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    debug!("Closing RTP demuxer thread.");
                    break;
                }
            };

            // clone because unmarshal modifies the data
            match rtp::packet::Packet::unmarshal(&mut buffer.clone()) {
                // https://datatracker.ietf.org/doc/html/rfc5761#section-4
                //
                // Given these constraints, it is RECOMMENDED to follow the guidelines
                // in the RTP/AVP profile [7] for the choice of RTP payload type values,
                // with the additional restriction that payload type values in the range
                // 64-95 MUST NOT be used.
                Ok(packet)
                    if packet.header.payload_type < 64 || packet.header.payload_type > 95 =>
                {
                    self.handle_new_rtp_packet(packet);
                }
                Ok(_) | Err(_) => match rtcp::packet::unmarshal(&mut buffer) {
                    Ok(rtcp_packets) => {
                        for rtcp_packet in rtcp_packets {
                            self.handle_new_rtcp_packet(rtcp_packet);
                        }
                    }
                    Err(err) => {
                        warn!(%err, "Received an unexpected packet, which is not recognized either as RTP or RTCP. Dropping.");
                    }
                },
            };
            self.process_rtp_from_jitter_buffer();
        }
        for track in &mut self.tracks {
            track.send_eos();
        }
    }

    fn handle_new_rtp_packet(&mut self, packet: rtp::packet::Packet) {
        let pt = packet.header.payload_type;
        if let Some(track) = self.tracks.iter_mut().find(|t| t.payload_type == pt) {
            track.ssrc.get_or_insert(packet.header.ssrc);
            track.jitter_buffer.write_packet(packet);
        }
    }

    fn handle_new_rtcp_packet(&mut self, rtcp_packet: Box<dyn rtcp::packet::Packet + Send + Sync>) {
        let header = rtcp_packet.header();
        debug!(?header, "Received RTCP packet");
        match header.packet_type {
            PacketType::SenderReport => {
                let sender_report = rtcp_packet.as_any().downcast_ref::<SenderReport>().unwrap();
                for track in &mut self.tracks {
                    if track.ssrc == Some(sender_report.ssrc) {
                        track
                            .jitter_buffer
                            .on_sender_report(sender_report.ntp_time, sender_report.rtp_time);
                    }
                }
            }
            PacketType::Goodbye => {
                self.flush_rtp_from_jitter_buffer();
                for ssrc in rtcp_packet.destination_ssrc() {
                    for track in &mut self.tracks {
                        if track.ssrc == Some(ssrc) {
                            track.send_eos();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn process_rtp_from_jitter_buffer(&mut self) {
        // Pop from the track with the lowest PTS first to interleave audio/video in order.
        loop {
            let next_track = self
                .tracks
                .iter_mut()
                .filter_map(|t| t.jitter_buffer.peek_next_pts().map(|pts| (t, pts)))
                .min_by_key(|(_, pts)| *pts)
                .map(|(t, _)| t);
            let Some(track) = next_track else {
                break;
            };
            let Some(packet) = track.jitter_buffer.try_read_packet() else {
                break;
            };
            track.send_packet(packet, &mut self.first_pts, self.has_offset);
        }
        // If a track's pop returned None (not ready), other tracks might still have
        // ready packets. Drain them individually.
        for track in &mut self.tracks {
            while let Some(packet) = track.jitter_buffer.try_read_packet() {
                track.send_packet(packet, &mut self.first_pts, self.has_offset);
            }
        }
    }

    fn flush_rtp_from_jitter_buffer(&mut self) {
        // Pop from the track with the lowest PTS first to interleave audio/video in order.
        loop {
            let next_track = self
                .tracks
                .iter_mut()
                .filter_map(|t| t.jitter_buffer.peek_next_pts().map(|pts| (t, pts)))
                .min_by_key(|(_, pts)| *pts)
                .map(|(t, _)| t);
            let Some(track) = next_track else {
                break;
            };
            let Some(packet) = track.jitter_buffer.read_packet() else {
                break;
            };
            track.send_packet(packet, &mut self.first_pts, self.has_offset);
        }
    }
}

impl TrackState {
    fn send_packet(
        &mut self,
        event: RtpInputEvent,
        first_pts: &mut Option<Duration>,
        has_offset: bool,
    ) {
        let event = match event {
            RtpInputEvent::Packet(mut packet) if has_offset => {
                let first_pts = *first_pts.get_or_insert(packet.timestamp);
                packet.timestamp = packet.timestamp.saturating_sub(first_pts);
                RtpInputEvent::Packet(packet)
            }
            event => event,
        };

        trace!(?event, "Sending RTP packet to decoder");
        let event = PipelineEvent::Data(event);
        if self.rtp_packet_sender.send(event).is_err() {
            debug!("Failed to send event. Channel closed");
            self.eos_sent = true;
        }
    }

    fn send_eos(&mut self) {
        if !self.eos_sent {
            self.eos_sent = true;
            if self.rtp_packet_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS. Channel closed.");
            }
        }
    }
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
