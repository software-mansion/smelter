use std::sync::Arc;

use axum::{
    Router, async_trait,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

use crate::{
    routes::{
        control_request::{handle_reset, handle_start},
        status::{stats_handler, status_handler},
        ws::ws_handler,
    },
    state::ApiState,
};

use crate::middleware::body_logger_middleware;

pub mod control_request;
pub mod input;
pub mod output;
pub mod resources;
pub mod status;
pub mod ws;

pub fn routes(state: Arc<ApiState>) -> Router {
    let inputs = Router::new()
        .route("/:id/register", post(input::handle_register))
        .route("/:id/unregister", post(input::handle_unregister))
        .route("/:id/update", post(input::handle_update));

    let outputs = Router::new()
        .route("/:id/register", post(output::handle_register))
        .route("/:id/unregister", post(output::handle_unregister))
        .route("/:id/update", post(output::handle_update))
        .route(
            "/:id/request_keyframe",
            post(output::handle_request_keyframe),
        );

    let image = Router::new()
        .route("/:id/register", post(resources::handle_register_image))
        .route("/:id/unregister", post(resources::handle_unregister_image));

    let web = Router::new()
        .route(
            "/:id/register",
            post(resources::handle_register_web_renderer),
        )
        .route(
            "/:id/unregister",
            post(resources::handle_unregister_web_renderer),
        );

    let shader = Router::new()
        .route("/:id/register", post(resources::handle_register_shader))
        .route("/:id/unregister", post(resources::handle_unregister_shader));

    let font = Router::new().route("/register", post(resources::handle_register_font));

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
        .route("/stats", get(stats_handler))
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(body_logger_middleware))
        .with_state(state)
}

/// Wrap axum::Json to return serialization errors as json
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[async_trait]
impl<S, T> FromRequest<S> for Json<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Value>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
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

pub struct Multipart(pub axum::extract::Multipart);

#[async_trait]
impl<S> FromRequest<S> for Multipart
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Value>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
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
