use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc::Receiver;
use tracing::{Instrument, debug, warn};
use webrtc::{
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
};

use crate::{
    PipelineCtx,
    pipeline::{
        decoder::KeyframeRequestSender,
        rtp::{RtpInputEvent, RtpJitterBuffer},
        webrtc::input_rtcp_listener::RtcpListeners,
    },
};

pub(super) struct WebrtcRtpReader {
    track: Arc<TrackRemote>,
    rtc_receiver: Arc<RTCRtpReceiver>,
    rtcp_listeners: RtcpListeners,
    jitter_buffer: RtpJitterBuffer,
    rtp_receiver: Receiver<webrtc::rtp::packet::Packet>,
    keyframe_request_sender: Option<KeyframeRequestSender>,
}

impl WebrtcRtpReader {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        track: Arc<TrackRemote>,
        rtc_receiver: Arc<RTCRtpReceiver>,
        jitter_buffer: RtpJitterBuffer,
    ) -> Self {
        let rtcp_listeners = RtcpListeners::start(ctx, rtc_receiver.clone());
        let rtp_receiver = Self::start_rtp_reader_task(track.clone());

        Self {
            track,
            rtc_receiver,
            rtcp_listeners,
            jitter_buffer,
            rtp_receiver,
            keyframe_request_sender: None,
        }
    }

    pub async fn enable_pli(&mut self) -> KeyframeRequestSender {
        let sender = start_pli_sender_task(&self.track, &self.rtc_receiver);
        self.keyframe_request_sender = Some(sender.clone());
        sender.send();
        sender
    }

    /// read_rtp is not cancel safe so we need to create separate tasks that
    /// sends packets over the channel
    fn start_rtp_reader_task(track: Arc<TrackRemote>) -> Receiver<webrtc::rtp::packet::Packet> {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        tokio::spawn(async move {
            loop {
                let packet = match track.read_rtp().await {
                    Ok((packet, _)) => packet,
                    Err(err) => {
                        debug!(?err, "Failed to read next RTP packet");
                        break;
                    }
                };
                if sender.send(packet).await.is_err() {
                    break;
                }
            }
        });
        receiver
    }

    pub async fn read_packet(&mut self) -> Option<RtpInputEvent> {
        loop {
            if let Some(packet) = self.jitter_buffer.pop_packet() {
                if let (RtpInputEvent::LostPacket, Some(sender)) =
                    (&packet, &self.keyframe_request_sender)
                {
                    sender.send()
                };
                return Some(packet);
            }

            if let Ok(report) = self.rtcp_listeners.sender_report_receiver.try_recv() {
                self.jitter_buffer
                    .on_sender_report(report.ntp_time, report.rtp_time);
            }

            tokio::select! {
                packet = self.rtp_receiver.recv() => {
                    match packet {
                        Some(packet) => {
                            self.jitter_buffer.write_packet(packet);
                        },
                        None => {
                            return None
                        },
                    }
                },
                _ = tokio::time::sleep(Duration::from_millis(10)) => ()
            };
        }
    }
}

pub fn start_pli_sender_task(
    track: &Arc<TrackRemote>,
    rtc_receiver: &Arc<RTCRtpReceiver>,
) -> KeyframeRequestSender {
    let (keyframe_request_sender, mut keyframe_request_receiver) =
        KeyframeRequestSender::new_async();
    let ssrc = track.ssrc();
    let transport = rtc_receiver.transport();
    tokio::spawn(
        async move {
            while keyframe_request_receiver.recv().await.is_some() {
                debug!(ssrc, "Sending PLI");
                let pli = PictureLossIndication {
                    // For receive-only endpoints RTP sender SSRC can be set to 0.
                    sender_ssrc: 0,
                    media_ssrc: ssrc,
                };

                if let Err(err) = transport.write_rtcp(&[Box::new(pli)]).await {
                    warn!(%err, "Failed to send RTCP packet (PictureLossIndication)")
                }
                tokio::time::sleep(Duration::from_secs(1)).await
            }
        }
        .instrument(tracing::Span::current()),
    );

    keyframe_request_sender
}
