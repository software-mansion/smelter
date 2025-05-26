use std::collections::VecDeque;

use crossbeam_channel::Receiver;

use crate::{pipeline::types::EncoderOutputEvent, queue::PipelineEvent};

use super::payloader::{Payloader, PayloadingError};

pub(super) struct PacketStream {
    packets_receiver: Receiver<EncoderOutputEvent>,
    state: VecDeque<bytes::Bytes>,
    audio_payloader: Payloader,
    video_payloader: Payloader,
}

impl PacketStream {
    pub(super) fn new(
        packets_receiver: Receiver<EncoderOutputEvent>,
        audio_payloader: Payloader,
        video_payloader: Payloader,
        mtu: usize,
    ) -> Self {
        Self {
            packets_receiver,
            audio_payloader,
            video_payloader,
            state: VecDeque::new(),
        }
    }

    fn next_packet(&mut self) -> Result<PipelineEvent<Vec<rtp::packet::Packet>>, PayloadingError> {
        //let Ok(packet) = self.packets_receiver.recv() else {
        //    // Send audio and video EOS if payloaders are supported and EOS was not sent before.
        //    match self.payloader.audio_eos() {
        //        Err(PayloadingError::NoAudioPayloader) => (),
        //        Err(PayloadingError::AudioEOSAlreadySent) => (),
        //        packet => return Some(packet),
        //    }
        //    match self.payloader.video_eos() {
        //        Err(PayloadingError::NoVideoPayloader) => (),
        //        Err(PayloadingError::VideoEOSAlreadySent) => (),
        //        packet => return Some(packet),
        //    }
        //    return None;
        //};
        match self.packets_receiver.recv() {
            Ok(pac) => todo!(),
            Err(_) => todo!(),
        }

        let encoded_chunk = match packet {
            EncoderOutputEvent::Data(packet) => packet,
            EncoderOutputEvent::AudioEOS => return Some(self.payloader.audio_eos()),
            EncoderOutputEvent::VideoEOS => return Some(self.payloader.video_eos()),
        };

        let rtp_packets = match self.payloader.payload(self.mtu, encoded_chunk) {
            Ok(packets) => packets,
            Err(err) => return Some(Err(err)),
        };

        // I'm assuming here that payload will never return empty list
        self.state = rtp_packets;
        self.state.pop_front().map(Ok)
    }
}

impl Iterator for PacketStream {
    type Item = Result<bytes::Bytes, PayloadingError>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            chunks if chunks.is_empty() => self.next_new_packet(),
            chunks => chunks.pop_front().map(Ok),
        }
    }
}
