use std::collections::HashMap;

use crate::{
    RtmpClientConfig, RtmpConnectionError, RtmpMessageParseError,
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

use crate::{
    CAPS_EX_MODEX, CAPS_EX_RECONNECT, CAPS_EX_TIMESTAMP_NANO, FOURCC_INFO_CAN_ENCODE,
    FOURCC_INFO_CAN_FORWARD,
};

use crate::{AUDIO_FOURCC_LIST, VIDEO_FOURCC_LIST};

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

pub(super) enum PublishStatus {
    Started { stream_id: u32 },
    Rejected(String),
}

pub(super) struct NegotiatedCapabilities {
    pub supports_enhanced: bool,
    pub supports_modex: bool,
}

impl NegotiationProgress {
    pub(super) fn try_match_connect_response(
        &self,
        msg: &RtmpMessageIncoming,
    ) -> Result<Option<(CommandMessageConnectSuccess, NegotiatedCapabilities)>, RtmpConnectionError>
    {
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
                // Fallback to checking 'information' because some non-compliant RTMP
                // servers mistakenly place Enhanced RTMP capabilities there instead of 'properties'.
                let supports_modex = map_supports_modex(&connect_success.properties)
                    || map_supports_modex(&connect_success.information);
                let supports_enhanced = map_supports_enhanced(&connect_success.properties)
                    || map_supports_enhanced(&connect_success.information);
                Ok(Some((
                    connect_success,
                    NegotiatedCapabilities {
                        supports_enhanced,
                        supports_modex,
                    },
                )))
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

    pub(super) fn try_match_on_status(&self, msg: &RtmpMessageIncoming) -> Option<PublishStatus> {
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

        Some(match status {
            AmfValue::Object(status) | AmfValue::EcmaArray(status) => {
                match status.get("code") {
                    Some(AmfValue::String(code)) if code == "NetStream.Publish.Start" => {
                        PublishStatus::Started {
                            stream_id: *stream_id,
                        }
                    }
                    Some(AmfValue::String(code)) => {
                        let description = status
                            .get("description")
                            .and_then(|value| match value {
                                AmfValue::String(description) => Some(description.as_str()),
                                _ => None,
                            })
                            .unwrap_or("RTMP server returned non-success onStatus");
                        PublishStatus::Rejected(format!("{code}: {description}"))
                    }
                    _ => PublishStatus::Rejected(format!(
                        "Unexpected onStatus payload: {status:?}"
                    )),
                }
            }
            _ => PublishStatus::Rejected(format!("Unexpected onStatus payload: {status:?}")),
        })
    }
}

pub(super) fn send_connect(
    stream: &mut RtmpMessageStream,
    config: &RtmpClientConfig,
) -> Result<(), RtmpConnectionError> {
    let encode_forward_caps =
        AmfValue::Number((FOURCC_INFO_CAN_ENCODE | FOURCC_INFO_CAN_FORWARD) as f64);
    let video_fourcc_info_map = HashMap::from_iter([
        (
            "*".to_string(),
            AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
        ),
        ("avc1".to_string(), encode_forward_caps.clone()),
        ("vp09".to_string(), encode_forward_caps.clone()),
        ("vp08".to_string(), encode_forward_caps.clone()),
    ]);
    let audio_fourcc_info_map = HashMap::from_iter([
        (
            "*".to_string(),
            AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
        ),
        ("mp4a".to_string(), encode_forward_caps.clone()),
        ("Opus".to_string(), encode_forward_caps),
    ]);
    let fourcc_list: Vec<AmfValue> = VIDEO_FOURCC_LIST
        .iter()
        .chain(AUDIO_FOURCC_LIST.iter())
        .map(|v| AmfValue::String((*v).to_string()))
        .collect();
    let props = HashMap::from_iter(
        [
            ("app", config.app.clone().into()),
            ("tcUrl", config.tc_url().into()),
            ("flashVer", "FMS/3,0,1,123".into()),
            ("fpad", AmfValue::Boolean(false)),
            // legacy RTMP codecs
            ("audioCodecs", AmfValue::Number(0x0400 as f64)), // AAC
            ("videoCodecs", AmfValue::Number(0x0080 as f64)), // H.264
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

fn map_supports_enhanced(map: &HashMap<String, AmfValue>) -> bool {
    let has_caps_ex = map_supports_modex(map);
    let has_fourcc_list = map
        .get("fourCcList")
        .is_some_and(fourcc_list_supports_video);
    let has_video_info_map = map
        .get("videoFourCcInfoMap")
        .is_some_and(video_fourcc_info_map_supports_video);

    has_caps_ex || has_fourcc_list || has_video_info_map
}

fn map_supports_modex(map: &HashMap<String, AmfValue>) -> bool {
    map.get("capsEx")
        .is_some_and(|v| matches!(v, AmfValue::Number(n) if (*n as u8) & CAPS_EX_MODEX != 0))
}

fn fourcc_list_supports_video(value: &AmfValue) -> bool {
    let AmfValue::StrictArray(items) = value else {
        return false;
    };

    items.iter().any(|item| match item {
        AmfValue::String(v) => v == "*" || VIDEO_FOURCC_LIST.contains(&v.as_str()),
        _ => false,
    })
}

fn video_fourcc_info_map_supports_video(value: &AmfValue) -> bool {
    let map = match value {
        AmfValue::Object(map) => map,
        AmfValue::EcmaArray(map) => map,
        _ => return false,
    };

    map.iter().any(|(k, v)| match v {
        AmfValue::Number(mask) if *mask > 0.0 => {
            k == "*" || VIDEO_FOURCC_LIST.contains(&k.as_str())
        }
        AmfValue::Number(_) => false,
        _ => false,
    })
}
