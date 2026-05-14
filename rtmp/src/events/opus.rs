use bytes::Bytes;

use crate::{AudioChannels, OpusConfigParseError};

#[derive(Clone)]
pub struct OpusAudioConfig {
    data: Bytes,
    channels: AudioChannels,
}

impl TryFrom<Bytes> for OpusAudioConfig {
    type Error = OpusConfigParseError;

    fn try_from(data: Bytes) -> Result<Self, Self::Error> {
        // Byte 9 = output channel count (RFC 7845 §5.1).
        // We only require 10 bytes since some E-RTMP senders omit the full
        // RFC 7845 Opus ID header and send a minimal payload.
        if data.len() < 10 {
            return Err(OpusConfigParseError::TooShort);
        }

        let channel_count = data[9];
        if channel_count == 0 {
            return Err(OpusConfigParseError::InvalidChannelCount(channel_count));
        }

        let channels = match channel_count {
            1 => AudioChannels::Mono,
            _ => AudioChannels::Stereo,
        };

        Ok(Self { data, channels })
    }
}

impl OpusAudioConfig {
    pub fn data(&self) -> &Bytes {
        &self.data
    }

    pub fn channels(&self) -> AudioChannels {
        self.channels
    }
}

impl std::fmt::Debug for OpusAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpusAudioConfig")
            .field("channels", &self.channels)
            .field("data", &crate::events::bytes_debug(&self.data))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use crate::{AudioChannels, OpusAudioConfig};

    #[test]
    fn parses_mono_opus_id_header() {
        // Minimal Opus ID header: 19 bytes
        // "OpusHead" + version(1) + channels(1) + pre_skip(0) + sample_rate(48000) + gain(0) + mapping(0)
        let data = Bytes::from_static(&[
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // magic
            1,    // version
            1,    // channel count = mono
            0, 0, // pre-skip
            0x80, 0xBB, 0x00, 0x00, // sample rate 48000 LE
            0, 0, // output gain
            0, // mapping family
        ]);
        let config = OpusAudioConfig::try_from(data).unwrap();
        assert_eq!(config.channels(), AudioChannels::Mono);
    }

    #[test]
    fn parses_stereo_opus_id_header() {
        let data = Bytes::from_static(&[
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 2, 0, 0, 0x80, 0xBB, 0x00, 0x00, 0,
            0, 0,
        ]);
        let config = OpusAudioConfig::try_from(data).unwrap();
        assert_eq!(config.channels(), AudioChannels::Stereo);
    }

    #[test]
    fn rejects_too_short() {
        let data = Bytes::from_static(&[b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1]);
        assert!(OpusAudioConfig::try_from(data).is_err());
    }

    #[test]
    fn rejects_zero_channels() {
        let data = Bytes::from_static(&[
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert!(OpusAudioConfig::try_from(data).is_err());
    }

    #[test]
    fn multichannel_maps_to_stereo() {
        let data = Bytes::from_static(&[
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 6, 0, 0, 0x80, 0xBB, 0x00, 0x00, 0,
            0, 0,
        ]);
        let config = OpusAudioConfig::try_from(data).unwrap();
        assert_eq!(config.channels(), AudioChannels::Stereo);
    }
}
