use compositor_render::image;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
            } => image::ImageSpec {
                src: from_url_or_path(url, path)?,
                image_type: image::ImageType::Auto {
                    resolution: resolution.map(Into::into),
                },
            },
        };
        Ok(Self::Image(image))
    }
}
