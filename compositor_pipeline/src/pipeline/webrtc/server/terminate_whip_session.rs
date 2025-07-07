use crate::pipeline::webrtc::{
    bearer_token::validate_token, error::WhipServerError, WhipWhepServerState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use compositor_render::InputId;
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whip_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipServerError> {
    let input_id = InputId(Arc::from(id));

    let bearer_token = state
        .inputs
        .get_with(&input_id, |input| Ok(input.bearer_token))?;
    validate_token(&bearer_token, headers.get("Authorization")).await?;

    let peer_connection = state.inputs.take_peer_connection(&input_id)?;

    match peer_connection {
        Some(peer_connection) => peer_connection.close().await?,
        None => {
            return Err(WhipServerError::InternalError(format!(
                "None peer connection for {input_id:?}"
            )));
        }
    }

    info!("WHIP session terminated for input: {:?}", input_id);
    Ok(StatusCode::OK)
}
