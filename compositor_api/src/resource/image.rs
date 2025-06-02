use compositor_render::image;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "asset_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageSpec {
    Png {
        url: Option<String>,
        path: Option<String>,
    },
    Jpeg {
        url: Option<String>,
        path: Option<String>,
    },
    Svg {
        url: Option<String>,
        path: Option<String>,
        resolution: Option<Resolution>,
    },
    Gif {
        url: Option<String>,
        path: Option<String>,
    },
    Auto {
        url: Option<String>,
        path: Option<String>,
        resolution: Option<Resolution>,
    },
}

impl TryFrom<ImageSpec> for compositor_render::RendererSpec {
    type Error = TypeError;

    fn try_from(spec: ImageSpec) -> Result<Self, Self::Error> {
        fn from_url_or_path(
            url: Option<String>,
            path: Option<String>,
        ) -> Result<image::ImageSource, TypeError> {
            match (url, path) {
                (None, None) => Err(TypeError::new(
                    "\"url\" or \"path\" field is required when registering an image.",
                )),
                (None, Some(path)) => Ok(image::ImageSource::LocalPath { path }),
                (Some(url), None) => Ok(image::ImageSource::Url { url }),
                (Some(_), Some(_)) => Err(TypeError::new(
                    "\"url\" and \"path\" fields are mutually exclusive when registering an image.",
                )),
            }
        }

        fn resolve_image_source_and_image_type(
            url: Option<String>,
            path: Option<String>,
            resolution: Option<Resolution>,
        ) -> Result<(image::ImageSource, image::ImageType), TypeError> {
            match (url, path) {
                (None, None) => Err(TypeError::new(
                    "\"url\" or \"path\" field is required when registering an image.",
                )),
                (None, Some(path)) => {
                    let extension = std::path::Path::new(&path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_lowercase);
                    let image_type = determine_image_type(extension, resolution)?;
                    Ok((image::ImageSource::LocalPath { path }, image_type))
                }
                (Some(url), None) => {
                    let extension = std::path::Path::new(&url)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_lowercase);
                    let image_type = determine_image_type(extension, resolution)?;
                    Ok((image::ImageSource::Url { url }, image_type))
                }
                (Some(_), Some(_)) => Err(TypeError::new(
                    "\"url\" and \"path\" fields are mutually exclusive when registering an image.",
                )),
            }
        }

        fn determine_image_type(
            extension: Option<String>,
            resolution: Option<Resolution>,
        ) -> Result<image::ImageType, TypeError> {
            let image_type = match extension.as_deref() {
                Some("png") => image::ImageType::Png,
                Some("jpg") | Some("jpeg") => image::ImageType::Jpeg,
                Some("gif") => image::ImageType::Gif,
                Some("svg") => image::ImageType::Svg {
                    resolution: resolution.clone().map(Into::into),
                },
                Some(ext) => {
                    return Err(TypeError::new(format!(
                        "Unsupported file extension: .{}",
                        ext
                    )))
                }
                None => {
                    return Err(TypeError::new(
                        "Missing file extension, unable to determine image type.",
                    ))
                }
            };
            if resolution.is_some() && !matches!(image_type, image::ImageType::Svg { .. }) {
                warn!("Ignoring resolution, only SVG images support custom resolution.");
            }
            Ok(image_type)
        }

        let image = match spec {
            ImageSpec::Png { url, path } => image::ImageSpec {
                src: from_url_or_path(url, path)?,
                image_type: image::ImageType::Png,
            },
            ImageSpec::Jpeg { url, path } => image::ImageSpec {
                src: from_url_or_path(url, path)?,
                image_type: image::ImageType::Jpeg,
            },
            ImageSpec::Svg {
                url,
                path,
                resolution,
            } => image::ImageSpec {
                src: from_url_or_path(url, path)?,
                image_type: image::ImageType::Svg {
                    resolution: resolution.map(Into::into),
                },
            },
            ImageSpec::Gif { url, path } => image::ImageSpec {
                src: from_url_or_path(url, path)?,
                image_type: image::ImageType::Gif,
            },
            ImageSpec::Auto {
                url,
                path,
                resolution,
            } => {
                let (src, image_type) = resolve_image_source_and_image_type(url, path, resolution)?;
                image::ImageSpec { src, image_type }
            }
        };
        Ok(Self::Image(image))
    }
}
