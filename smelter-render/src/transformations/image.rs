use std::{fs, io, path::Path, str::Utf8Error, sync::Arc, time::Duration};

use animated_image::{AnimatedAsset, AnimatedNodeState};
use bitmap_image::{BitmapAsset, BitmapNodeState};
use bytes::Bytes;

use image::ImageFormat;
use resvg::usvg;
use tracing::debug;

use crate::{
    Resolution,
    scene::{Size, image_component::ImageRenderParams},
    state::{RegisterCtx, RenderCtx, node_texture::NodeTexture},
    wgpu::WgpuCtx,
};

pub use svg_image::{SvgAsset, SvgNodeState};

mod animated_image;
mod bitmap_image;
mod svg_image;

#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpec {
    pub src: ImageSource,
    pub image_type: ImageType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    Url { url: Arc<str> },
    LocalPath { path: Arc<Path> },
    Bytes { bytes: Bytes },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageType {
    Png,
    Jpeg,
    Svg,
    Gif,
    Auto,
}

#[derive(Debug, Clone)]
pub enum Image {
    Bitmap(Arc<BitmapAsset>),
    Animated(Arc<AnimatedAsset>),
    Svg(Arc<SvgAsset>),
}

impl Image {
    pub fn new(ctx: &RegisterCtx, spec: ImageSpec) -> Result<Self, ImageError> {
        let file = Self::download_file(&spec.src)?;
        let renderer = match spec.image_type {
            ImageType::Png => {
                let asset = BitmapAsset::new(&ctx.wgpu_ctx, file, ImageFormat::Png)?;
                Image::Bitmap(Arc::new(asset))
            }
            ImageType::Jpeg => {
                let asset = BitmapAsset::new(&ctx.wgpu_ctx, file, ImageFormat::Jpeg)?;
                Image::Bitmap(Arc::new(asset))
            }
            ImageType::Svg => {
                let asset = SvgAsset::new(&ctx.wgpu_ctx, file)?;
                Image::Svg(Arc::new(asset))
            }
            ImageType::Gif => {
                let asset = AnimatedAsset::new(&ctx.wgpu_ctx, file.clone(), ImageFormat::Gif);
                match asset {
                    Ok(asset) => Image::Animated(Arc::new(asset)),
                    Err(AnimatedError::SingleFrame) => {
                        let asset = BitmapAsset::new(&ctx.wgpu_ctx, file, ImageFormat::Gif)?;
                        Image::Bitmap(Arc::new(asset))
                    }
                    Err(err) => return Err(ImageError::from(err)),
                }
            }
            ImageType::Auto => {
                let format = match image::guess_format(&file) {
                    Ok(format) => format,
                    Err(_) => {
                        let asset = SvgAsset::new(&ctx.wgpu_ctx, file).map_err(|err| {
                            debug!("{:?}", err);
                            ImageError::UnsupportedFormat
                        })?;
                        return Ok(Image::Svg(Arc::new(asset)));
                    }
                };

                match format {
                    ImageFormat::Gif => {
                        let asset =
                            AnimatedAsset::new(&ctx.wgpu_ctx, file.clone(), ImageFormat::Gif);
                        match asset {
                            Ok(asset) => Image::Animated(Arc::new(asset)),
                            Err(AnimatedError::SingleFrame) => {
                                let asset =
                                    BitmapAsset::new(&ctx.wgpu_ctx, file, ImageFormat::Gif)?;
                                Image::Bitmap(Arc::new(asset))
                            }
                            Err(err) => return Err(ImageError::from(err)),
                        }
                    }
                    other_format => {
                        let asset = BitmapAsset::new(&ctx.wgpu_ctx, file, other_format)?;
                        Image::Bitmap(Arc::new(asset))
                    }
                }
            }
        };
        Ok(renderer)
    }

    /// Original image size, for SVG it can be fractional.
    pub fn size(&self) -> Size {
        match self {
            Image::Bitmap(asset) => asset.resolution().into(),
            Image::Animated(asset) => asset.resolution().into(),
            Image::Svg(asset) => asset.size(),
        }
    }

    fn download_file(src: &ImageSource) -> Result<bytes::Bytes, ImageError> {
        match src {
            #[cfg(target_arch = "wasm32")]
            ImageSource::Url { .. } => {
                return Err(ImageError::ImageSourceUrlNotSupported);
            }
            #[cfg(not(target_arch = "wasm32"))]
            ImageSource::Url { url } => {
                let response = reqwest::blocking::get(url.as_ref())?;
                let response = response.error_for_status()?;
                Ok(response.bytes()?)
            }
            ImageSource::LocalPath { path } => {
                let file = fs::read(path)?;
                Ok(Bytes::from(file))
            }
            ImageSource::Bytes { bytes } => Ok(bytes.clone()),
        }
    }
}

