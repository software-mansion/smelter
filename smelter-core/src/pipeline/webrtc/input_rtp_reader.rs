use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc::Receiver;
use tracing::{debug, warn};
use webrtc::{
    dtls_transport::RTCDtlsTransport,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
};

use crate::{
    PipelineCtx,
    pipeline::{
        rtp::{RtpInputEvent, RtpJitterBuffer},
        webrtc::input_rtcp_listener::RtcpListeners,
    },
};

pub(super) struct WebrtcRtpReader {
    rtcp_listeners: RtcpListeners,
    jitter_buffer: RtpJitterBuffer,
    rtp_receiver: Receiver<webrtc::rtp::packet::Packet>,
    pli_sender: PliSender,
}

impl WebrtcRtpReader {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        track: Arc<TrackRemote>,
        rtc_receiver: Arc<RTCRtpReceiver>,
        jitter_buffer: RtpJitterBuffer,
    ) -> Self {
        let pli_sender = PliSender::new(&track, &rtc_receiver);
        let rtcp_listeners = RtcpListeners::start(ctx, rtc_receiver);
        let rtp_receiver = Self::start_rtp_reader_task(track);

        Self {
            rtcp_listeners,
            jitter_buffer,
            rtp_receiver,
            pli_sender,
        }
    }

    pub async fn enable_pli(&mut self) {
        self.pli_sender.enabled = true;
        self.pli_sender.try_send().await;
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
                if let RtpInputEvent::LostPacket = &packet {
                    self.pli_sender.try_send().await;
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

struct PliSender {
    transport: Arc<RTCDtlsTransport>,
    ssrc: u32,
    enabled: bool,
}

impl PliSender {
    fn new(track: &Arc<TrackRemote>, rtc_receiver: &Arc<RTCRtpReceiver>) -> Self {
        let ssrc = track.ssrc();
        let transport = rtc_receiver.transport();
        Self {
            transport,
            ssrc,
            enabled: false,
        }
    }

    async fn try_send(&self) {
        if !self.enabled {
            return;
        }

        debug!(ssrc = self.ssrc, "Sending PLI");
        let pli = PictureLossIndication {
            // For receive-only endpoints RTP sender SSRC can be set to 0.
            sender_ssrc: 0,
            media_ssrc: self.ssrc,
        };

        if let Err(err) = self.transport.write_rtcp(&[Box::new(pli)]).await {
            warn!(%err, "Failed to send RTCP packet (PictureLossIndication)")
        }
    }
}
