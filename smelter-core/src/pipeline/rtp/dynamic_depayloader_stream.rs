use smelter_render::error::ErrorStack;
use tracing::{error, warn};
use webrtc::{rtp, rtp_transceiver::PayloadType};

use crate::pipeline::{
    decoder::negotiated_codecs::NegotiatedVideoCodecsInfo,
    rtp::{
        depayloader::{new_depayloader, Depayloader, DepayloaderOptions, DepayloadingError},
        RtpPacket,
    },
};

use crate::prelude::*;

pub(crate) struct DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    depayloader: Option<Box<dyn Depayloader>>,
    last_payload_type: Option<PayloadType>,
    source: Source,
    eos_sent: bool,
    codec_info: NegotiatedVideoCodecsInfo,
}

impl<Source> DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    pub(crate) fn new(codec_info: NegotiatedVideoCodecsInfo, source: Source) -> Self {
        Self {
            source,
            eos_sent: false,
            codec_info,
            depayloader: None,
            last_payload_type: None,
        }
    }

    fn ensure_depayloader(&mut self, payload_type: PayloadType) {
        if self.last_payload_type == Some(payload_type) {
            return;
        }
        self.last_payload_type = Some(payload_type);
        if self.codec_info.is_payload_type_h264(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::H264));
        } else if self.codec_info.is_payload_type_vp8(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::Vp8));
        } else if self.codec_info.is_payload_type_vp9(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::Vp9));
        } else {
            error!("Failed to create depayloader for payload_type: {payload_type}")
        }
    }
}

impl<Source> Iterator for DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    type Item = Vec<PipelineEvent<EncodedInputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(packet)) => {
                self.ensure_depayloader(packet.packet.header.payload_type);
                let depayloader = self.depayloader.as_mut()?;
                match depayloader.depayload(packet) {
                    Ok(chunks) => Some(chunks.into_iter().map(PipelineEvent::Data).collect()),
                    // TODO: Remove after updating webrc-rs
                    Err(DepayloadingError::Rtp(rtp::Error::ErrShortPacket)) => Some(vec![]),
                    Err(err) => {
                        warn!("Depayloader error: {}", ErrorStack::new(&err).into_string());
                        Some(vec![])
                    }
                }
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
    }
}
