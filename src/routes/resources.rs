use std::sync::Arc;

use axum::extract::{Path, State};
use glyphon::fontdb::Source;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_api::{
    ImageSpec, OkResponse, RendererId, ShaderSpec, UnregisterRequest, WebRendererSpec,
};
use smelter_core::Pipeline;
use smelter_render::RegistryType;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    routes::{Json, Multipart},
    state::ApiState,
};

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
pub async fn handle_register_shader(
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
    path = "/api/shader/{shader_id}/unregister",
    operation_id = "unregister_shader",
    params(("shader_id" = str, Path, description = "Shader ID.")),
    responses(
        (status = 200, description = "Shader unregistered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Shader not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_unregister_shader(
    State(api): State<Arc<ApiState>>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<UnregisterRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    api.schedule_or_run(request.schedule_time()?, move |pipeline| {
        pipeline.unregister_renderer(&shader_id.into(), RegistryType::Shader)
    })?;
    Ok(Json(OkResponse {}))
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
pub async fn handle_register_web_renderer(
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
    path = "/api/web-renderer/{instance_id}/unregister",
    operation_id = "unregister_web_renderer",
    params(("instance_id" = str, Path, description = "Web renderer ID.")),
    responses(
        (status = 200, description = "Web renderer unregistered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Web renderer not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_unregister_web_renderer(
    State(api): State<Arc<ApiState>>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<UnregisterRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    api.schedule_or_run(request.schedule_time()?, move |pipeline| {
        pipeline.unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)
    })?;
    Ok(Json(OkResponse {}))
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
pub async fn handle_register_image(
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

#[utoipa::path(
    post,
    path = "/api/image/{image_id}/unregister",
    operation_id = "unregister_image",
    params(("image_id" = str, Path, description = "Image ID.")),
    responses(
        (status = 200, description = "Image unregistered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Image not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_unregister_image(
    State(api): State<Arc<ApiState>>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<UnregisterRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    api.schedule_or_run(request.schedule_time()?, move |pipeline| {
        pipeline.unregister_renderer(&image_id.into(), RegistryType::Image)
    })?;
    Ok(Json(OkResponse {}))
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
pub async fn handle_register_font(
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
