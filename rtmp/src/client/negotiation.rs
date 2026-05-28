use std::collections::HashMap;

use crate::{
    CAPS_EX_MODEX, CAPS_EX_RECONNECT, CAPS_EX_TIMESTAMP_NANO, ExCapabilities,
    FOURCC_INFO_CAN_ENCODE, FOURCC_INFO_CAN_FORWARD, RtmpAudioCodec, RtmpClientConfig,
    RtmpConnectionError, RtmpMessageParseError, RtmpVideoCodec,
    amf0::AmfValue,
    error::RtmpStreamError,
    message::{
        CONTROL_MESSAGE_STREAM_ID, CommandMessage, CommandMessageConnectSuccess,
        CommandMessageCreateStreamSuccess, CommandMessageResultExt, RtmpMessageIncoming,
        RtmpMessageOutgoing,
    },
    protocol::message_stream::RtmpMessageStream,
};

const CONNECT_TRANSACTION_ID: u32 = 1;
const CREATE_STREAM_TRANSACTION_ID: u32 = 2;

/// Legacy FLV `audioCodecs` bit for AAC. See FLV spec, `SoundFormat`.
const LEGACY_AUDIO_CODEC_AAC: u32 = 0x0400;
/// Legacy FLV `videoCodecs` bit for H.264.
const LEGACY_VIDEO_CODEC_H264: u32 = 0x0080;

/// -> - from client to server
/// <- - from server to client
///
/// indented steps are not reliable, assume that they can happen at different point or
/// not at all
pub(super) enum NegotiationProgress {
    /// -> connect
    ///     <- Window Ack size
    ///     <- Set Peer Bandwidth
    ///     -> Window Ack Size
    ///     <- StreamBegin (with stream id 0)
    /// <- connect _result
    WaitingForConnectResult,

    /// -> createStream
    /// <- createStream _result
    WaitingForCreateStreamResult,

    /// -> publish
    ///     <- StreamBegin (with real stream id)
    ///     -> DataMessage (metadata)       TODO
    ///     -> SetChunkSize                 TODO
    /// <- onStatus
    WaitingForOnStatus { stream_id: u32 },
}

impl NegotiationProgress {
    pub(super) fn try_match_connect_response(
        &self,
        msg: &RtmpMessageIncoming,
    ) -> Result<Option<(CommandMessageConnectSuccess, ExCapabilities)>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForConnectResult = self else {
            return Ok(None);
        };

        let RtmpMessageIncoming::CommandMessage { msg, .. } = msg else {
            return Ok(None);
        };
        let CommandMessage::Result(result) = msg else {
            return Ok(None);
        };

        if result.transaction_id() != CONNECT_TRANSACTION_ID {
            return Ok(None);
        }

        match result {
            Ok(result) => {
                let connect_success = result
                    .to_connect_success()
                    .map_err(RtmpMessageParseError::CommandMessage)
                    .map_err(RtmpStreamError::ParseMessage)?;
                let caps_ex_bits = parse_caps_ex_bits(&connect_success.properties)
                    | parse_caps_ex_bits(&connect_success.information);
                let ex_capabilities = ExCapabilities::from_caps_ex_bits(caps_ex_bits);

                Ok(Some((connect_success, ex_capabilities)))
            }
            Err(err) => Err(RtmpConnectionError::ErrorOnConnect(format!("{err:?}"))),
        }
    }

    pub(super) fn try_match_create_stream_response(
        &self,
        msg: &RtmpMessageIncoming,
    ) -> Result<Option<CommandMessageCreateStreamSuccess>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForCreateStreamResult = self else {
            return Ok(None);
        };

        let RtmpMessageIncoming::CommandMessage { msg, .. } = msg else {
            return Ok(None);
        };
        let CommandMessage::Result(result) = msg else {
            return Ok(None);
        };

        if result.transaction_id() != CREATE_STREAM_TRANSACTION_ID {
            return Ok(None);
        }

        match result {
            Ok(result) => {
                let create_stream_success = result
                    .to_create_stream_success()
                    .map_err(RtmpMessageParseError::CommandMessage)
                    .map_err(RtmpStreamError::ParseMessage)?;
                Ok(Some(create_stream_success))
            }
            Err(err) => Err(RtmpConnectionError::ErrorOnCreateStream(format!("{err:?}"))),
        }
    }

    pub(super) fn try_match_on_status(&self, msg: &RtmpMessageIncoming) -> Option<(AmfValue, u32)> {
        let NegotiationProgress::WaitingForOnStatus { stream_id } = self else {
            return None;
        };

        let RtmpMessageIncoming::CommandMessage {
            msg: CommandMessage::OnStatus(status),
            stream_id: on_status_stream_id,
        } = msg
        else {
            return None;
        };

        if on_status_stream_id != stream_id {
            return None;
        }
        Some((status.clone(), *stream_id))
    }
}