pub enum ImageNode {
    Bitmap {
        asset: Arc<BitmapAsset>,
        state: BitmapNodeState,
    },
    Animated {
        asset: Arc<AnimatedAsset>,
        state: AnimatedNodeState,
    },
    Svg {
        asset: Arc<SvgAsset>,
        state: Box<SvgNodeState>,
    },
}

impl ImageNode {
    pub fn new(ctx: &WgpuCtx, image: ImageRenderParams) -> Self {
        match image.image {
            Image::Bitmap(asset) => Self::Bitmap {
                asset,
                state: BitmapNodeState::new(image.resolution),
            },
            Image::Animated(asset) => Self::Animated {
                asset,
                state: AnimatedNodeState::new(image.start_pts, image.resolution),
            },
            Image::Svg(asset) => Self::Svg {
                asset,
                state: SvgNodeState::new(ctx, image.resolution).into(),
            },
        }
    }

    pub fn render(&mut self, ctx: &mut RenderCtx, target: &mut NodeTexture, pts: Duration) {
        let resolution = self.resolution();
        if validate_resolution(ctx.wgpu_ctx, resolution).is_err() {
            // Component size does not fit in a texture, render transparent pixel instead.
            let target = target.ensure_size(ctx.wgpu_ctx, Resolution::ONE_PIXEL);
            target.upload(ctx.wgpu_ctx, &[0; 4]);
            return;
        }

        let target = target.ensure_size(ctx.wgpu_ctx, resolution);
        let encoder = &mut ctx.encoder;
        match self {
            ImageNode::Bitmap { asset, state, .. } => {
                asset.render(ctx.wgpu_ctx, target, state, encoder)
            }
            ImageNode::Animated { asset, state, .. } => {
                asset.render(ctx.wgpu_ctx, target, state, pts, encoder)
            }
            ImageNode::Svg { asset, state, .. } => {
                asset.render(ctx.wgpu_ctx, target, state, encoder)
            }
        }
    }

    fn resolution(&self) -> Resolution {
        match self {
            ImageNode::Bitmap { state, .. } => state.resolution(),
            ImageNode::Animated { state, .. } => state.resolution(),
            ImageNode::Svg { state, .. } => state.resolution(),
        }
    }
}

/// Decoder limits that reject images not fitting in a texture before decoding them.
fn decoder_limits(ctx: &WgpuCtx) -> image::io::Limits {
    let max = ctx.device.limits().max_texture_dimension_2d;
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(max);
    limits.max_image_height = Some(max);
    limits
}

fn validate_resolution(
    ctx: &WgpuCtx,
    resolution: Resolution,
) -> Result<(), UnsupportedResolutionError> {
    let max = ctx.device.limits().max_texture_dimension_2d as usize;
    let range = 1..=max;
    if range.contains(&resolution.width) && range.contains(&resolution.height) {
        Ok(())
    } else {
        Err(UnsupportedResolutionError {
            width: resolution.width,
            height: resolution.height,
            max,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "Unsupported image resolution {width}x{height}, dimensions have to be between 1 and {max}."
)]
pub struct UnsupportedResolutionError {
    width: usize,
    height: usize,
    max: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Failed to download asset: {0}")]
    AssetDownload(#[from] reqwest::Error),

    #[error("Failed to read image from disk: {0}")]
    AssetDiskReadError(#[from] io::Error),

    #[error("Failed to parse an image: {0}")]
    FailedToReadAsBitmap(#[from] image::ImageError),

    #[error(transparent)]
    UnsupportedResolution(#[from] UnsupportedResolutionError),

    #[error(transparent)]
    ParsingSvgFailed(#[from] SvgError),

    #[error(transparent)]
    ParsingAnimatedFailed(#[from] AnimatedError),

    #[error("Providing URL as image source is not supported on wasm platform")]
    ImageSourceUrlNotSupported,

    #[error("Unsupported file format")]
    UnsupportedFormat,
}

#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("Invalid utf-8 content inside SVG file: {0}")]
    InvalidUtf8Content(#[from] Utf8Error),

    #[error("Failed to parse the SVG image: {0}")]
    ParsingSvgFailed(#[from] usvg::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum AnimatedError {
    #[error(transparent)]
    UnsupportedResolution(#[from] UnsupportedResolutionError),

    /// If there is only one frame we return error so the code can fallback to the more efficient
    /// implementation.
    #[error("Single frame")]
    SingleFrame,

    #[error("Animated image does not contain any frames.")]
    NoFrames,

    #[error("Failed to read animated image, variable resolution is not supported.")]
    UnsupportedVariableResolution,

    #[error("Failed to parse image: {0}")]
    FailedToParse(#[from] image::ImageError),

    #[error("Unsupported animated image format: {0:?}")]
    UnsupportedImageFormat(ImageFormat),
}
