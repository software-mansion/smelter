use axum::{
    async_trait,
    extract::{rejection::JsonRejection, ws::WebSocketUpgrade, FromRequest, Request, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use compositor_pipeline::Pipeline;
use serde_json::{json, Value};

use crate::state::{ApiState, Response};

use compositor_api::error::ApiError;

use self::{
    update_output::handle_keyframe_request, update_output::handle_output_update,
    ws::handle_ws_upgrade,
};

mod register_request;
mod unregister_request;
mod update_output;
mod ws;

#[allow(unused_imports)]
pub use register_request::{RegisterInput, RegisterOutput};
#[allow(unused_imports)]
pub use unregister_request::{UnregisterInput, UnregisterOutput};

pub fn routes(state: ApiState) -> Router {
    let inputs = Router::new()
        .route("/:id/register", post(register_request::handle_input))
        .route("/:id/unregister", post(unregister_request::handle_input));

    let outputs = Router::new()
        .route("/:id/register", post(register_request::handle_output))
        .route("/:id/unregister", post(unregister_request::handle_output))
        .route("/:id/update", post(handle_output_update))
        .route("/:id/request_keyframe", post(handle_keyframe_request));

    let image = Router::new()
        .route("/:id/register", post(register_request::handle_image))
        .route("/:id/unregister", post(unregister_request::handle_image));

    let web = Router::new()
        .route("/:id/register", post(register_request::handle_web_renderer))
        .route(
            "/:id/unregister",
            post(unregister_request::handle_web_renderer),
        );

    let shader = Router::new()
        .route("/:id/register", post(register_request::handle_shader))
        .route("/:id/unregister", post(unregister_request::handle_shader));

    async fn handle_start(State(state): State<ApiState>) -> Result<Response, ApiError> {
        Pipeline::start(&state.pipeline);
        Ok(Response::Ok {})
    }

    Router::new()
        .nest("/api/input", inputs)
        .nest("/api/output", outputs)
        .nest("/api/image", image)
        .nest("/api/web-renderer", web)
        .nest("/api/shader", shader)
        // Start request
        .route("/api/start", post(handle_start))
        // WebSocket - events
        .route("/ws", get(ws_handler))
        .route(
            "/status",
            get(axum::Json(json!({
                "instance_id": state.config.instance_id
            }))),
        )
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    // finalize the upgrade process by returning upgrade callback.
    ws.on_upgrade(handle_ws_upgrade)
}

/// Wrap axum::Json to return serialization errors as json
pub(super) struct Json<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for Json<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Value>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (parts, body) = req.into_parts();
        let req = Request::from_parts(parts, body);

        match axum::Json::<T>::from_request(req, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let payload = json!({
                    "error_code": "MALFORMED_REQUEST",
                    "message": rejection.body_text(),
                });

                Err((rejection.status(), axum::Json(payload)))
            }
        }
    }
}
