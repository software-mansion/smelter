use std::thread;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use smelter_render::event_handler::{Event, subscribe};
use tokio::sync::mpsc::channel;
use tracing::debug;

pub(super) async fn handle_ws_upgrade(socket: WebSocket) {
    enum InternalMessage {
        Event(Event),
        Close,
        Pong(Vec<u8>),
    }
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (event_sender, mut event_receiver) = channel(100);

    let event_sender_2 = event_sender.clone();
    thread::Builder::new()
        .name("Web socket thread".to_string())
        .spawn(move || {
            let receiver = subscribe();
            for event in receiver {
                if event_sender_2
                    .blocking_send(InternalMessage::Event(event))
                    .is_err()
                {
                    return;
                }
            }
        })
        .unwrap();

    tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            match event {
                InternalMessage::Event(event) => {
                    let serialized = event_to_json(event).to_string();
                    if let Err(err) = socket_sender.send(Message::Text(serialized)).await {
                        debug!(%err, "WebSocket send error.");
                        return;
                    }
                }
                InternalMessage::Close => {
                    if let Err(err) = socket_sender.send(Message::Close(None)).await {
                        debug!(%err, "WebSocket send error.");
                        return;
                    }
                    return;
                }
                InternalMessage::Pong(data) => {
                    if let Err(err) = socket_sender.send(Message::Pong(data)).await {
                        debug!(%err, "WebSocket send error.");
                        return;
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        while let Some(Ok(msg)) = socket_receiver.next().await {
            match msg {
                Message::Close(_) => {
                    let _ = event_sender.send(InternalMessage::Close).await;
                    return;
                }
                Message::Ping(data) => {
                    let _ = event_sender.send(InternalMessage::Pong(data)).await;
                }
                msg => {
                    debug!(?msg, "Received ws message.")
                }
            }
        }
    });
}

fn event_to_json(event: Event) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), event.kind.into());
    for (key, value) in event.properties {
        map.insert(key, value.into());
    }
    map.into()
}
