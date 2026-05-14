use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use smelter_api::*;
use smelter_render::image::{ImageSource, ImageType};
use smelter_render::shader;
use smelter_render::web_renderer::{WebEmbeddingMethod, WebRendererSpec as RenderWebRendererSpec};
use smelter_render::Resolution;

type RendererSpec = smelter_render::RendererSpec;

#[track_caller]
fn check_image(raw: serde_json::Value, expected: RendererSpec) {
    let resource = raw.get("resource").unwrap().clone();
    let api: ImageSpec = serde_json::from_value(resource).unwrap();
    let actual = RendererSpec::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_image_err(raw: serde_json::Value, expected_msg: &str) {
    let resource = raw.get("resource").unwrap().clone();
    let api: ImageSpec = serde_json::from_value(resource).unwrap();
    let err = RendererSpec::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_shader(raw: serde_json::Value, expected: RendererSpec) {
    let resource = raw.get("resource").unwrap().clone();
    let api: ShaderSpec = serde_json::from_value(resource).unwrap();
    let actual = RendererSpec::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_web_renderer(raw: serde_json::Value, expected: RendererSpec) {
    let resource = raw.get("resource").unwrap().clone();
    let api: WebRendererSpec = serde_json::from_value(resource).unwrap();
    let actual = RendererSpec::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_serde_err<T: serde::de::DeserializeOwned>(raw: serde_json::Value) {
    let resource = raw.get("resource").unwrap().clone();
    assert!(serde_json::from_value::<T>(resource).is_err());
}

fn image_url(url: &str, image_type: ImageType) -> RendererSpec {
    RendererSpec::Image(smelter_render::image::ImageSpec {
        src: ImageSource::Url {
            url: Arc::from(url),
        },
        image_type,
    })
}

fn image_path(path: &str, image_type: ImageType) -> RendererSpec {
    RendererSpec::Image(smelter_render::image::ImageSpec {
        src: ImageSource::LocalPath {
            path: Arc::from(Path::new(path)),
        },
        image_type,
    })
}

// ── Image: PNG ───────────────────────────────────────────────────────

#[test]
fn image_png_with_url() {
    check_image(
        json!({
            "resource": {
                "asset_type": "png",
                "url": "https://example.com/image.png"
            }
        }),
        image_url("https://example.com/image.png", ImageType::Png),
    );
}

#[test]
fn image_png_with_path() {
    check_image(
        json!({
            "resource": {
                "asset_type": "png",
                "path": "/tmp/image.png"
            }
        }),
        image_path("/tmp/image.png", ImageType::Png),
    );
}

// ── Image: JPEG ──────────────────────────────────────────────────────

#[test]
fn image_jpeg_with_url() {
    check_image(
        json!({
            "resource": {
                "asset_type": "jpeg",
                "url": "https://example.com/photo.jpg"
            }
        }),
        image_url("https://example.com/photo.jpg", ImageType::Jpeg),
    );
}

#[test]
fn image_jpeg_with_path() {
    check_image(
        json!({
            "resource": {
                "asset_type": "jpeg",
                "path": "/tmp/photo.jpg"
            }
        }),
        image_path("/tmp/photo.jpg", ImageType::Jpeg),
    );
}

// ── Image: SVG ───────────────────────────────────────────────────────

#[test]
fn image_svg_with_url() {
    check_image(
        json!({
            "resource": {
                "asset_type": "svg",
                "url": "https://example.com/icon.svg"
            }
        }),
        image_url("https://example.com/icon.svg", ImageType::Svg),
    );
}

#[test]
fn image_svg_with_path_and_resolution() {
    check_image(
        json!({
            "resource": {
                "asset_type": "svg",
                "path": "/tmp/icon.svg",
                "resolution": { "width": 200, "height": 200 }
            }
        }),
        image_path("/tmp/icon.svg", ImageType::Svg),
    );
}

// ── Image: GIF ───────────────────────────────────────────────────────

#[test]
fn image_gif_with_url() {
    check_image(
        json!({
            "resource": {
                "asset_type": "gif",
                "url": "https://example.com/anim.gif"
            }
        }),
        image_url("https://example.com/anim.gif", ImageType::Gif),
    );
}

#[test]
fn image_gif_with_path() {
    check_image(
        json!({
            "resource": {
                "asset_type": "gif",
                "path": "/tmp/anim.gif"
            }
        }),
        image_path("/tmp/anim.gif", ImageType::Gif),
    );
}

// ── Image: Auto ──────────────────────────────────────────────────────

#[test]
fn image_auto_with_url() {
    check_image(
        json!({
            "resource": {
                "asset_type": "auto",
                "url": "https://example.com/image"
            }
        }),
        image_url("https://example.com/image", ImageType::Auto),
    );
}

#[test]
fn image_auto_with_path() {
    check_image(
        json!({
            "resource": {
                "asset_type": "auto",
                "path": "/tmp/image"
            }
        }),
        image_path("/tmp/image", ImageType::Auto),
    );
}

// ── Image errors ─────────────────────────────────────────────────────

#[test]
fn err_image_neither_url_nor_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "png"
            }
        }),
        "\"url\" or \"path\" field is required when registering an image.",
    );
}

#[test]
fn err_image_both_url_and_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "png",
                "url": "https://example.com/image.png",
                "path": "/tmp/image.png"
            }
        }),
        "\"url\" and \"path\" fields are mutually exclusive when registering an image.",
    );
}

