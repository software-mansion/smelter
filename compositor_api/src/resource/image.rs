use compositor_render::image;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageSpec {
    pub url: Option<String>,
    pub path: Option<String>,
    pub resolution: Option<Resolution>,
}

impl TryFrom<ImageSpec> for compositor_render::RendererSpec {
    type Error = TypeError;

    fn try_from(spec: ImageSpec) -> Result<Self, Self::Error> {
        fn determine_image_type(
            extension: Option<String>,
            resolution: Option<Resolution>,
        ) -> Result<image::ImageType, TypeError> {
            let image_type = match extension.as_deref() {
                Some("png") => image::ImageType::Png,
                Some("jpg") | Some("jpeg") => image::ImageType::Jpeg,
                Some("gif") => image::ImageType::Gif,
                Some("svg") | Some("svgz") => image::ImageType::Svg {
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

        let (image_source, image_type) =
            match (spec.url, spec.path) {
                (None, None) => {
                    return Err(TypeError::new(
                        "\"url\" or \"path\" field is required when registering an image.",
                    ))
                }
                (None, Some(path)) => {
                    let extension = std::path::Path::new(&path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_lowercase);

                    let image_type = determine_image_type(extension, spec.resolution)?;
                    (image::ImageSource::LocalPath { path }, image_type)
                }
                (Some(url), None) => {
                    let extension = std::path::Path::new(&url)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_lowercase);
                    let image_type = determine_image_type(extension, spec.resolution)?;
                    (image::ImageSource::Url { url }, image_type)
                }
                (Some(_), Some(_)) => return Err(TypeError::new(
                    "\"url\" and \"path\" fields are mutually exclusive when registering an image.",
                )),
            };

        Ok(Self::Image(image::ImageSpec {
            src: image_source,
            image_type,
        }))
    }
}
