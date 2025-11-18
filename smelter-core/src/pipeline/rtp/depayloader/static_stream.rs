use smelter_render::error::ErrorStack;
use tracing::debug;

use crate::pipeline::{
    decoder::EncodedInputEvent,
    rtp::{
        RtpInputEvent,
        depayloader::{Depayloader, DepayloaderOptions, new_depayloader},
    },
};

use crate::prelude::*;

pub(crate) struct DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpInputEvent>>,
{
    depayloader: Box<dyn Depayloader>,
    source: Source,
    eos_sent: bool,
}

impl<Source> DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpInputEvent>>,
{
    pub fn new(options: DepayloaderOptions, source: Source) -> Self {
        Self {
            depayloader: new_depayloader(options),
            source,
            eos_sent: false,
        }
    }
}

impl<Source> Iterator for DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpInputEvent>>,
{
    type Item = Vec<PipelineEvent<EncodedInputEvent>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(RtpInputEvent::Packet(packet))) => {
                match self.depayloader.depayload(packet) {
                    Ok(chunks) => Some(
                        chunks
                            .into_iter()
                            .map(|chunk| PipelineEvent::Data(EncodedInputEvent::Chunk(chunk)))
                            .collect(),
                    ),
                    Err(err) => {
                        debug!("Depayloader error: {}", ErrorStack::new(&err).into_string());
                        Some(vec![])
                    }
                }
            }
            Some(PipelineEvent::Data(RtpInputEvent::LostPacket)) => {
                Some(vec![PipelineEvent::Data(EncodedInputEvent::LostData)])
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
