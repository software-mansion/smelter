use std::thread;

use axum::{
    extract::{
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use crossbeam_channel::{bounded, select};
use futures_util::{SinkExt, StreamExt};
use smelter_render::event_handler::{Event, subscribe};
use tokio::sync::mpsc::channel;
use tracing::debug;

#[utoipa::path(
    get,
    path = "/ws",
    operation_id = "ws",
    responses(
        (status = 200, description = "WebSocket connection started succesfully."),
        (status = 400, description = "Bad request."),
        (status = 500, description = "Internal server error."),
    ),
    tags = ["ws_request"],
)]
pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    // finalize the upgrade process by returning upgrade callback.
    ws.on_upgrade(handle_ws_upgrade)
}

pub(super) async fn handle_ws_upgrade(socket: WebSocket) {
    enum InternalMessage {
        Event(Event),
        Close,
    }
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (event_sender, mut event_receiver) = channel(100);
    // Dropped when the connection is closed, which stops the event thread.
    let (connection_closed_sender, connection_closed_receiver) = bounded::<()>(0);

    let event_sender_2 = event_sender.clone();
    thread::Builder::new()
        .name("Web socket thread".to_string())
        .spawn(move || {
            let events = subscribe();
            loop {
                select! {
                    recv(events) -> event => {
                        let Ok(event) = event else {
                            return;
                        };
                        if event_sender_2
                            .blocking_send(InternalMessage::Event(event))
                            .is_err()
                        {
                            return;
                        }
                    }
                    recv(connection_closed_receiver) -> _ => return,
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
            }
        }
    });

    tokio::spawn(async move {
        let _connection_closed_sender = connection_closed_sender;
        while let Some(Ok(msg)) = socket_receiver.next().await {
            match msg {
                Message::Close(_) => {
                    let _ = event_sender.send(InternalMessage::Close).await;
                    return;
                }
                // Pings are answered by tungstenite while reading.
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
