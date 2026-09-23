use std::sync::Arc;

use axum::extract::{Path, State};
use glyphon::fontdb::Source;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_core::Pipeline;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    routes::{Json, Multipart},
};
use smelter_api::{
    ImageSpec, InputId, OkResponse, OutputId, RegisterInput, RegisterInputResponse, RegisterOutput,
    RegisterOutputResponse, RendererId, ShaderSpec, WebRendererSpec,
};

use super::ApiState;

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/register",
    operation_id = "register_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input registered successfully.", body = RegisterInputResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_input(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<RegisterInput>,
) -> Result<Json<RegisterInputResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let info =
            Pipeline::register_input(&api.pipeline()?, input_id.into(), request.try_into()?)?;
        Ok(Json(info.into()))
    })
    .await
    // `unwrap()` panics only when the task panicked or `response.abort()` was called
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/register",
    operation_id = "register_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output registered successfully.", body = RegisterOutputResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_output(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<RegisterOutput>,
) -> Result<Json<RegisterOutputResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let port =
            Pipeline::register_output(&api.pipeline()?, output_id.into(), request.try_into()?)?;
        Ok(Json(port.into()))
    })
    .await
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/shader/{shader_id}/register",
    operation_id = "register_shader",
    params(("shader_id" = str, Path, description = "Shader ID.")),
    responses(
        (status = 200, description = "Shader registered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_shader(
    State(api): State<Arc<ApiState>>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<ShaderSpec>,
) -> Result<Json<OkResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, shader_id.into(), request.try_into()?)?;
        Ok(Json(OkResponse {}))
    })
    .await
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/web-renderer/{instance_id}/register",
    operation_id = "register_web_renderer",
    params(("instance_id" = str, Path, description = "Web renderer instance ID.")),
    responses(
        (status = 200, description = "Web renderer registered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_web_renderer(
    State(api): State<Arc<ApiState>>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<WebRendererSpec>,
) -> Result<Json<OkResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, instance_id.into(), request.try_into()?)?;
        Ok(Json(OkResponse {}))
    })
    .await
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/image/{image_id}/register",
    operation_id = "register_image",
    params(("image_id" = str, Path, description = "Image ID.")),
    responses(
        (status = 200, description = "Image registered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_image(
    State(api): State<Arc<ApiState>>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<ImageSpec>,
) -> Result<Json<OkResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, image_id.into(), request.try_into()?)?;
        Ok(Json(OkResponse {}))
    })
    .await
    .unwrap()
}

// This type is currently used only for OpenAPI generation
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
pub struct RegisterFontRequest {
    #[schema(format = Binary, content_media_type = "application/octet-stream")]
    pub file: String,
}

#[utoipa::path(
    post,
    path = "/api/font/register",
    operation_id = "register_font",
    request_body(content = RegisterFontRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Font registered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_font(
    State(api): State<Arc<ApiState>>,
    Multipart(mut multipart): Multipart,
) -> Result<Json<OkResponse>, ApiError> {
    let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::malformed_request(&err))?
    else {
        return Err(ApiError::malformed_request(&"Missing font file"));
    };

    let bytes = field
        .bytes()
        .await
        .map_err(|err| ApiError::malformed_request(&err))?;

    let binary_font_source = Source::Binary(Arc::new(bytes));

    tokio::task::spawn_blocking(move || {
        api.pipeline()?
            .lock()
            .unwrap()
            .register_font(binary_font_source);
        Ok(Json(OkResponse {}))
    })
    .await
    .unwrap()
}
