use std::sync::Arc;

use axum::{
    async_trait,
    extract::{rejection::JsonRejection, ws::WebSocketUpgrade, FromRequest, Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use smelter_core::Pipeline;
use tower_http::cors::CorsLayer;

use crate::{
    error::ApiError,
    routes::status::status_handler,
    state::{ApiState, Response},
};

use self::{
    update_output::handle_keyframe_request, update_output::handle_output_update,
    ws::handle_ws_upgrade,
};
use crate::middleware::body_logger_middleware;

mod register_request;
mod status;
mod unregister_request;
mod update_output;
mod ws;

pub use register_request::{RegisterInput, RegisterOutput};
pub use unregister_request::{UnregisterInput, UnregisterOutput};
pub use update_output::UpdateOutputRequest;

pub fn routes(state: Arc<ApiState>) -> Router {
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

    let font = Router::new().route("/register", post(register_request::handle_font));

    async fn handle_start(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
        Pipeline::start(&state.pipeline()?);
        Ok(Response::Ok {})
    }

    async fn handle_reset(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
        tokio::task::spawn_blocking(move || state.reset())
            .await
            .unwrap()?;
        Ok(Response::Ok {})
    }

    Router::new()
        .nest("/api/input", inputs)
        .nest("/api/output", outputs)
        .nest("/api/image", image)
        .nest("/api/web-renderer", web)
        .nest("/api/shader", shader)
        .nest("/api/font", font)
        // Start request
        .route("/api/start", post(handle_start))
        .route("/api/reset", post(handle_reset))
        // WebSocket - events
        .route("/ws", get(ws_handler))
        .route("/status", get(status_handler))
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(body_logger_middleware))
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

pub(super) struct Multipart(pub axum::extract::Multipart);

#[async_trait]
impl<S> FromRequest<S> for Multipart
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Value>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (parts, body) = req.into_parts();
        let req = Request::from_parts(parts, body);

        match axum::extract::Multipart::from_request(req, state).await {
            Ok(multipart) => Ok(Multipart(multipart)),
            Err(rejection) => {
                let payload = json!({
                    "error_code": "MALFORMED_MULTIPART",
                    "message": rejection.body_text(),
                });

                Err((StatusCode::BAD_REQUEST, axum::Json(payload)))
            }
        }
    }
}