pub(super) fn send_connect(
    stream: &mut RtmpMessageStream,
    config: &RtmpClientConfig,
) -> Result<(), RtmpConnectionError> {
    let encode_forward_caps =
        AmfValue::Number((FOURCC_INFO_CAN_ENCODE | FOURCC_INFO_CAN_FORWARD) as f64);

    let mut video_fourcc_info_map = HashMap::new();
    video_fourcc_info_map.insert(
        "*".to_string(),
        AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
    );
    for codec in &config.video_codecs {
        video_fourcc_info_map.insert(codec.fourcc().to_string(), encode_forward_caps.clone());
    }

    let mut audio_fourcc_info_map = HashMap::new();
    audio_fourcc_info_map.insert(
        "*".to_string(),
        AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
    );
    for codec in &config.audio_codecs {
        audio_fourcc_info_map.insert(codec.fourcc().to_string(), encode_forward_caps.clone());
    }

    let video_fourcc_list = config
        .video_codecs
        .iter()
        .map(|c| AmfValue::String(c.fourcc().to_string()));
    let audio_fourcc_list = config
        .audio_codecs
        .iter()
        .map(|c| AmfValue::String(c.fourcc().to_string()));
    let fourcc_list: Vec<AmfValue> = video_fourcc_list.chain(audio_fourcc_list).collect();

    let legacy_audio_codecs = if config.audio_codecs.contains(&RtmpAudioCodec::Aac) {
        LEGACY_AUDIO_CODEC_AAC
    } else {
        0
    };
    let legacy_video_codecs = if config.video_codecs.contains(&RtmpVideoCodec::H264) {
        LEGACY_VIDEO_CODEC_H264
    } else {
        0
    };

    let props = HashMap::from_iter(
        [
            ("app", config.app.clone().into()),
            ("tcUrl", config.tc_url().into()),
            ("flashVer", "FMS/3,0,1,123".into()),
            ("fpad", AmfValue::Boolean(false)),
            // legacy RTMP codecs
            ("audioCodecs", AmfValue::Number(legacy_audio_codecs as f64)),
            ("videoCodecs", AmfValue::Number(legacy_video_codecs as f64)),
            ("videoFunction", AmfValue::Number(0.0)),
            // E-RTMP codecs
            ("fourCcList", AmfValue::StrictArray(fourcc_list)),
            (
                "videoFourCcInfoMap",
                AmfValue::Object(video_fourcc_info_map),
            ),
            (
                "audioFourCcInfoMap",
                AmfValue::Object(audio_fourcc_info_map),
            ),
            (
                "capsEx",
                AmfValue::Number(
                    (CAPS_EX_RECONNECT | CAPS_EX_MODEX | CAPS_EX_TIMESTAMP_NANO) as f64,
                ),
            ),
            ("objectEncoding", AmfValue::Number(0.0)),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v)),
    );

    stream.write_msg(RtmpMessageOutgoing::CommandMessage {
        msg: CommandMessage::Connect {
            transaction_id: CONNECT_TRANSACTION_ID,
            command_object: props,
            optional_args: None,
        },
        stream_id: CONTROL_MESSAGE_STREAM_ID,
    })?;
    Ok(())
}

pub(super) fn send_create_stream(
    stream: &mut RtmpMessageStream,
) -> Result<(), RtmpConnectionError> {
    stream.write_msg(RtmpMessageOutgoing::CommandMessage {
        msg: CommandMessage::CreateStream {
            transaction_id: CREATE_STREAM_TRANSACTION_ID,
            command_object: AmfValue::Null,
        },
        stream_id: CONTROL_MESSAGE_STREAM_ID,
    })?;
    Ok(())
}

pub(super) fn send_publish(
    stream: &mut RtmpMessageStream,
    stream_key: &str,
    stream_id: u32,
) -> Result<(), RtmpConnectionError> {
    stream.write_msg(RtmpMessageOutgoing::CommandMessage {
        msg: CommandMessage::Publish {
            stream_key: stream_key.to_string(),
            publishing_type: "live".to_string(),
        },
        stream_id,
    })?;
    Ok(())
}

fn parse_caps_ex_bits(map: &HashMap<String, AmfValue>) -> u8 {
    match map.get("capsEx") {
        Some(AmfValue::Number(bits)) if bits.is_finite() => {
            bits.floor().clamp(0.0, u8::MAX as f64) as u8
        }
        _ => 0,
    }
}

#[cfg(test)]
fn map_advertises_enhanced_codecs(map: &HashMap<String, AmfValue>) -> bool {
    map.get("fourCcList")
        .is_some_and(fourcc_list_supports_enhanced)
        || map
            .get("videoFourCcInfoMap")
            .is_some_and(video_fourcc_info_map_supports_video)
        || map
            .get("audioFourCcInfoMap")
            .is_some_and(audio_fourcc_info_map_supports_audio)
}

