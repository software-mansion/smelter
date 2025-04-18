use super::{WhipCtx, WhipError};
use compositor_render::error::ErrorStack;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Method, StatusCode,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, error, info};
use url::{ParseError, Url};
use webrtc::{
    ice_transport::{
        ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
        ice_connection_state::RTCIceConnectionState,
    },
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
};

pub async fn connect(
    peer_connection: Arc<RTCPeerConnection>,
    client: Arc<reqwest::Client>,
    whip_ctx: &WhipCtx,
) -> Result<Url, WhipError> {
    let whip_ctx = whip_ctx.clone();
    peer_connection.on_ice_connection_state_change(Box::new(
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

    let offer = peer_connection
        .create_offer(None)
        .await
        .map_err(WhipError::OfferCreationError)?;

    let endpoint_url = Url::parse(&whip_ctx.options.endpoint_url)
        .map_err(|e| WhipError::InvalidEndpointUrl(e, whip_ctx.options.endpoint_url.clone()))?;

    debug!("WHIP endpoint url: {}", endpoint_url);

    let mut header_map = HeaderMap::new();
    header_map.append("Content-Type", HeaderValue::from_static("application/sdp"));

    if let Some(token) = &whip_ctx.options.bearer_token {
        let header_value_str: HeaderValue = match format!("Bearer {token}").parse() {
            Ok(val) => val,
            Err(err) => {
                error!("Ivalid header token, couldn't parse: {}", err);
                HeaderValue::from_static("Bearer")
            }
        };
        header_map.append("Authorization", header_value_str);
    }

    let response = client
        .post(endpoint_url.clone())
        .headers(header_map)
        .body(offer.sdp.clone())
        .send()
        .await
        .map_err(|_| WhipError::RequestFailed(Method::POST, endpoint_url.clone()))?;

    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        let answer = &response
            .text()
            .await
            .map_err(|e| WhipError::BodyParsingError("sdp offer", e))?;
        return Err(WhipError::BadStatus(status, answer.to_string()));
    };

    let location_url_str = response
        .headers()
        .get("location")
        .and_then(|url| url.to_str().ok())
        .ok_or_else(|| WhipError::MissingLocationHeader)?;

    let location_url = match Url::parse(location_url_str) {
        Ok(url) => Ok(url),
        Err(err) => match err {
            ParseError::RelativeUrlWithoutBase => {
                let mut location = endpoint_url.clone();
                location.set_path(location_url_str);
                Ok(location)
            }
            _ => Err(WhipError::InvalidEndpointUrl(
                err,
                location_url_str.to_string(),
            )),
        },
    }?;

    peer_connection
        .set_local_description(offer)
        .await
        .map_err(WhipError::LocalDescriptionError)?;

    let answer = response
        .text()
        .await
        .map_err(|e| WhipError::BodyParsingError("sdp answer", e))?;

    let rtc_answer =
        RTCSessionDescription::answer(answer).map_err(WhipError::RTCSessionDescriptionError)?;

    peer_connection
        .set_remote_description(rtc_answer)
        .await
        .map_err(WhipError::RemoteDescriptionError)?;

    let should_stop = Arc::new(AtomicBool::new(false));
    let location_url_clone = location_url.clone();
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        let bearer_token = whip_ctx.options.bearer_token.clone();
        let client = client.clone();
        let location_url = location_url_clone.clone();
        let should_stop_clone = should_stop.clone();
        Box::pin(async move {
            if should_stop_clone.load(Ordering::Relaxed) {
                return;
            }
            if let Some(candidate) = candidate {
                if let Err(err) =
                    handle_candidate(candidate, bearer_token, client, location_url.clone()).await
                {
                    match err {
                        WhipError::TrickleIceNotSupported => {
                            info!("Trickle ICE not supported by WHIP server");
                            should_stop_clone.store(true, Ordering::Relaxed);
                        }
                        WhipError::EntityTagMissing | WhipError::EntityTagNonMatching => {
                            info!("Entity tags not supported by WHIP output");
                            should_stop_clone.store(true, Ordering::Relaxed);
                        }
                        _ => error!("{}", ErrorStack::new(&err).into_string()),
                    }
                }
            }
        })
    }));

    Ok(location_url)
}

async fn handle_candidate(
    candidate: RTCIceCandidate,
    bearer_token: Option<Arc<str>>,
    client: Arc<Client>,
    location: Url,
) -> Result<(), WhipError> {
    let ice_candidate = candidate
        .to_json()
        .map_err(WhipError::IceCandidateToJsonError)?;

    let mut header_map = HeaderMap::new();
    header_map.append(
        "Content-Type",
        HeaderValue::from_static("application/trickle-ice-sdpfrag"),
    );

    if let Some(token) = bearer_token {
        let header_value_str: HeaderValue = match format!("Bearer {token}").parse() {
            Ok(val) => val,
            Err(err) => {
                error!("Ivalid header token, couldn't parse: {}", err);
                HeaderValue::from_static("Bearer")
            }
        };
        header_map.append("Authorization", header_value_str);
    }

    let response = client
        .patch(location.clone())
        .headers(header_map)
        .body(sdp_from_candidate(ice_candidate))
        .send()
        .await
        .map_err(|_| WhipError::RequestFailed(Method::PATCH, location.clone()))?;

    let status = response.status();
    if status.is_server_error() || status.is_client_error() {
        let trickle_ice_error = match status {
            StatusCode::UNPROCESSABLE_ENTITY | StatusCode::METHOD_NOT_ALLOWED => {
                WhipError::TrickleIceNotSupported
            }
            StatusCode::PRECONDITION_REQUIRED => WhipError::EntityTagMissing,
            StatusCode::PRECONDITION_FAILED => WhipError::EntityTagNonMatching,
            _ => {
                let answer = &response
                    .text()
                    .await
                    .map_err(|e| WhipError::BodyParsingError("ICE Candidate", e))?;
                WhipError::BadStatus(status, answer.to_string())
            }
        };
        return Err(trickle_ice_error);
    };

    Ok(())
}

fn sdp_from_candidate(candidate: RTCIceCandidateInit) -> String {
    let mut sdp = String::new();
    if let Some(mid) = candidate.sdp_mid {
        if !mid.is_empty() {
            sdp.push_str(format!("a=mid:{}\n", mid).as_str());
        }
    }
    if let Some(ufrag) = candidate.username_fragment {
        sdp.push_str(format!("a=ice-ufrag:{}\n", ufrag).as_str());
    }
    sdp.push_str(format!("a=candidate:{}\n", candidate.candidate).as_str());
    sdp
}
