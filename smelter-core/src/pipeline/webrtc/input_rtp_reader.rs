use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc::Receiver;
use tracing::debug;
use webrtc::track::track_remote::TrackRemote;

use crate::pipeline::{
    rtp::{RtpJitterBuffer, RtpPacket},
    webrtc::input_rtcp_listener::RtcpListeners,
};

pub(super) struct WebrtcRtpReader {
    rtcp_listeners: RtcpListeners,
    jitter_buffer: RtpJitterBuffer,
    receiver: Receiver<webrtc::rtp::packet::Packet>,
}

impl WebrtcRtpReader {
    pub fn new(
        track: Arc<TrackRemote>,
        rtcp_listeners: RtcpListeners,
        jitter_buffer: RtpJitterBuffer,
    ) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        tokio::spawn(async move {
            loop {
                // read_rtp is not cancel safe so we need to create separate tasks that
                // sends packets over the channel
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
        Self {
            rtcp_listeners,
            jitter_buffer,
            receiver,
        }
    }

    pub async fn read_packet(&mut self) -> Option<RtpPacket> {
        loop {
            if let Some(packet) = self.jitter_buffer.pop_packet() {
                return Some(packet);
            }

            if let Ok(report) = self.rtcp_listeners.sender_report_receiver.try_recv() {
                self.jitter_buffer
                    .on_sender_report(report.ntp_time, report.rtp_time);
            }

            tokio::select! {
                packet = self.receiver.recv() => {
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
