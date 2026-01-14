use crate::{
    error::RtmpError,
    message::{RtmpMessage, message_writer::RtmpMessageWriter},
    protocol::{MessageType, UserControlMessageEvent},
};
use bytes::Bytes;

pub(crate) fn send_window_ack_size(
    writer: &mut RtmpMessageWriter,
    window_size: u32,
) -> Result<(), RtmpError> {
    let payload = window_size.to_be_bytes().to_vec();
    let message = RtmpMessage {
        msg_type: MessageType::WindowAckSize,
        stream_id: 0,
        timestamp: 0,
        payload: Bytes::from(payload),
    };
    writer.write(&message)
}

pub(crate) fn send_set_peer_bandwidth(
    writer: &mut RtmpMessageWriter,
    bandwidth: u32,
    limit_type: u8,
) -> Result<(), RtmpError> {
    let mut payload = bandwidth.to_be_bytes().to_vec();
    payload.push(limit_type);
    let message = RtmpMessage {
        msg_type: MessageType::SetPeerBandwidth,
        stream_id: 0,
        timestamp: 0,
        payload: Bytes::from(payload),
    };
    writer.write(&message)
}

pub(crate) fn send_stream_begin(
    writer: &mut RtmpMessageWriter,
    stream_id: u32,
) -> Result<(), RtmpError> {
    let mut payload = Vec::with_capacity(6);
    payload.extend_from_slice(&(UserControlMessageEvent::StreamBegin as u16).to_be_bytes());
    payload.extend_from_slice(&stream_id.to_be_bytes());

    let message = RtmpMessage {
        msg_type: MessageType::UserControl,
        stream_id: 0,
        timestamp: 0,
        payload: Bytes::from(payload),
    };
    writer.write(&message)
}
