use std::{fs, io, str::Utf8Error, sync::Arc, time::Duration};

use animated_image::{AnimatedAsset, AnimatedNodeState};
use bitmap_image::{BitmapAsset, BitmapNodeState};
use bytes::Bytes;

use image::ImageFormat;
use resvg::usvg;

use crate::{
    scene::ImageComponent,
    state::{node_texture::NodeTexture, RegisterCtx, RenderCtx},
    wgpu::WgpuCtx,
    Resolution,
};

pub use svg_image::{SvgAsset, SvgNodeState};

mod animated_image;
mod bitmap_image;
mod svg_image;

#[derive(Debug, Clone)]
pub struct ImageSpec {
    pub src: ImageSource,
    pub image_type: ImageType,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    Url { url: String },
    LocalPath { path: String },
    Bytes { bytes: Bytes },
}

#[derive(Debug, Clone)]
pub enum ImageType {
    Png,
    Jpeg,
    Svg { resolution: Option<Resolution> },
    Gif,
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
            ImageType::Svg { resolution } => {
                let asset = SvgAsset::new(&ctx.wgpu_ctx, file, resolution)?;
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
        };
        Ok(renderer)
    }

    pub fn resolution(&self) -> Resolution {
        match self {
            Image::Bitmap(asset) => asset.resolution(),
            Image::Animated(asset) => asset.resolution(),
            Image::Svg(asset) => asset.resolution(),
        }
    }

    fn download_file(src: &ImageSource) -> Result<bytes::Bytes, ImageError> {
        match src {
            ImageSource::Url { url } => {
                #[cfg(target_arch = "wasm32")]
                return Err(ImageError::ImageSourceUrlNotSupported);

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let response = reqwest::blocking::get(url)?;
                    let response = response.error_for_status()?;
                    Ok(response.bytes()?)
                }
            }
            ImageSource::LocalPath { path } => {
                let file = fs::read(path)?;
                Ok(Bytes::from(file))
            }
            ImageSource::Bytes { bytes } => Ok(bytes.clone()),
        }
    }
}

pub struct ImageNode {
    params: ImageComponent,
    node: InnerImageNode,
}

impl ImageNode {
    pub fn new(
        ctx: &WgpuCtx,
        params: ImageComponent,
        image: Image,
        prev_node: Option<&Self>,
    ) -> Self {
        let node = match image {
            Image::Bitmap(asset) => InnerImageNode::Bitmap {
                asset,
                state: BitmapNodeState::new(),
            },
            Image::Animated(asset) => {
                let prev_node = prev_node.and_then(|img| match &img.node {
                    InnerImageNode::Animated { state, .. } => Some((&img.params, state)),
                    _ => None,
                });
                let state = match prev_node {
                    Some((prev_params, state)) if *prev_params == params => state.clone(),
                    _ => AnimatedNodeState::new(),
                };

                InnerImageNode::Animated { asset, state }
            }
            Image::Svg(asset) => InnerImageNode::Svg {
                asset,
                state: SvgNodeState::new(ctx),
            },
        };

        Self { params, node }
    }

    pub fn render(&mut self, ctx: &mut RenderCtx, target: &mut NodeTexture, pts: Duration) {
        let target = target.ensure_size(ctx.wgpu_ctx, self.resolution());
        match &mut self.node {
            InnerImageNode::Bitmap { asset, state } => asset.render(ctx.wgpu_ctx, target, state),
            InnerImageNode::Animated { asset, state } => {
                asset.render(ctx.wgpu_ctx, target, state, pts)
            }
            InnerImageNode::Svg { asset, state } => asset.render(ctx.wgpu_ctx, target, state),
        }
    }

    fn resolution(&self) -> Resolution {
        match &self.node {
            InnerImageNode::Bitmap { asset, .. } => asset.resolution(),
            InnerImageNode::Animated { asset, .. } => asset.resolution(),
            InnerImageNode::Svg { asset, .. } => asset.resolution(),
        }
    }
}

pub enum InnerImageNode {
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
        state: SvgNodeState,
    },
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
    ParsingSvgFailed(#[from] SvgError),

    #[error(transparent)]
    ParsingAnimatedFailed(#[from] AnimatedError),

    #[error("Providing URL as image source is not supported on wasm platform")]
    ImageSourceUrlNotSupported,
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
    #[error(
        "Detected over 1000 frames inside the animated image. This case is not currently supported."
    )]
    TooMuchFrames,

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
