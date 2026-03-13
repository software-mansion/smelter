use std::{ops::Deref, sync::Arc};

use crate::{
    amf0::AmfValue,
    message::{CommandMessage, RtmpMessage},
};

pub const WINDOW_ACK_SIZE: u32 = 2_500_000;
pub const PEER_BANDWIDTH: u32 = 2_500_000;

pub(super) struct NegotiationResult {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
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
    WaitingForCreateStream { app: Arc<str> },

    /// -> publish
    ///     <- StreamBegin (with real stream id) - we are not waiting for that
    ///     -> DataMessage (metadata)       TODO
    ///     -> SetChunkSize
    /// <- onStatus
    WaitingForPublish { app: Arc<str> },
}

impl NegotiationProgress {
    pub fn try_match_connect(&self, msg: &RtmpMessage) -> Option<(u32, Arc<str>)> {
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

        Some((*transaction_id, Arc::from(app)))
    }

    pub fn try_match_create_stream(&self, msg: &RtmpMessage) -> Option<(u32, Arc<str>)> {
        let NegotiationProgress::WaitingForCreateStream { app } = self else {
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
        let NegotiationProgress::WaitingForPublish { app } = self else {
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
        })
    }
}
