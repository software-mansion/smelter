use anyhow::{Result, anyhow};

use futures_util::{SinkExt, StreamExt};
use reqwest::{StatusCode, blocking::Response};
use smelter::{config::read_config, server};
use std::{
    process, thread,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite;
use tracing::{error, info};

use serde::Serialize;

use crate::media::download_all_samples;

pub fn post<T: Serialize + ?Sized>(route: &str, json: &T) -> Result<Response> {
    info!("[example] Sent post request to `{route}`.");

    let client = reqwest::blocking::Client::new();
    let response_result = client
        .post(format!(
            "http://127.0.0.1:{}/api/{}",
            read_config().api_port,
            route
        ))
        .timeout(Duration::from_secs(100))
        .json(json)
        .send();

    match response_result {
        Ok(response) if response.status() >= StatusCode::BAD_REQUEST => {
            log_request_error(&json, response);
            Err(anyhow!("Request failed."))
        }
        Ok(response) => Ok(response),
        Err(_) => {
            error!("Couldn't send request. Make sure the example server is running.");
            Err(anyhow!("Request failed."))
        }
    }
}

pub fn run_example(client_code: fn() -> Result<()>) {
    thread::spawn(move || {
        ffmpeg_next::format::network::init();

        download_all_samples().unwrap();

        if let Err(err) = wait_for_server_ready(Duration::from_secs(10)) {
            error!("{err}");
            process::exit(1);
        }

        thread::spawn(move || {
            if let Err(err) = client_code() {
                error!("{err}");
                process::exit(1);
            }
        });

        start_server_msg_listener();
    });
    server::run();
}

pub fn run_example_server() {
    thread::spawn(move || {
        ffmpeg_next::format::network::init();

        download_all_samples().unwrap();

        if let Err(err) = wait_for_server_ready(Duration::from_secs(10)) {
            error!("{err}");
            process::exit(1);
        }

        start_server_msg_listener();
    });
    server::run();
}

fn wait_for_server_ready(timeout: Duration) -> Result<()> {
    let server_status_url = format!("http://127.0.0.1:{}/status", read_config().api_port);
    let wait_start_time = Instant::now();
    loop {
        match reqwest::blocking::get(&server_status_url) {
            Ok(_) => break,
            Err(_) => info!("Waiting for the server to start."),
        };
        if wait_start_time.elapsed() > timeout {
            return Err(anyhow!("Error while starting server, timeout exceeded."));
        }
        thread::sleep(Duration::from_millis(1000));
    }
    Ok(())
}

// has to be public as long as docker is enabled externally, not through this crate
pub fn start_server_msg_listener() {
    thread::Builder::new()
        .name("Websocket Thread".to_string())
        .spawn(|| {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { server_msg_listener().await });
        })
        .unwrap();
}

async fn server_msg_listener() {
    let url = format!("ws://127.0.0.1:{}/ws", read_config().api_port);

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
                _ => {
                    info!("Received compositor event: {:?}", result);
                }
            }
        }
    });

    sender_task.await.unwrap();
    receiver_task.await.unwrap();
}

fn log_request_error<T: Serialize + ?Sized>(request_body: &T, response: Response) {
    let status = response.status();
    let request_str = serde_json::to_string_pretty(request_body).unwrap();
    let body_str = response.text().unwrap();

    let formated_body = get_formated_body(&body_str);
    error!(
        "Request failed:\n\nRequest: {}\n\nResponse code: {}\n\nResponse body:\n{}",
        request_str, status, formated_body
    )
}

fn get_formated_body(body_str: &str) -> String {
    let Ok(mut body_json) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return body_str.to_string();
    };

    let Some(stack_value) = body_json.get("stack") else {
        return serde_json::to_string_pretty(&body_json).unwrap();
    };

    let errors: Vec<&str> = stack_value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let msg_string = " - ".to_string() + &errors.join("\n - ");
    let body_map = body_json.as_object_mut().unwrap();
    body_map.remove("stack");
    format!(
        "{}\n\nError stack:\n{}",
        serde_json::to_string_pretty(&body_map).unwrap(),
        msg_string,
    )
}
