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

    let connection = state.inputs.get_input_connection_options(input_id)?;
    validate_token(connection.bearer_token, headers.get("Authorization")).await?;

    let peer_connection = state.inputs.take_peer_connection(&input_id)?;

    if let Some(peer_connection) = peer_connection {
        peer_connection.close().await?;
    } else {
        return Err(WhipServerError::InternalError(format!(
            "None peer connection for {input_id:?}"
        )));
    }

    info!("WHIP session terminated for input: {:?}", input_id);
    Ok(StatusCode::OK)
}
