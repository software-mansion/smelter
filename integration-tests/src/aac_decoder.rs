//! AAC decoder for MP4 dumps, built on ffmpeg.
//!
//! smelter's MP4 output stores raw AAC access units plus an
//! `AudioSpecificConfig` (ASC) in the container. ffmpeg's AAC decoder
//! happily auto-detects ADTS-framed input without needing extradata,
//! so rather than threading the ASC through as codec extradata we wrap
//! each sample in a 7-byte ADTS header synthesised from the ASC and
//! feed that to the decoder.
//!
//! Output mirrors [`crate::audio_decoder`]: interleaved stereo `f32`
//! samples in i16 amplitude scale, grouped into per-sample-batch
//! [`AudioSampleBatch`]es carrying their presentation timestamp, so the
//! same comparison and inspection code works regardless of dump format.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use ffmpeg_next::{
    Rational,
    codec::{Context as FfmpegContext, Id},
    format::sample::{Sample, Type},
    frame,
    media::Type as MediaType,
};

use crate::audio_decoder::AudioSampleBatch;

pub struct AacDecoder {
    decoder: ffmpeg_next::codec::decoder::Opened,
    adts: AdtsConfig,
    decoded_samples: Vec<AudioSampleBatch>,
}

impl AacDecoder {
    /// Build a decoder from the track's `AudioSpecificConfig`.
    pub fn new(asc: &Bytes) -> Result<Self> {
        let adts = AdtsConfig::from_asc(asc)?;

        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();
            parameters.codec_type = MediaType::Audio.into();
            parameters.codec_id = Id::AAC.into();
        }

        let mut decoder = FfmpegContext::from_parameters(parameters)?;
        unsafe {
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, 1_000_000).into();
        }
        let decoder = decoder.decoder().open_as(Id::AAC)?;

        Ok(Self {
            decoder,
            adts,
            decoded_samples: Vec::new(),
        })
    }

    /// Decode a single raw AAC access unit at the given timestamp.
    pub fn decode(&mut self, data: &[u8], pts: Duration) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let framed = self.adts.frame(data);
        let mut packet = ffmpeg_next::Packet::new(framed.len());
        packet.data_mut().unwrap().copy_from_slice(&framed);
        packet.set_pts(Some(pts.as_micros() as i64));
        packet.set_dts(None);
        self.decoder.send_packet(&packet)?;
        self.receive_decoded_samples()
    }

    pub fn take_samples(mut self) -> Result<Vec<AudioSampleBatch>> {
        self.decoder.send_eof().ok();
        self.receive_decoded_samples()?;
        Ok(self.decoded_samples)
    }

    fn receive_decoded_samples(&mut self) -> Result<()> {
        let mut decoded = frame::Audio::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let samples = frame_to_interleaved_stereo(&decoded)?;
            let pts = decoded.pts().map(|p| p.max(0) as u64).unwrap_or(0);
            self.decoded_samples.push(AudioSampleBatch {
                samples,
                pts: Duration::from_micros(pts),
            });
        }
        Ok(())
    }
}

/// Pull a decoded ffmpeg audio frame into interleaved stereo `f32`
/// samples at i16 amplitude scale (matching the OPUS decoder path).
/// Mono frames are duplicated into both channels so the rest of the
/// harness can keep assuming a stereo interleave.
fn frame_to_interleaved_stereo(frame: &frame::Audio) -> Result<Vec<f32>> {
    let channels = frame.channels() as usize;
    let sample_count = frame.samples();
    if channels == 0 {
        return Ok(Vec::new());
    }

    // Per (channel, sample) accessor scaled into i16 range.
    let value: Box<dyn Fn(usize, usize) -> f32> = match frame.format() {
        Sample::F32(Type::Planar) => {
            let planes: Vec<&[f32]> = (0..channels).map(|c| frame.plane::<f32>(c)).collect();
            Box::new(move |ch, s| planes[ch][s] * 32768.0)
        }
        Sample::F32(Type::Packed) => {
            let data = packed_f32(frame, channels * sample_count);
            Box::new(move |ch, s| data[s * channels + ch] * 32768.0)
        }
        Sample::I16(Type::Planar) => {
            let planes: Vec<&[i16]> = (0..channels).map(|c| frame.plane::<i16>(c)).collect();
            Box::new(move |ch, s| planes[ch][s] as f32)
        }
        Sample::I16(Type::Packed) => {
            let plane = frame.plane::<i16>(0).to_vec();
            Box::new(move |ch, s| plane[s * channels + ch] as f32)
        }
        other => bail!("aac_decoder: unsupported sample format {other:?}"),
    };

    let mut out = Vec::with_capacity(sample_count * 2);
    for s in 0..sample_count {
        let left = value(0, s);
        let right = if channels >= 2 { value(1, s) } else { left };
        out.push(left);
        out.push(right);
    }
    Ok(out)
}

/// Reinterpret a packed-float frame's raw byte buffer as `f32`. Packed
/// frames keep every channel in a single plane, which `plane::<f32>`
/// can't slice correctly, so read the bytes directly.
fn packed_f32(frame: &frame::Audio, len: usize) -> Vec<f32> {
    let bytes = frame.data(0);
    bytes
        .chunks_exact(4)
        .take(len)
        .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Fields needed to wrap a raw AAC access unit in an ADTS header.
struct AdtsConfig {
    profile: u8,
    freq_index: u8,
    channel_config: u8,
}

impl AdtsConfig {
    fn from_asc(asc: &Bytes) -> Result<Self> {
        let b0 = *asc.first().context("AAC ASC too short")?;
        let b1 = *asc.get(1).context("AAC ASC too short")?;
        let object_type = b0 >> 3;
        let freq_index = ((b0 & 0x07) << 1) | (b1 >> 7);
        let channel_config = (b1 >> 3) & 0x0F;
        if freq_index == 0x0F {
            bail!("aac_decoder: explicit sample rate in ASC is not supported");
        }
        if object_type == 0 {
            bail!("aac_decoder: invalid AAC object type");
        }
        Ok(Self {
            // ADTS profile is the audio object type minus one.
            profile: object_type - 1,
            freq_index,
            channel_config,
        })
    }

    /// Prepend a 7-byte ADTS header (no CRC) to a raw AAC access unit.
    fn frame(&self, payload: &[u8]) -> Vec<u8> {
        let frame_len = (payload.len() + 7) as u32;
        let mut out = Vec::with_capacity(payload.len() + 7);
        out.push(0xFF);
        out.push(0xF1); // MPEG-4, layer 0, no CRC
        out.push((self.profile << 6) | (self.freq_index << 2) | (self.channel_config >> 2));
        out.push(((self.channel_config & 0x3) << 6) | ((frame_len >> 11) & 0x3) as u8);
        out.push(((frame_len >> 3) & 0xFF) as u8);
        out.push((((frame_len & 0x7) << 5) | 0x1F) as u8);
        out.push(0xFC);
        out.extend_from_slice(payload);
        out
    }
}
