use std::{
    i128,
    sync::{
        atomic::{AtomicBool},
        Arc,  RwLock,
    },
    time::{Duration, Instant},
};

use compositor_render::{Frame, InputId};
use crossbeam_channel::{bounded, Receiver};
use rtcp::header::PacketType;
use tracing::{debug, span, trace, warn, Level};
use webrtc_util::Unmarshal;

use self::{tcp_server::start_tcp_server_thread, udp::start_udp_reader_thread};

use crate::{
    pipeline::rtp::rtp_input::{
        rtp_audio_thread::{RtpAudioThread, RtpAudioThreadOptions},
        rtp_video_thread::RtpVideoThread,
    },
    prelude::*,
    thread_utils::InitializableThread,
};
use crate::{
    pipeline::{
        decoder::{
            fdk_aac::FdkAacDecoder, ffmpeg_h264::FfmpegH264Decoder, ffmpeg_vp8::FfmpegVp8Decoder,
            ffmpeg_vp9::FfmpegVp9Decoder, libopus::OpusDecoder, vulkan_h264::VulkanH264Decoder,
        },
        input::Input,
        rtp::{
            depayloader::DepayloaderOptions,
            rtp_input::{
                rtp_audio_thread::RtpAudioTrackThreadHandle,
                rtp_video_thread::RtpVideoTrackThreadHandle,
            },
            util::BindToPortError,
            RtpPacket,
        },
    },
    queue::QueueDataReceiver,
};

mod rollover_state;
mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

pub(crate) use rollover_state::RolloverState;

pub struct RtpInput {
    should_close: Arc<AtomicBool>,
}