#[test]
fn err_image_jpeg_neither_url_nor_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "jpeg"
            }
        }),
        "\"url\" or \"path\" field is required when registering an image.",
    );
}

#[test]
fn err_image_svg_both_url_and_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "svg",
                "url": "https://example.com/icon.svg",
                "path": "/tmp/icon.svg"
            }
        }),
        "\"url\" and \"path\" fields are mutually exclusive when registering an image.",
    );
}

#[test]
fn err_image_gif_neither_url_nor_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "gif"
            }
        }),
        "\"url\" or \"path\" field is required when registering an image.",
    );
}

#[test]
fn err_image_auto_neither_url_nor_path() {
    check_image_err(
        json!({
            "resource": {
                "asset_type": "auto"
            }
        }),
        "\"url\" or \"path\" field is required when registering an image.",
    );
}

#[test]
fn err_serde_image_unknown_asset_type() {
    check_serde_err::<ImageSpec>(json!({
        "resource": {
            "asset_type": "bmp",
            "url": "https://example.com/image.bmp"
        }
    }));
}

#[test]
fn err_serde_image_missing_asset_type() {
    check_serde_err::<ImageSpec>(json!({
        "resource": {
            "url": "https://example.com/image.png"
        }
    }));
}

#[test]
fn err_serde_image_unknown_field() {
    check_serde_err::<ImageSpec>(json!({
        "resource": {
            "asset_type": "png",
            "url": "https://example.com/image.png",
            "unknown": true
        }
    }));
}

// ── Shader ───────────────────────────────────────────────────────────

#[test]
fn shader_basic() {
    check_shader(
        json!({
            "resource": {
                "source": "@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }"
            }
        }),
        RendererSpec::Shader(shader::ShaderSpec {
            source: Arc::from(
                "@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }",
            ),
        }),
    );
}

#[test]
fn shader_empty_source() {
    check_shader(
        json!({
            "resource": {
                "source": ""
            }
        }),
        RendererSpec::Shader(shader::ShaderSpec {
            source: Arc::from(""),
        }),
    );
}

#[test]
fn err_serde_shader_missing_source() {
    check_serde_err::<ShaderSpec>(json!({
        "resource": {}
    }));
}

#[test]
fn err_serde_shader_unknown_field() {
    check_serde_err::<ShaderSpec>(json!({
        "resource": {
            "source": "code",
            "extra": true
        }
    }));
}

// ── WebRenderer ──────────────────────────────────────────────────────

#[test]
fn web_renderer_minimal() {
    check_web_renderer(
        json!({
            "resource": {
                "url": "https://example.com",
                "resolution": { "width": 1920, "height": 1080 }
            }
        }),
        RendererSpec::WebRenderer(RenderWebRendererSpec {
            url: Arc::from("https://example.com"),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            embedding_method: WebEmbeddingMethod::NativeEmbeddingOverContent,
        }),
    );
}

#[test]
fn web_renderer_chromium_embedding() {
    check_web_renderer(
        json!({
            "resource": {
                "url": "https://example.com",
                "resolution": { "width": 1920, "height": 1080 },
                "embedding_method": "chromium_embedding"
            }
        }),
        RendererSpec::WebRenderer(RenderWebRendererSpec {
            url: Arc::from("https://example.com"),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            embedding_method: WebEmbeddingMethod::ChromiumEmbedding,
        }),
    );
}

#[test]
fn web_renderer_native_over_content() {
    check_web_renderer(
        json!({
            "resource": {
                "url": "https://example.com",
                "resolution": { "width": 1280, "height": 720 },
                "embedding_method": "native_embedding_over_content"
            }
        }),
        RendererSpec::WebRenderer(RenderWebRendererSpec {
            url: Arc::from("https://example.com"),
            resolution: Resolution {
                width: 1280,
                height: 720,
            },
            embedding_method: WebEmbeddingMethod::NativeEmbeddingOverContent,
        }),
    );
}

#[test]
fn web_renderer_native_under_content() {
    check_web_renderer(
        json!({
            "resource": {
                "url": "https://example.com",
                "resolution": { "width": 1280, "height": 720 },
                "embedding_method": "native_embedding_under_content"
            }
        }),
        RendererSpec::WebRenderer(RenderWebRendererSpec {
            url: Arc::from("https://example.com"),
            resolution: Resolution {
                width: 1280,
                height: 720,
            },
            embedding_method: WebEmbeddingMethod::NativeEmbeddingUnderContent,
        }),
    );
}

#[test]
fn err_serde_web_renderer_missing_url() {
    check_serde_err::<WebRendererSpec>(json!({
        "resource": {
            "resolution": { "width": 1920, "height": 1080 }
        }
    }));
}

#[test]
fn err_serde_web_renderer_missing_resolution() {
    check_serde_err::<WebRendererSpec>(json!({
        "resource": {
            "url": "https://example.com"
        }
    }));
}

#[test]
fn err_serde_web_renderer_unknown_embedding_method() {
    check_serde_err::<WebRendererSpec>(json!({
        "resource": {
            "url": "https://example.com",
            "resolution": { "width": 1920, "height": 1080 },
            "embedding_method": "unknown_method"
        }
    }));
}

#[test]
fn err_serde_web_renderer_unknown_field() {
    check_serde_err::<WebRendererSpec>(json!({
        "resource": {
            "url": "https://example.com",
            "resolution": { "width": 1920, "height": 1080 },
            "unknown": true
        }
    }));
}
