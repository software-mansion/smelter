use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use core::str;
use http_body_util::BodyExt;
use serde_json::Value;
use tracing::{Level, enabled, trace};

pub async fn body_logger_middleware(
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, Response> {
    if !enabled!(target: "smelter::log_request_body", Level::TRACE) {
        return Ok(next.run(request).await);
    }
    let request = buffer_request_body(request).await?;
    let response = next.run(request).await;
    let response = buffer_response_body(response).await?;

    Ok(response)
}

async fn buffer_request_body(request: Request) -> Result<Request, Response> {
    let (parts, body) = request.into_parts();

    let bytes = body
        .collect()
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .to_bytes();

    match serde_json::from_slice::<Value>(&bytes) {
        Ok(body_json) => {
            trace!(target: "smelter::log_request_body", method = ?parts.method, path = ?parts.uri, "Request body: {}", body_json);
        }
        Err(_) => match str::from_utf8(&bytes) {
            Ok(body_str) => {
                trace!(target: "smelter::log_request_body", method = ?parts.method, path = ?parts.uri, "Request body: {}", body_str);
            }
            Err(_) => {
                trace!(target: "smelter::log_request_body", method = ?parts.method, path = ?parts.uri, "Request body: {:?}", bytes);
            }
        },
    }

    Ok(Request::from_parts(parts, Body::from(bytes)))
}

async fn buffer_response_body(response: Response) -> Result<Response, Response> {
    let (parts, body) = response.into_parts();

    let bytes = body
        .collect()
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
        .to_bytes();

    match serde_json::from_slice::<Value>(&bytes) {
        Ok(body_json) => {
            trace!(target: "smelter::log_request_body", status=?parts.status, "Response body: {}", body_json);
        }
        Err(_) => match str::from_utf8(&bytes) {
            Ok(body_str) => {
                trace!(target: "smelter::log_request_body", status=?parts.status, "Response body: {}", body_str);
            }
            Err(_) => {
                trace!(target: "smelter::log_request_body", status=?parts.status, "Response body: {:?}", bytes);
            }
        },
    }

    Ok(Response::from_parts(parts, Body::from(bytes)))
}