#[cfg(test)]
fn fourcc_list_supports_enhanced(value: &AmfValue) -> bool {
    let AmfValue::StrictArray(items) = value else {
        return false;
    };

    let known_video: Vec<&'static str> =
        [RtmpVideoCodec::H264, RtmpVideoCodec::Vp8, RtmpVideoCodec::Vp9]
            .into_iter()
            .map(|c| c.fourcc())
            .collect();
    let known_audio: Vec<&'static str> = [RtmpAudioCodec::Aac, RtmpAudioCodec::Opus]
        .into_iter()
        .map(|c| c.fourcc())
        .collect();

    items.iter().any(|item| match item {
        AmfValue::String(v) => {
            v == "*" || known_video.contains(&v.as_str()) || known_audio.contains(&v.as_str())
        }
        _ => false,
    })
}

#[cfg(test)]
fn video_fourcc_info_map_supports_video(value: &AmfValue) -> bool {
    let map = match value {
        AmfValue::Object(map) => map,
        AmfValue::EcmaArray(map) => map,
        _ => return false,
    };
    let known: Vec<&'static str> =
        [RtmpVideoCodec::H264, RtmpVideoCodec::Vp8, RtmpVideoCodec::Vp9]
            .into_iter()
            .map(|c| c.fourcc())
            .collect();

    map.iter().any(|(k, v)| match v {
        AmfValue::Number(mask) if *mask > 0.0 => k == "*" || known.contains(&k.as_str()),
        AmfValue::Number(_) => false,
        _ => false,
    })
}

#[cfg(test)]
fn audio_fourcc_info_map_supports_audio(value: &AmfValue) -> bool {
    let map = match value {
        AmfValue::Object(map) => map,
        AmfValue::EcmaArray(map) => map,
        _ => return false,
    };
    let known: Vec<&'static str> = [RtmpAudioCodec::Aac, RtmpAudioCodec::Opus]
        .into_iter()
        .map(|c| c.fourcc())
        .collect();

    map.iter().any(|(k, v)| match v {
        AmfValue::Number(mask) if *mask > 0.0 => k == "*" || known.contains(&k.as_str()),
        AmfValue::Number(_) => false,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_ex_without_fourcc_metadata_counts_as_enhanced() {
        let capabilities = HashMap::from([(
            "capsEx".to_string(),
            AmfValue::Number(CAPS_EX_RECONNECT as f64),
        )]);

        let caps_ex_bits = parse_caps_ex_bits(&capabilities);
        let advertises_enhanced_codecs =
            map_advertises_enhanced_codecs(&capabilities) || caps_ex_bits != 0;

        assert!(advertises_enhanced_codecs);
        assert_eq!(caps_ex_bits, CAPS_EX_RECONNECT);
    }

    #[test]
    fn audio_only_fourcc_list_counts_as_enhanced() {
        let capabilities = HashMap::from([(
            "fourCcList".to_string(),
            AmfValue::StrictArray(vec![AmfValue::String("Opus".to_string())]),
        )]);

        assert!(map_advertises_enhanced_codecs(&capabilities));
        assert_eq!(parse_caps_ex_bits(&capabilities), 0);
    }

    #[test]
    fn audio_only_fourcc_info_map_counts_as_enhanced() {
        let capabilities = HashMap::from([(
            "audioFourCcInfoMap".to_string(),
            AmfValue::Object(HashMap::from([(
                "Opus".to_string(),
                AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
            )])),
        )]);

        assert!(map_advertises_enhanced_codecs(&capabilities));
    }

    #[test]
    fn caps_ex_tracks_mod_ex_bits_separately_from_enhanced_codecs() {
        let capabilities = HashMap::from([(
            "capsEx".to_string(),
            AmfValue::Number((CAPS_EX_MODEX | CAPS_EX_TIMESTAMP_NANO) as f64),
        )]);

        let caps_ex_bits = parse_caps_ex_bits(&capabilities);
        let ex_capabilities = ExCapabilities::from_caps_ex_bits(caps_ex_bits);

        assert!(ex_capabilities.supports_timestamp_nano_mod_ex());
    }

    #[test]
    fn merges_caps_ex_from_properties_and_information() {
        let properties =
            HashMap::from([("capsEx".to_string(), AmfValue::Number(CAPS_EX_MODEX as f64))]);
        let information = HashMap::from([(
            "capsEx".to_string(),
            AmfValue::Number(CAPS_EX_TIMESTAMP_NANO as f64),
        )]);

        let caps_ex_bits = parse_caps_ex_bits(&properties) | parse_caps_ex_bits(&information);
        let ex_capabilities = ExCapabilities::from_caps_ex_bits(caps_ex_bits);

        assert!(ex_capabilities.supports_timestamp_nano_mod_ex());
    }
}
