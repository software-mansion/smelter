use std::time::Duration;

use crate::{
    AacAudioConfig, AudioChannels, AudioConfig, AudioData, AudioTag, AudioTagAacPacketType,
    AudioTagSampleSize, AudioTagSoundRate, FlvAudioTagParseError, LegacyFlvAudioCodec,
    RtmpAudioCodec, RtmpMessageParseError, RtmpMessageSerializeError, TrackId,
    message::AUDIO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum AudioMessage {
    Data(AudioData),
    Config(AudioConfig),
    /// Wire-level audio packet types that carry no user-visible payload
    /// (Enhanced RTMP ExHeader audio — parsing not yet implemented).
    Unknown,
}

impl AudioMessage {
    pub(crate) fn is_media_packet(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let tag = match AudioTag::parse(msg.payload) {
            Ok(tag) => tag,
            Err(FlvAudioTagParseError::UnknownCodecId(_)) => {
                return Ok(Self::Unknown);
            }
            Err(err) => return Err(err.into()),
        };
        let pts = Duration::from_millis(msg.timestamp.into());
        let sample_rate = sound_rate_to_hz(tag.sample_rate);
        let sample_size = sample_size_to_bits(tag.sample_size);
        let codec = audio_codec_from_legacy(tag.codec, sample_rate, sample_size);

        let msg = match (tag.codec, tag.aac_packet_type) {
            (LegacyFlvAudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
                Self::Config(AudioConfig {
                    track_id: TrackId::PRIMARY,
                    codec,
                    data: tag.data,
                })
            }
            _ => Self::Data(AudioData {
                track_id: TrackId::PRIMARY,
                codec,
                pts,
                channels: tag.channels,
                data: tag.data,
            }),
        };
        Ok(msg)
    }

    pub(super) fn into_raw(self, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
        match self {
            Self::Data(audio) => {
                let legacy_codec = legacy_from_audio_codec(audio.codec)?;
                let (rate_hz, size_bits) = codec_rate_and_size(audio.codec);
                let sample_rate = rate_hz
                    .map(hz_to_sound_rate)
                    .unwrap_or(AudioTagSoundRate::Rate44000);
                let sample_size = size_bits
                    .map(bits_to_sample_size)
                    .unwrap_or(AudioTagSampleSize::Sample16Bit);
                let aac_packet_type = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => Some(AudioTagAacPacketType::Data),
                    _ => None,
                };
                Ok(RawMessage {
                    msg_type: MessageType::Audio.into_raw(),
                    stream_id,
                    chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
                    timestamp: audio.pts.as_millis() as u32,
                    payload: AudioTag {
                        aac_packet_type,
                        codec: legacy_codec,
                        sample_rate,
                        sample_size,
                        channels: audio.channels,
                        data: audio.data,
                    }
                    .serialize()?,
                })
            }
            Self::Unknown => Err(RtmpMessageSerializeError::InternalError(
                "Cannot serialize an unknown audio message".into(),
            )),
            Self::Config(config) => {
                let legacy_codec = legacy_from_audio_codec(config.codec)?;
                let (aac_packet_type, channels) = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => {
                        let parsed =
                            AacAudioConfig::try_from(config.data.clone()).map_err(|err| {
                                RtmpMessageSerializeError::InternalError(format!(
                                    "Failed to parse AAC config: {err}"
                                ))
                            })?;
                        (Some(AudioTagAacPacketType::Config), parsed.channels())
                    }
                    _ => (None, AudioChannels::Stereo),
                };
                Ok(RawMessage {
                    msg_type: MessageType::Audio.into_raw(),
                    stream_id,
                    chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
                    timestamp: 0,
                    payload: AudioTag {
                        aac_packet_type,
                        codec: legacy_codec,
                        sample_rate: AudioTagSoundRate::Rate44000,
                        sample_size: AudioTagSampleSize::Sample16Bit,
                        channels,
                        data: config.data,
                    }
                    .serialize()?,
                })
            }
        }
    }
}

