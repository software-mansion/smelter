use crate::pipeline::webrtc::{error::WhipWhepServerError, WhipWhepServerState};
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
) -> Result<StatusCode, WhipWhepServerError> {
    let input_id = InputId(Arc::from(id));

    state.inputs.validate_token(&input_id, &headers).await?;

    let peer_connection = state
        .inputs
        .get_mut_with(&input_id, |input| Ok(input.peer_connection.take()))?;

    match peer_connection {
        Some(peer_connection) => peer_connection.close().await?,
        None => {
            return Err(WhipWhepServerError::InternalError(format!(
                "None peer connection for {input_id:?}"
            )));
        }
    }

    info!("WHIP session terminated for input: {:?}", input_id);
    Ok(StatusCode::OK)
}
