mod audio_only;
mod offline_processing;
mod push_input_before_start;
mod required_inputs;
mod schedule_update;
mod unregistering;
mod video_audio;

use crossbeam_channel::Sender;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio_tungstenite::tungstenite;

pub fn start_server_msg_listener(port: u16, event_sender: Sender<tungstenite::Message>) {
    std::thread::Builder::new()
        .name("Websocket Thread".to_string())
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { server_msg_listener(port, event_sender).await });
        })
        .unwrap();
}

async fn server_msg_listener(port: u16, event_sender: Sender<tungstenite::Message>) {
    let url = format!("ws://127.0.0.1:{port}/ws");

    let (ws_stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("Failed to connect");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut outgoing, mut incoming) = ws_stream.split();

    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let tungstenite::Message::Close(None) = &msg {
                let _ = outgoing.send(msg).await;
                return;
            }
            match outgoing.send(msg).await {
                Ok(()) => (),
                Err(e) => {
                    println!("Send Loop: {e:?}");
                    let _ = outgoing.send(tungstenite::Message::Close(None)).await;
                    return;
                }
            }
        }
    });

    let receiver_task = tokio::spawn(async move {
        while let Some(result) = incoming.next().await {
            match result {
                Ok(tungstenite::Message::Close(_)) => {
                    let _ = tx.send(tungstenite::Message::Close(None));
                    return;
                }
                Ok(tungstenite::Message::Ping(data)) => {
                    if tx.send(tungstenite::Message::Pong(data)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(tungstenite::Message::Close(None));
                    return;
                }
                Ok(msg) => {
                    event_sender.send(msg).unwrap();
                }
            }
        }
    });

    sender_task.await.unwrap();
    receiver_task.await.unwrap();
}