fn audio_codec_from_legacy(
    codec: LegacyFlvAudioCodec,
    sample_rate: u32,
    sample_size: u8,
) -> RtmpAudioCodec {
    match codec {
        LegacyFlvAudioCodec::Aac => RtmpAudioCodec::Aac,
        LegacyFlvAudioCodec::Mp3 => RtmpAudioCodec::Mp3 {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::Mp3_8k => RtmpAudioCodec::Mp3_8k { sample_size },
        LegacyFlvAudioCodec::Pcm => RtmpAudioCodec::Pcm {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::Adpcm => RtmpAudioCodec::Adpcm {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::PcmLe => RtmpAudioCodec::PcmLe {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::Nellymoser => RtmpAudioCodec::Nellymoser {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::Nellymoser8kMono => RtmpAudioCodec::Nellymoser8kMono { sample_size },
        LegacyFlvAudioCodec::Nellymoser16kMono => RtmpAudioCodec::Nellymoser16kMono { sample_size },
        LegacyFlvAudioCodec::G711ALaw => RtmpAudioCodec::G711ALaw {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::G711MuLaw => RtmpAudioCodec::G711MuLaw {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::Speex => RtmpAudioCodec::Speex {
            sample_rate,
            sample_size,
        },
        LegacyFlvAudioCodec::DeviceSpecific => RtmpAudioCodec::DeviceSpecific {
            sample_rate,
            sample_size,
        },
    }
}

fn legacy_from_audio_codec(
    codec: RtmpAudioCodec,
) -> Result<LegacyFlvAudioCodec, RtmpMessageSerializeError> {
    Ok(match codec {
        RtmpAudioCodec::Aac => LegacyFlvAudioCodec::Aac,
        RtmpAudioCodec::Mp3 { .. } => LegacyFlvAudioCodec::Mp3,
        RtmpAudioCodec::Mp3_8k { .. } => LegacyFlvAudioCodec::Mp3_8k,
        RtmpAudioCodec::Pcm { .. } => LegacyFlvAudioCodec::Pcm,
        RtmpAudioCodec::Adpcm { .. } => LegacyFlvAudioCodec::Adpcm,
        RtmpAudioCodec::PcmLe { .. } => LegacyFlvAudioCodec::PcmLe,
        RtmpAudioCodec::Nellymoser { .. } => LegacyFlvAudioCodec::Nellymoser,
        RtmpAudioCodec::Nellymoser8kMono { .. } => LegacyFlvAudioCodec::Nellymoser8kMono,
        RtmpAudioCodec::Nellymoser16kMono { .. } => LegacyFlvAudioCodec::Nellymoser16kMono,
        RtmpAudioCodec::G711ALaw { .. } => LegacyFlvAudioCodec::G711ALaw,
        RtmpAudioCodec::G711MuLaw { .. } => LegacyFlvAudioCodec::G711MuLaw,
        RtmpAudioCodec::Speex { .. } => LegacyFlvAudioCodec::Speex,
        RtmpAudioCodec::DeviceSpecific { .. } => LegacyFlvAudioCodec::DeviceSpecific,
    })
}

fn codec_rate_and_size(codec: RtmpAudioCodec) -> (Option<u32>, Option<u8>) {
    match codec {
        RtmpAudioCodec::Aac => (None, None),
        RtmpAudioCodec::Mp3 {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::Pcm {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::Adpcm {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::PcmLe {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::Nellymoser {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::G711ALaw {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::G711MuLaw {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::Speex {
            sample_rate,
            sample_size,
        }
        | RtmpAudioCodec::DeviceSpecific {
            sample_rate,
            sample_size,
        } => (Some(sample_rate), Some(sample_size)),
        RtmpAudioCodec::Mp3_8k { sample_size } => (Some(8000), Some(sample_size)),
        RtmpAudioCodec::Nellymoser8kMono { sample_size } => (Some(8000), Some(sample_size)),
        RtmpAudioCodec::Nellymoser16kMono { sample_size } => (Some(16000), Some(sample_size)),
    }
}

fn sound_rate_to_hz(rate: AudioTagSoundRate) -> u32 {
    match rate {
        AudioTagSoundRate::Rate5500 => 5500,
        AudioTagSoundRate::Rate11000 => 11025,
        AudioTagSoundRate::Rate22000 => 22050,
        AudioTagSoundRate::Rate44000 => 44100,
    }
}

fn hz_to_sound_rate(hz: u32) -> AudioTagSoundRate {
    match hz {
        0..=5500 => AudioTagSoundRate::Rate5500,
        5501..=11025 => AudioTagSoundRate::Rate11000,
        11026..=22050 => AudioTagSoundRate::Rate22000,
        _ => AudioTagSoundRate::Rate44000,
    }
}

fn sample_size_to_bits(size: AudioTagSampleSize) -> u8 {
    match size {
        AudioTagSampleSize::Sample8Bit => 8,
        AudioTagSampleSize::Sample16Bit => 16,
    }
}

fn bits_to_sample_size(bits: u8) -> AudioTagSampleSize {
    if bits <= 8 {
        AudioTagSampleSize::Sample8Bit
    } else {
        AudioTagSampleSize::Sample16Bit
    }
}
