use crate::pipeline::whip_whep::{
    bearer_token::validate_token, error::WhipServerError, WhipWhepState,
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
    State(state): State<WhipWhepState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipServerError> {
    let input_id = InputId(Arc::from(id));

    let connections = state.inputs.0;
    let bearer_token = {
        let guard = connections.lock().unwrap();
        guard
            .get(&input_id)
            .map(|connection| connection.bearer_token.clone())
            .ok_or_else(|| WhipServerError::NotFound(format!("{input_id:?} not found")))?
    };

    validate_token(bearer_token, headers.get("Authorization")).await?;

    let peer_connection = {
        let mut guard = connections.lock().unwrap();
        let Some(connection) = guard.get_mut(&input_id) else {
            return Err(WhipServerError::NotFound(format!("{input_id:?} not found")));
        };
        connection.peer_connection.take()
    };

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
