use std::collections::HashMap;

use crate::{
    RtmpClientConfig, RtmpConnectionError, RtmpMessageParseError,
    amf0::AmfValue,
    error::RtmpStreamError,
    message::{
        CONTROL_MESSAGE_STREAM_ID, CommandMessage, CommandMessageConnectSuccess,
        CommandMessageCreateStreamSuccess, CommandMessageResultExt, RtmpMessage,
    },
    protocol::message_stream::RtmpMessageStream,
};

const CONNECT_TRANSACTION_ID: u32 = 1;
const CREATE_STREAM_TRANSACTION_ID: u32 = 2;

use crate::{
    CAPS_EX_MODEX, CAPS_EX_RECONNECT, CAPS_EX_TIMESTAMP_NANO, FOURCC_INFO_CAN_DECODE,
    FOURCC_INFO_CAN_ENCODE, FOURCC_INFO_CAN_FORWARD,
};

use crate::VIDEO_FOURCC_LIST;

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
        msg: &RtmpMessage,
    ) -> Result<Option<(CommandMessageConnectSuccess, bool)>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForConnectResult = self else {
            return Ok(None);
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
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
                let supports_enhanced = map_supports_enhanced(&connect_success.properties)
                    || map_supports_enhanced(&connect_success.information);
                Ok(Some((connect_success, supports_enhanced)))
            }
            Err(err) => Err(RtmpConnectionError::ErrorOnConnect(format!("{err:?}"))),
        }
    }

    pub(super) fn try_match_create_stream_response(
        &self,
        msg: &RtmpMessage,
    ) -> Result<Option<CommandMessageCreateStreamSuccess>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForCreateStreamResult = self else {
            return Ok(None);
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
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

    pub(super) fn try_match_on_status(&self, msg: &RtmpMessage) -> Option<(AmfValue, u32)> {
        let NegotiationProgress::WaitingForOnStatus { stream_id } = self else {
            return None;
        };

        let RtmpMessage::CommandMessage {
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
    let video_fourcc_info_map = HashMap::from_iter([
        (
            "*".to_string(),
            AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
        ),
        (
            "avc1".to_string(),
            AmfValue::Number(
                (FOURCC_INFO_CAN_DECODE | FOURCC_INFO_CAN_ENCODE | FOURCC_INFO_CAN_FORWARD) as f64,
            ),
        ),
    ]);
    let props = HashMap::from_iter(
        [
            ("app", config.app.clone().into()),
            ("tcUrl", config.tc_url().into()),
            ("flashVer", "FMS/3,0,1,123".into()),
            // True if proxy is being used
            ("fpad", AmfValue::Boolean(false)),
            // TODO: add config option
            ("audioCodecs", AmfValue::Number(0x0FFF as f64)), // all RTMP supported
            // TODO: add config option
            ("videoCodecs", AmfValue::Number(0x00FF as f64)), // all RTMP supported
            ("videoFunction", AmfValue::Number(0.0)),
            (
                "fourCcList",
                AmfValue::StrictArray(
                    VIDEO_FOURCC_LIST
                        .iter()
                        .map(|v| AmfValue::String((*v).to_string()))
                        .collect(),
                ),
            ),
            (
                "videoFourCcInfoMap",
                AmfValue::Object(video_fourcc_info_map),
            ),
            // TODO: add audioFourCcInfoMap once enhanced audio tags are implemented.
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

    stream.write_msg(RtmpMessage::CommandMessage {
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
    stream.write_msg(RtmpMessage::CommandMessage {
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
    stream.write_msg(RtmpMessage::CommandMessage {
        msg: CommandMessage::Publish {
            stream_key: stream_key.to_string(),
            publishing_type: "live".to_string(),
        },
        stream_id,
    })?;
    Ok(())
}

fn map_supports_enhanced(map: &HashMap<String, AmfValue>) -> bool {
    // TODO: include audio capability indicators once enhanced audio is implemented.
    let has_fourcc_list = map
        .get("fourCcList")
        .is_some_and(fourcc_list_supports_video);
    let has_video_info_map = map
        .get("videoFourCcInfoMap")
        .is_some_and(video_fourcc_info_map_supports_video);

    has_fourcc_list || has_video_info_map
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
