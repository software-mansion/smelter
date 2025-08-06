use std::time::Duration;

use anyhow::Result;

use rtp::{codecs::opus::OpusPacket, packetizer::Depacketizer};

#[derive(Clone)]
pub struct AudioSampleBatch {
    pub samples: Vec<f32>,
    pub pts: Duration,
}

#[derive(Debug, Clone, Copy)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

pub struct AudioDecoder {
    buffer: Vec<i16>,
    depayloader: OpusPacket,
    decoder: opus::Decoder,
    decoded_samples: Vec<AudioSampleBatch>,
    sample_rate: u32,
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
            decoded_samples: vec![],
            sample_rate,
        })
    }

    pub fn decode(&mut self, packet: rtp::packet::Packet) -> Result<()> {
        let chunk_data = self.depayloader.depacketize(&packet.payload)?;
        if chunk_data.is_empty() {
            return Ok(());
        }

        let samples_count = self.decoder.decode(&chunk_data, &mut self.buffer, false)?;
        self.decoded_samples.push(AudioSampleBatch {
            samples: self.batch_to_f32(&self.buffer[..samples_count]),
            pts: Duration::from_secs_f64(packet.header.timestamp as f64 / self.sample_rate as f64),
        });

        Ok(())
    }

    pub fn take_samples(self) -> Vec<AudioSampleBatch> {
        self.decoded_samples
    }

    fn batch_to_f32(&self, batch: &[i16]) -> Vec<f32> {
        batch.iter().map(|s| *s as f32).collect()
    }
}
