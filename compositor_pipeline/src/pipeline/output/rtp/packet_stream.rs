use tracing::error;
use webrtc_util::Marshal;

use crossbeam_channel::Receiver;

use super::RtpEvent;

pub(super) struct RtpBinaryPacketStream {
    pub receiver: Receiver<RtpEvent>,
    pub waiting_audio_eos: bool,
    pub waiting_video_eos: bool,
}

impl Iterator for RtpBinaryPacketStream {
    type Item = Vec<bytes::Bytes>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = match self.receiver.recv() {
            Ok(RtpEvent::Data(packet)) => match packet.marshal() {
                Ok(data) => Some(vec![data]),
                Err(err) => {
                    error!("Failed to marshal an RTP packet: {}", err);
                    Some(Vec::new())
                }
            },
            Ok(RtpEvent::AudioEos) => {
                self.waiting_audio_eos = false;
                Some(Vec::new())
            }
            Ok(RtpEvent::VideoEos) => {
                self.waiting_video_eos = false;
                Some(Vec::new())
            }
            Ok(RtpEvent::Err(err)) => {
                error!("Failed to payload a packet: {}", err);
                Some(Vec::new())
            }
            Err(_) => None,
        };
        match self.waiting_video_eos || self.waiting_audio_eos {
            true => result,
            false => None,
        }
    }
}
