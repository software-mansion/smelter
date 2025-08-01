use anyhow::Result;

use rtp::{codecs::opus::OpusPacket, packetizer::Depacketizer};

#[derive(Debug, Clone, Copy)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

pub struct AudioDecoder {
    buffer: Vec<i16>,
    depayloader: OpusPacket,
    decoder: opus::Decoder,
    decoded_samples: Vec<i16>,
}

impl AudioDecoder {
    pub fn new(sample_rate: u32, channels: AudioChannels) -> Result<Self> {
        let channels = match channels {
            AudioChannels::Mono => opus::Channels::Mono,
            AudioChannels::Stereo => opus::Channels::Stereo,
        };
        let decoder = opus::Decoder::new(sample_rate, channels)?;

        Ok(Self {
            buffer: vec![0; sample_rate as usize * 20],
            depayloader: OpusPacket,
            decoder,
            decoded_samples: Vec::new(),
        })
    }

    pub fn decode(&mut self, packet: rtp::packet::Packet) -> Result<()> {
        let chunk_data = self.depayloader.depacketize(&packet.payload)?;
        if chunk_data.is_empty() {
            return Ok(());
        }

        let samples_count = self.decoder.decode(&chunk_data, &mut self.buffer, false)?;
        self.decoded_samples
            .extend(self.buffer[..samples_count].iter());

        Ok(())
    }

    pub fn take_samples(self) -> Vec<f32> {
        self.decoded_samples.into_iter().map(|s| s as f32).collect()
    }
}
