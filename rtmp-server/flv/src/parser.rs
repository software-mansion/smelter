#[derive(Debug, Clone)]
pub struct Header {
    pub audio_present: bool,
    pub video_present: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PacketType {
    Audio,
    AudioConfig,
    Video,
    VideoConfig,
    ScriptData,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCodec {
    Pcm,
    Adpcm,
    Mp3,
    PcmLe,
    Nellymoser16kMono,
    Nellymoser8kMono,
    Nellymoser,
    G711ALaw,
    G711MuLaw,
    AAC,
    Speex,
    Mp3_8k,
    DeviceSpecific,
}

impl AudioCodec {
    pub fn to_index(self) -> u8 {
        match self {
            AudioCodec::Pcm => 0,
            AudioCodec::Adpcm => 1,
            AudioCodec::Mp3 => 2,
            AudioCodec::PcmLe => 3,
            AudioCodec::Nellymoser16kMono => 4,
            AudioCodec::Nellymoser8kMono => 5,
            AudioCodec::Nellymoser => 6,
            AudioCodec::G711ALaw => 7,
            AudioCodec::G711MuLaw => 8,
            AudioCodec::AAC => 10,
            AudioCodec::Speex => 11,
            AudioCodec::Mp3_8k => 14,
            AudioCodec::DeviceSpecific => 15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoCodec {
    SorensonH263,
    ScreenVideo,
    Vp6,
    Vp6WithAlpha,
    ScreenVideo2,
    H264,
    // Enhanced RTMP codecs
    HEVC,
    AV1,
    VP9,
}

impl VideoCodec {
    pub fn to_index(self) -> Option<u8> {
        match self {
            VideoCodec::SorensonH263 => Some(2),
            VideoCodec::ScreenVideo => Some(3),
            VideoCodec::Vp6 => Some(4),
            VideoCodec::Vp6WithAlpha => Some(5),
            VideoCodec::ScreenVideo2 => Some(6),
            VideoCodec::H264 => Some(7),
            // Enhanced RTMP codecs don't have legacy indices
            VideoCodec::HEVC | VideoCodec::AV1 | VideoCodec::VP9 => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Codec {
    Audio(AudioCodec),
    Video(VideoCodec),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameType {
    Keyframe,
    Interframe,
}

#[derive(Debug, Clone, Default)]
pub struct CodecParams {
    pub composition_time: i32,
    pub key_frame: Option<bool>,
    pub sound_rate: Option<u32>,
    pub sound_type: Option<AudioChannels>,
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub pts: i64,
    pub dts: i64,
    pub stream_id: u32,
    pub packet_type: PacketType,
    pub payload: Vec<u8>,
    pub codec: Codec,
    pub codec_params: CodecParams,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    NotEnoughData,
    NotAHeader,
    UnsupportedCodec(u8),
}

// TODO behaviour like in membrane code but do not know if we would like to support all those codecs
pub fn parse_audio_payload(
    payload: &[u8],
) -> Result<(PacketType, Codec, CodecParams, &[u8]), ParseError> {
    if payload.len() < 2 {
        return Err(ParseError::NotEnoughData);
    }

    let sound_format = (payload[0] >> 4) & 0x0F;
    let sound_rate = (payload[0] >> 2) & 0x03;
    let sound_type = payload[0] & 0x01;

    let sound_rate_hz = match sound_rate {
        0 => 5_500,
        1 => 11_000,
        2 => 22_050,
        3 => 44_100,
        _ => 44_100, // TODO default or unrechable
    };

    let channels = match sound_type {
        0 => AudioChannels::Mono,
        1 => AudioChannels::Stereo,
        _ => AudioChannels::Stereo, // TODO default or unreachable
    };

    if sound_format == AudioCodec::AAC.to_index() {
        let packet_type = payload[1];
        let ptype = if packet_type == 1 {
            PacketType::Audio
        } else {
            PacketType::AudioConfig
        };

        return Ok((
            ptype,
            Codec::Audio(AudioCodec::AAC),
            CodecParams {
                sound_rate: Some(sound_rate_hz),
                sound_type: Some(channels),
                ..Default::default()
            },
            &payload[2..],
        ));
    }

    let audio_codec = index_to_audio_codec(sound_format)?;

    Ok((
        PacketType::Audio,
        Codec::Audio(audio_codec),
        CodecParams {
            sound_rate: Some(sound_rate_hz),
            sound_type: Some(channels),
            ..Default::default()
        },
        &payload[1..],
    ))
}

fn index_to_audio_codec(index: u8) -> Result<AudioCodec, ParseError> {
    match index {
        0 => Ok(AudioCodec::Pcm),
        1 => Ok(AudioCodec::Adpcm),
        2 => Ok(AudioCodec::Mp3),
        3 => Ok(AudioCodec::PcmLe),
        4 => Ok(AudioCodec::Nellymoser16kMono),
        5 => Ok(AudioCodec::Nellymoser8kMono),
        6 => Ok(AudioCodec::Nellymoser),
        7 => Ok(AudioCodec::G711ALaw),
        8 => Ok(AudioCodec::G711MuLaw),
        10 => Ok(AudioCodec::AAC),
        11 => Ok(AudioCodec::Speex),
        14 => Ok(AudioCodec::Mp3_8k),
        15 => Ok(AudioCodec::DeviceSpecific),
        _ => Err(ParseError::UnsupportedCodec(index)),
    }
}

fn index_to_video_codec(index: u8) -> Result<VideoCodec, ParseError> {
    match index {
        2 => Ok(VideoCodec::SorensonH263),
        3 => Ok(VideoCodec::ScreenVideo),
        4 => Ok(VideoCodec::Vp6),
        5 => Ok(VideoCodec::Vp6WithAlpha),
        6 => Ok(VideoCodec::ScreenVideo2),
        7 => Ok(VideoCodec::H264),
        _ => Err(ParseError::UnsupportedCodec(index)),
    }
}

const KEYFRAME_FRAME_TYPE: u8 = 1;

const FOURCC_AV1: &[u8; 4] = b"av01";
const FOURCC_VP9: &[u8; 4] = b"vp09";
const FOURCC_HEVC: &[u8; 4] = b"hvc1";

const PACKET_TYPE_SEQUENCE_START: u8 = 0;
const PACKET_TYPE_CODED_FRAMES: u8 = 1;
const PACKET_TYPE_METADATA: u8 = 4;
const PACKET_TYPE_MPEG2TS_SEQUENCE_START: u8 = 5;

pub fn parse_video_payload(
    payload: &[u8],
) -> Result<(PacketType, Codec, CodecParams, &[u8]), ParseError> {
    if payload.is_empty() {
        return Err(ParseError::NotEnoughData);
    }

    let first_byte = payload[0];
    let is_ex_header = (first_byte >> 7) & 0x01 == 1;

    if is_ex_header {
        parse_enhanced_video_payload(payload)
    } else {
        parse_legacy_video_payload(payload)
    }
}

fn parse_enhanced_video_payload(
    payload: &[u8],
) -> Result<(PacketType, Codec, CodecParams, &[u8]), ParseError> {
    if payload.len() < 5 {
        return Err(ParseError::NotEnoughData);
    }

    let frame_type = (payload[0] >> 4) & 0x07;
    let packet_type = payload[0] & 0x0F;
    let fourcc = &payload[1..5];

    let codec = match fourcc {
        b if b == FOURCC_AV1 => VideoCodec::AV1,
        b if b == FOURCC_VP9 => VideoCodec::VP9,
        b if b == FOURCC_HEVC => VideoCodec::HEVC,
        _ => return Err(ParseError::UnsupportedCodec(0)),
    };

    let is_key_frame = frame_type == KEYFRAME_FRAME_TYPE;

    let ptype = match (codec, packet_type) {
        (_, PACKET_TYPE_METADATA) => PacketType::VideoConfig,
        (VideoCodec::AV1, PACKET_TYPE_SEQUENCE_START | PACKET_TYPE_MPEG2TS_SEQUENCE_START) => {
            PacketType::VideoConfig
        }
        (VideoCodec::HEVC | VideoCodec::VP9, PACKET_TYPE_SEQUENCE_START) => PacketType::VideoConfig,
        _ => PacketType::Video,
    };

    let (composition_time, data_start) =
        if codec == VideoCodec::HEVC && packet_type == PACKET_TYPE_CODED_FRAMES {
            if payload.len() < 8 {
                return Err(ParseError::NotEnoughData);
            }
            let ct = i32::from_be_bytes([0, payload[5], payload[6], payload[7]]);
            (ct, 8)
        } else {
            (0, 5)
        };

    Ok((
        ptype,
        Codec::Video(codec),
        CodecParams {
            composition_time,
            key_frame: Some(is_key_frame),
            ..Default::default()
        },
        &payload[data_start..],
    ))
}

fn parse_legacy_video_payload(
    payload: &[u8],
) -> Result<(PacketType, Codec, CodecParams, &[u8]), ParseError> {
    if payload.len() < 5 {
        return Err(ParseError::NotEnoughData);
    }

    let frame_type = (payload[0] >> 4) & 0x0F;
    let codec_id = payload[0] & 0x0F;

    if Some(codec_id) == VideoCodec::H264.to_index() {
        let packet_type = payload[1];
        let composition_time = i32::from_be_bytes([0, payload[2], payload[3], payload[4]]);

        let ptype = if packet_type == 0 {
            PacketType::VideoConfig
        } else {
            PacketType::Video
        };

        return Ok((
            ptype,
            Codec::Video(VideoCodec::H264),
            CodecParams {
                composition_time,
                key_frame: Some(frame_type == KEYFRAME_FRAME_TYPE),
                ..Default::default()
            },
            &payload[5..],
        ));
    }

    let video_codec = index_to_video_codec(codec_id)?;

    Ok((
        PacketType::Video,
        Codec::Video(video_codec),
        CodecParams {
            key_frame: Some(frame_type == KEYFRAME_FRAME_TYPE),
            ..Default::default()
        },
        &payload[1..],
    ))
}
