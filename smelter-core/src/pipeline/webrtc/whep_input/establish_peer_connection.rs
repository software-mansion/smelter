use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::pipeline::webrtc::whep_input::{
    peer_connection::PeerConnection,
    whep_http_client::{SdpAnswer, WhepHttpClient},
};
use smelter_render::error::ErrorStack;
use tracing::{debug, error, info, warn};
use url::Url;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::prelude::*;

pub(crate) async fn exchange_sdp_offers(
    pc: &PeerConnection,
    client: &Arc<WhepHttpClient>,
) -> Result<(Url, RTCSessionDescription), WebrtcClientError> {
    let offer = pc.create_offer().await?;
    debug!("SDP offer: {}", offer.sdp);

    let SdpAnswer {
        session_url: location,
        answer,
    } = client.send_offer(&offer).await?;
    debug!("SDP answer: {}", answer.sdp);

    pc.set_local_description(offer).await?;

    listen_for_trickle_candidates(pc, client, location.clone());

    Ok((location, answer))
}

fn listen_for_trickle_candidates(pc: &PeerConnection, client: &Arc<WhepHttpClient>, location: Url) {
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
    client: Arc<WhepHttpClient>,
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
