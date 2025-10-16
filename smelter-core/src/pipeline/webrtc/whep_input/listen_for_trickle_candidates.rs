use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::pipeline::webrtc::{
    http_client::WhipWhepHttpClient, peer_connection_recvonly::RecvonlyPeerConnection,
};
use smelter_render::error::ErrorStack;
use tracing::{error, info, warn};
use url::Url;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;

use crate::prelude::*;

pub(crate) fn listen_for_trickle_candidates(
    pc: &RecvonlyPeerConnection,
    client: &Arc<WhipWhepHttpClient>,
    location: Url,
) {
    let should_stop_trickle = Arc::new(AtomicBool::new(false));
    let location = location.clone();
    let client = client.clone();
    pc.on_ice_candidate(Box::new(move |candidate| {
        Box::pin(handle_trickle_candidate(
            client.clone(),
            candidate,
            location.clone(),
            should_stop_trickle.clone(),
        ))
    }));
}

async fn handle_trickle_candidate(
    client: Arc<WhipWhepHttpClient>,
    candidate: Option<RTCIceCandidate>,
    location: Url,
    should_stop_trickle: Arc<AtomicBool>,
) {
    if should_stop_trickle.load(Ordering::Relaxed) {
        return;
    }
    let Some(candidate) = candidate else { return };
    let candidate = match candidate.to_json() {
        Ok(candidate) => candidate,
        Err(err) => {
            error!("Failed to process ICE candidate: {}", err);
            return;
        }
    };

    match client.send_trickle_ice(&location, candidate).await {
        Err(WebrtcClientError::TrickleIceNotSupported) => {
            info!("Trickle ICE is not supported by WHEP server");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(WebrtcClientError::EntityTagMissing) | Err(WebrtcClientError::EntityTagNonMatching) => {
            info!("Entity tags not supported by WHEP input");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(err) => warn!(
            "Trickle ICE request failed: {}",
            ErrorStack::new(&err).into_string()
        ),
        Ok(_) => (),
    };
}