impl RtpInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: RtpInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));

        let (port, raw_packets_receiver) = match opts.transport_protocol {
            RtpInputTransportProtocol::Udp => {
                start_udp_reader_thread(&input_id, &opts, should_close.clone())?
            }
            RtpInputTransportProtocol::TcpServer => {
                start_tcp_server_thread(&input_id, &opts, should_close.clone())?
            }
        };

        let (video_handle, video_frames_receiver) =
            Self::start_video_thread(&ctx, &input_id, opts.video)?;

        let (audio_handle, audio_samples_receiver) =
            Self::start_audio_thread(&ctx, &input_id, opts.audio)?;

        // TODO: this could ran on the same thread as tcp/udp socket
        Self::start_rtp_demuxer_thread(
            &input_id,
            ctx.queue_sync_point,
            opts.buffer_duration.unwrap_or(ctx.default_buffer_duration),
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
        input_id: &InputId,
        sync_point: Instant,
        buffer_duration: Duration,
        receiver: Receiver<bytes::Bytes>,
        audio: Option<RtpAudioTrackThreadHandle>,
        video: Option<RtpVideoTrackThreadHandle>,
    ) {
        let input_id = input_id.clone();
        std::thread::Builder::new()
            .name(format!("Depayloading thread for input: {}", input_id.0))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "RTP demuxer", input_id = input_id.to_string()).entered();
                run_rtp_demuxer_thread(sync_point, buffer_duration, receiver, video, audio)
            })
            .unwrap();
    }

    #[allow(clippy::type_complexity)]
    fn start_video_thread(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
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
                input_id.clone(),
                (ctx.clone(), DepayloaderOptions::H264, sender),
            )?,
            VideoDecoderOptions::FfmpegVp8 => RtpVideoThread::<FfmpegVp8Decoder>::spawn(
                input_id.clone(),
                (ctx.clone(), DepayloaderOptions::Vp8, sender),
            )?,
            VideoDecoderOptions::FfmpegVp9 => RtpVideoThread::<FfmpegVp9Decoder>::spawn(
                input_id.clone(),
                (ctx.clone(), DepayloaderOptions::Vp9, sender),
            )?,
            VideoDecoderOptions::VulkanH264 => RtpVideoThread::<VulkanH264Decoder>::spawn(
                input_id.clone(),
                (ctx.clone(), DepayloaderOptions::H264, sender),
            )?,
        };
        Ok((Some(handle), Some(receiver)))
    }

    #[allow(clippy::type_complexity)]
    fn start_audio_thread(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
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
                input_id,
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
                input_id,
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
    sync_point: Instant,
    buffer_duration: Duration,
    receiver: Receiver<bytes::Bytes>,
    video_handle: Option<RtpVideoTrackThreadHandle>,
    audio_handle: Option<RtpAudioTrackThreadHandle>,
) {
    struct TrackState<Handle> {
        handle: Handle,
        time_sync: RtpTimestampSync,
        eos_received: bool,
    }

    let sync_point = RtpNtpSyncPoint::new(sync_point);

    let mut audio = audio_handle.map(|handle| TrackState {
        time_sync: RtpTimestampSync::new(&sync_point, handle.sample_rate, buffer_duration),
        handle,
        eos_received: false,
    });
    let mut video = video_handle.map(|handle| TrackState {
        time_sync: RtpTimestampSync::new(&sync_point, 90_000, buffer_duration),
        handle,
        eos_received: false,
    });

    let mut audio_ssrc = None;
    let mut video_ssrc = None;

    let maybe_send_video_eos = |video: &mut Option<TrackState<RtpVideoTrackThreadHandle>>| {
        if let Some(video) = video {
            if !video.eos_received {
                video.eos_received = true;
                let sender = &video.handle.rtp_packet_sender;
                if sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from RTP video depayloader. Channel closed.");
                }
            }
        }
    };
    let maybe_send_audio_eos = |audio: &mut Option<TrackState<RtpAudioTrackThreadHandle>>| {
        if let Some(audio) = audio {
            if !audio.eos_received {
                audio.eos_received = true;
                let sender = &audio.handle.rtp_packet_sender;
                if sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from RTP audio depayloader. Channel closed.");
                }
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
                        let timestamp = video.time_sync.timestamp(packet.header.timestamp);
                        let sender = &video.handle.rtp_packet_sender;
                        trace!(?timestamp, packet=?packet.header, "Received video RTP packet");
                        if sender
                            .send(PipelineEvent::Data(RtpPacket { packet, timestamp }))
                            .is_err()
                        {
                            debug!("Channel closed");
                            continue;
                        }
                    }
                } else if packet.header.payload_type == 97 {
                    audio_ssrc.get_or_insert(packet.header.ssrc);
                    if let Some(audio) = &mut audio {
                        let timestamp = audio.time_sync.timestamp(packet.header.timestamp);
                        let sender = &audio.handle.rtp_packet_sender;
                        trace!(?timestamp, packet=?packet.header, "Received audio RTP packet");
                        if sender
                            .send(PipelineEvent::Data(RtpPacket { packet, timestamp }))
                            .is_err()
                        {
                            debug!("Channel closed");
                            continue;
                        }
                    }
                }
            }
            Ok(_) | Err(_) => {
                match rtcp::packet::unmarshal(&mut buffer) {
                    Ok(rtcp_packets) => {
                        for rtcp_packet in rtcp_packets {
                            if let PacketType::Goodbye = rtcp_packet.header().packet_type {
                                for ssrc in rtcp_packet.destination_ssrc() {
                                    if Some(ssrc) == audio_ssrc {
                                        maybe_send_audio_eos(&mut audio)
                                    }
                                    if Some(ssrc) == video_ssrc {
                                        maybe_send_video_eos(&mut video)
                                    }
                                }
                            } else {
                                debug!(
                                    packet_type=?rtcp_packet.header().packet_type,
                                    "Received RTCP packet"
                                )
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

const POW_2_32: f64 = (1i64 << 32) as f64;

#[derive(Debug)]
pub struct RtpNtpSyncPoint {
    sync_point: Instant,
    /// First 32 bytes represent seconds, last 32 bytes fraction of the second.
    /// Represents NTP time of sync point
    ntp_time: RwLock<Option<u64>>,
}

impl RtpNtpSyncPoint {
    pub fn new(sync_point: Instant) -> Arc<Self> {
        Self { sync_point, ntp_time: RwLock::new(None) }.into()
    }

    pub fn ntp_time_to_pts(&self, ntp_time: u64) -> Duration {
        let sync_point_ntp_time = self.ntp_time.read().unwrap().unwrap_or(0) as i128;

        let ntp_time_diff_secs = (ntp_time as i128 - sync_point_ntp_time) as f64 / POW_2_32;

            warn!(
                ntp_time_diff_secs ,
                ntp_time= (ntp_time as f64/ POW_2_32),
                sync_point_ntp_time=(sync_point_ntp_time as f64 / POW_2_32) , "PTS from NTP time ");
        Duration::try_from_secs_f64(ntp_time_diff_secs).unwrap_or_else(|err| {
            warn!(%err, "NTP time from before sync point");
            Duration::ZERO
        })
    }

    /// ntp_time - absolute time from RTP packet
    /// rtp_timestamp - rtp timestamp that represents the same time as ntp_time
    /// cmp_rtp_timestamp - rtp timestamp of some RTP packet
    /// cmp_pts - pts(duration from sync_point without buffer) representing above packet
    pub fn ensure_sync_info(
        &self,
        ntp_time: u64,
        rtp_timestamp: u32,
        cmp_rtp_timestamp: u32,
        cmp_pts: Duration,
        clock_rate: u32,
    ) {
        //{
        //    let guard = self.ntp_time.read().unwrap();
        //    if guard.is_some() {
        //        return;
        //    }
        //}

        let mut guard = self.ntp_time.write().unwrap();
        let rtp_diff_secs = (cmp_rtp_timestamp as f64 - rtp_timestamp as f64) / clock_rate as f64;

        let sync_point_ntp_time = ntp_time as i128 
            + (rtp_diff_secs * POW_2_32) as i128 // ntp time of cmp packet
            - (cmp_pts.as_secs_f64() * POW_2_32) as i128; // ntp_time of sync_point
                                                          //
        warn!(
            rtp_diff_secs, 
            ntp_time, 
            rtp_diff_in_ntp=(rtp_diff_secs * POW_2_32), 
            ?cmp_pts,
            cmp_pts_in_ntp=(cmp_pts.as_secs_f64() * POW_2_32),
            pow=POW_2_32,
            sync_point_ntp_time,
            old_value=?*guard,
            "timestamps"
        );
        if guard.is_none() {
            *guard = Some(sync_point_ntp_time as u64);
        }
    }
}

#[derive(Debug)]
enum PartialSyncInfo {
    Synced,
    None,
    FirstPacket {
        /// timestamp of some RTP packet
        rtp_timestamp: u32,
        /// pts(duration from sync_point without buffer) representing above packet
        pts: Duration,
    },
    SenderReport {
        ntp_time: u64,
        /// rtp timestamp that represents the same time as ntp_time
        rtp_timestamp: u32,
    },
}

#[derive(Debug)]
pub struct RtpTimestampSync {
    // offset to sync timestamps to zero (and at the same time PTS of the first packet)
    rtp_timestamp_offset: Option<u64>,
    // offset to sync final duration to sync_point, assuming
    // that pts of first packet was zero.
    //
    // Calculation:
    // - best effort at start: elapsed since sync point on first packet
    // - after sync:
    //   - get pts of some packet from RtpNtpSyncPoint
    //   - calculate pts of first packet based on the difference
    //   - pts of first packet is an offset
    sync_offset: Option<Duration>,
    // additional buffer that defines how much input start should be ahead
    // of the queue.
    buffer_duration: Duration,
    clock_rate: u32,
    rollover_state: RolloverState,

    sync_point: Arc<RtpNtpSyncPoint>,
    partial_sync_info: PartialSyncInfo,
}

impl RtpTimestampSync {
    pub fn new(
        sync_point: &Arc<RtpNtpSyncPoint>,
        clock_rate: u32,
        buffer_duration: Duration,
    ) -> Self {
        Self {
            sync_offset: None,
            rtp_timestamp_offset: None,
            buffer_duration,

            clock_rate,
            rollover_state: Default::default(),

            sync_point: sync_point.clone(),
            partial_sync_info: PartialSyncInfo::None,
        }
    }

    pub fn timestamp(&mut self, rtp_timestamp: u32) -> Duration {
        let sync_offset = *self.sync_offset.get_or_insert_with(|| {
            let sync_offset = self.sync_point.sync_point.elapsed();
            debug!(
                ?sync_offset,
                initial_rtp_timestamp = rtp_timestamp,
                "Init offset from sync point"
            );
            sync_offset
        });

        let rolled_timestamp = self.rollover_state.timestamp(rtp_timestamp);

        let rtp_timestamp_offset = *self.rtp_timestamp_offset.get_or_insert(rolled_timestamp);

        let timestamp = rolled_timestamp - rtp_timestamp_offset;
        let next_pts =
            Duration::from_secs_f64(timestamp as f64 / self.clock_rate as f64) + sync_offset;

        match self.partial_sync_info {
            PartialSyncInfo::None => {
                self.partial_sync_info = PartialSyncInfo::FirstPacket {
                    rtp_timestamp,
                    pts: next_pts,
                }
            }
            PartialSyncInfo::SenderReport {
                ntp_time: sr_ntp_time,
                rtp_timestamp: sr_rtp_timestamp,
            } => {
                panic!("I don't think this will ever happen");
                self.sync_point.ensure_sync_info(
                    sr_ntp_time,
                    sr_rtp_timestamp,
                    rtp_timestamp,
                    next_pts,
                    self.clock_rate,
                );
                self.partial_sync_info = PartialSyncInfo::Synced;
                self.update_sync_offset(sr_ntp_time, sr_rtp_timestamp);
            }
            _ => (),
        }

        let next_pts = next_pts + self.buffer_duration;
        trace!(?next_pts, "New PTS from synchronizer");
        next_pts
    }

    pub fn on_sender_report(&mut self, ntp_time: u64, rtp_timestamp: u32) {
        warn!(ntp_time, rtp_timestamp, "on_sender_report");
        match self.partial_sync_info {
            PartialSyncInfo::Synced => return,
            PartialSyncInfo::None => {
                self.partial_sync_info = PartialSyncInfo::SenderReport {
                    ntp_time,
                    rtp_timestamp,
                }
            }
            PartialSyncInfo::FirstPacket {
                rtp_timestamp: cmp_rtp_timestamp,
                pts: cmp_pts,
            } => {
                self.sync_point.ensure_sync_info(
                    ntp_time,
                    rtp_timestamp,
                    cmp_rtp_timestamp,
                    cmp_pts,
                    self.clock_rate,
                );
               // self.partial_sync_info = PartialSyncInfo::Synced;
                self.update_sync_offset(ntp_time, rtp_timestamp);
            }
            PartialSyncInfo::SenderReport { .. } => return,
        }
    }

    fn update_sync_offset(&mut self, ntp_time: u64, rtp_timestamp: u32) {
        let Some(first_rtp_timestamp) = self.rtp_timestamp_offset else {
            warn!("Updating sync offset before first packet, This should not happen.");
            return;
        };
        // pts representing `rtp_timestamp`
        let pts = self.sync_point.ntp_time_to_pts(ntp_time);
        let pts_diff_secs =
            (first_rtp_timestamp as i64 - rtp_timestamp as i64) as f64 / self.clock_rate as f64;

        warn!(
            old=?self.sync_offset,
            new=?Some(Duration::from_secs_f64(pts.as_secs_f64() + pts_diff_secs)),
            ?pts,
            pts_diff_secs,
            first_rtp_timestamp,
            rtp_timestamp,
            "Update sync offset"
            );
        self.sync_offset = Some(Duration::from_secs_f64(pts.as_secs_f64() + pts_diff_secs))
    }
}
