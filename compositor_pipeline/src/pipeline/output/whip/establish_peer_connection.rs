use super::{whip_http_client::SdpAnswer, PeerConnection, WhipCtx, WhipError};
use compositor_render::error::ErrorStack;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, error, info};
use url::Url;
use webrtc::ice_transport::{
    ice_candidate::RTCIceCandidate, ice_connection_state::RTCIceConnectionState,
};

pub async fn exchange_sdp_offers(
    peer_connection: PeerConnection,
    whip_ctx: Arc<WhipCtx>,
) -> Result<Url, WhipError> {
    {
        let whip_ctx = whip_ctx.clone();
        peer_connection.pc.on_ice_connection_state_change(Box::new(
            move |connection_state: RTCIceConnectionState| {
                debug!("Connection State has changed {connection_state}.");
                if connection_state == RTCIceConnectionState::Connected {
                    debug!("Ice connected.");
                } else if connection_state == RTCIceConnectionState::Failed {
                    debug!("Ice connection failed.");
                    whip_ctx
                        .should_close
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Box::pin(async {})
            },
        ));
    }

    let offer = peer_connection
        .pc
        .create_offer(None)
        .await
        .map_err(WhipError::OfferCreationError)?;

    let SdpAnswer { session_url: location, answer } = whip_ctx.client.send_offer(offer).await?;

    // TODO;
    //  peer_connection
    //      .set_local_description(offer)
    //      .await
    //      .map_err(WhipError::LocalDescriptionError)?;

    peer_connection
        .pc
        .set_remote_description(answer)
        .await
        .map_err(WhipError::RemoteDescriptionError)?;

    listen_for_trickle_candidates(peer_connection, whip_ctx, location.clone());

    Ok(location)
}

fn listen_for_trickle_candidates(
    peer_connection: PeerConnection,
    whip_ctx: Arc<WhipCtx>,
    location: Url,
) {
    let should_stop_trickle = Arc::new(AtomicBool::new(false));
    let location = location.clone();
    peer_connection
        .pc
        .on_ice_candidate(Box::new(move |candidate| {
            Box::pin(handle_trickle_candidate(
                whip_ctx.clone(),
                candidate,
                location.clone(),
                should_stop_trickle.clone(),
            ))
        }));
}

async fn handle_trickle_candidate(
    ctx: Arc<WhipCtx>,
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

    match ctx.client.send_trickle_ice(&location, candidate).await {
        Err(WhipError::TrickleIceNotSupported) => {
            info!("Trickle ICE is not supported by WHIP server");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(WhipError::EntityTagMissing) | Err(WhipError::EntityTagNonMatching) => {
            info!("Entity tags not supported by WHIP output");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(err) => error!("{}", ErrorStack::new(&err).into_string()),
        Ok(_) => (),
    };
}
