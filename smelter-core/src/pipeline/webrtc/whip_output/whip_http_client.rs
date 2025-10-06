use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use tracing::error;
use url::{ParseError, Url};
use webrtc::{
    ice_transport::ice_candidate::RTCIceCandidateInit,
    peer_connection::sdp::session_description::RTCSessionDescription,
};

use super::{WhipOutputError, WhipOutputOptions};

#[derive(Debug)]
pub(super) struct WhipHttpClient {
    http_client: reqwest::Client,
    endpoint_url: Url,
    bearer_token: Option<Arc<str>>,
}

pub(super) struct SdpAnswer {
    pub session_url: Url,
    pub answer: RTCSessionDescription,
}

impl WhipHttpClient {
    pub fn new(options: &WhipOutputOptions) -> Result<Arc<Self>, WhipOutputError> {
        let endpoint_url = Url::parse(&options.endpoint_url).map_err(|e| {
            WhipOutputError::InvalidEndpointUrl(e, options.endpoint_url.to_string())
        })?;

        Ok(Arc::new(Self {
            http_client: reqwest::Client::new(),
            endpoint_url,
            bearer_token: options.bearer_token.clone(),
        }))
    }

    pub async fn send_offer(
        &self,
        offer: &RTCSessionDescription,
    ) -> Result<SdpAnswer, WhipOutputError> {
        let headers = self.header_map(HeaderValue::from_static("application/sdp"));

        let response = self
            .http_client
            .post(self.endpoint_url.clone())
            .headers(headers)
            .body(offer.sdp.clone())
            .send()
            .await
            .map_err(|_| WhipOutputError::RequestFailed(Method::POST, self.endpoint_url.clone()))?;

        let response = map_response_err(response).await?;
        let session_url = self.get_location_from_headers(&response).await?;

        let answer = response
            .text()
            .await
            .map_err(|e| WhipOutputError::BodyParsingError("sdp answer", e))?;

        let answer = RTCSessionDescription::answer(answer)
            .map_err(WhipOutputError::RTCSessionDescriptionError)?;

        Ok(SdpAnswer {
            session_url,
            answer,
        })
    }

    pub async fn send_trickle_ice(
        &self,
        session_url: &Url,
        ice_candidate: RTCIceCandidateInit,
    ) -> Result<(), WhipOutputError> {
        let headers = self.header_map(HeaderValue::from_static("application/trickle-ice-sdpfrag"));
        let response = self
            .http_client
            .patch(session_url.clone())
            .headers(headers)
            .body(sdp_from_candidate(ice_candidate))
            .send()
            .await
            .map_err(|_| WhipOutputError::RequestFailed(Method::PATCH, session_url.clone()))?;

        let status = response.status();
        if status.is_server_error() || status.is_client_error() {
            let trickle_ice_error = match status {
                StatusCode::UNPROCESSABLE_ENTITY | StatusCode::METHOD_NOT_ALLOWED => {
                    WhipOutputError::TrickleIceNotSupported
                }
                StatusCode::PRECONDITION_REQUIRED => WhipOutputError::EntityTagMissing,
                StatusCode::PRECONDITION_FAILED => WhipOutputError::EntityTagNonMatching,
                _ => {
                    let answer = &response
                        .text()
                        .await
                        .map_err(|e| WhipOutputError::BodyParsingError("ICE Candidate", e))?;
                    WhipOutputError::BadStatus(status, answer.to_string())
                }
            };
            return Err(trickle_ice_error);
        };
        Ok(())
    }

    pub async fn delete_session(&self, session_url: Url) {
        // Endpoint is required, but some platforms e.g. Twitch do not implement it
        // so we are silently ignoring
        if let Err(err) = self.http_client.delete(session_url).send().await {
            error!("Error while sending delete whip session request: {}", err);
        }
    }

    fn header_map(&self, content_type: HeaderValue) -> HeaderMap {
        let mut header_map = HeaderMap::new();
        header_map.append("Content-Type", content_type);

        if let Some(token) = &self.bearer_token {
            let header_value_str: HeaderValue = match format!("Bearer {token}").parse() {
                Ok(val) => val,
                Err(err) => {
                    error!("Invalid header token, couldn't parse: {}", err);
                    HeaderValue::from_static("Bearer")
                }
            };
            header_map.append("Authorization", header_value_str);
        }
        header_map
    }

    async fn get_location_from_headers(
        &self,
        response: &reqwest::Response,
    ) -> Result<Url, WhipOutputError> {
        let location_url_str = response
            .headers()
            .get("location")
            .and_then(|url| url.to_str().ok())
            .ok_or_else(|| WhipOutputError::MissingLocationHeader)?;

        let location = match Url::parse(location_url_str) {
            Ok(url) => Ok(url),
            Err(err) => match err {
                ParseError::RelativeUrlWithoutBase => {
                    let mut location = self.endpoint_url.clone();
                    location.set_path(location_url_str);
                    Ok(location)
                }
                _ => Err(WhipOutputError::InvalidEndpointUrl(
                    err,
                    location_url_str.to_string(),
                )),
            },
        }?;

        Ok(location)
    }
}

async fn map_response_err(
    response: reqwest::Response,
) -> Result<reqwest::Response, WhipOutputError> {
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        let answer = &response
            .text()
            .await
            .map_err(|e| WhipOutputError::BodyParsingError("sdp offer", e))?;
        Err(WhipOutputError::BadStatus(status, answer.to_string()))
    } else {
        Ok(response)
    }
}

fn sdp_from_candidate(candidate: RTCIceCandidateInit) -> String {
    let mut sdp = String::new();
    if let Some(mid) = candidate.sdp_mid
        && !mid.is_empty() {
            sdp.push_str(format!("a=mid:{mid}\n").as_str());
        }
    if let Some(ufrag) = candidate.username_fragment {
        sdp.push_str(format!("a=ice-ufrag:{ufrag}\n").as_str());
    }
    sdp.push_str(format!("a=candidate:{}\n", candidate.candidate).as_str());
    sdp
}
