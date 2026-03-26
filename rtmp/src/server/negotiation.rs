use std::{collections::HashMap, ops::Deref, sync::Arc};

use crate::{
    amf0::AmfValue,
    message::{CommandMessage, RtmpMessage},
};

pub const WINDOW_ACK_SIZE: u32 = 2_500_000;
pub const PEER_BANDWIDTH: u32 = 2_500_000;

pub(super) struct NegotiationResult {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub supports_enhanced_video: bool,
}

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
    WaitingForConnect,

    /// -> createStream
    /// <- createStream _result
    WaitingForCreateStream {
        app: Arc<str>,
        supports_enhanced_video: bool,
    },

    /// -> publish
    ///     <- StreamBegin (with real stream id) - we are not waiting for that
    ///     -> DataMessage (metadata)       TODO
    ///     -> SetChunkSize
    /// <- onStatus
    WaitingForPublish {
        app: Arc<str>,
        supports_enhanced_video: bool,
    },
}

impl NegotiationProgress {
    pub fn try_match_connect(&self, msg: &RtmpMessage) -> Option<(u32, Arc<str>, bool)> {
        let NegotiationProgress::WaitingForConnect = self else {
            return None;
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
            return None;
        };
        let CommandMessage::Connect {
            transaction_id,
            command_object,
            ..
        } = msg
        else {
            return None;
        };

        let app = match command_object.get("app") {
            Some(AmfValue::String(app)) => app,
            None | Some(_) => "",
        };

        let supports_enhanced_video = connect_supports_enhanced_video(command_object);

        Some((*transaction_id, Arc::from(app), supports_enhanced_video))
    }

    pub fn try_match_create_stream(&self, msg: &RtmpMessage) -> Option<(u32, Arc<str>)> {
        let NegotiationProgress::WaitingForCreateStream { app, .. } = self else {
            return None;
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
            return None;
        };
        let CommandMessage::CreateStream { transaction_id, .. } = msg else {
            return None;
        };

        Some((*transaction_id, app.clone()))
    }

    pub fn try_match_publish(&self, msg: &RtmpMessage) -> Option<NegotiationResult> {
        let NegotiationProgress::WaitingForPublish {
            app,
            supports_enhanced_video,
        } = self
        else {
            return None;
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
            return None;
        };
        let CommandMessage::Publish { stream_key, .. } = msg else {
            return None;
        };

        Some(NegotiationResult {
            app: app.clone(),
            stream_key: Arc::from(stream_key.deref()),
            supports_enhanced_video: *supports_enhanced_video,
        })
    }
}

const VIDEO_FOURCC_KEYS: [&str; 6] = ["avc1", "hvc1", "vvc1", "av01", "vp09", "vp08"];

fn connect_supports_enhanced_video(command_object: &HashMap<String, AmfValue>) -> bool {
    let has_fourcc_list = command_object
        .get("fourCcList")
        .is_some_and(fourcc_list_supports_video);
    let has_video_info_map = command_object
        .get("videoFourCcInfoMap")
        .is_some_and(video_fourcc_info_map_supports_video);

    has_fourcc_list || has_video_info_map
}

fn fourcc_list_supports_video(value: &AmfValue) -> bool {
    let AmfValue::StrictArray(items) = value else {
        return false;
    };

    items.iter().any(|item| match item {
        AmfValue::String(v) => v == "*" || VIDEO_FOURCC_KEYS.contains(&v.as_str()),
        _ => false,
    })
}

fn video_fourcc_info_map_supports_video(value: &AmfValue) -> bool {
    let map = match value {
        AmfValue::Object(map) => map,
        AmfValue::EcmaArray(map) => map,
        _ => return false,
    };

    map.iter().any(|(k, v)| {
        let AmfValue::Number(mask) = v else {
            return false;
        };
        *mask > 0.0 && (k == "*" || VIDEO_FOURCC_KEYS.contains(&k.as_str()))
    })
}
