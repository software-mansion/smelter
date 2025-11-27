mod base;
mod interleaved_yuv422;
mod nv12;
mod planar_yuv;
mod rgba_linear;
mod rgba_multiview;
mod rgba_srgb;
pub mod utils;

#[cfg(feature = "web-renderer")]
mod bgra_linear;
#[cfg(feature = "web-renderer")]
mod bgra_srgb;

#[cfg(feature = "web-renderer")]
pub type BgraLinearTexture = bgra_linear::BgraLinearTexture;
#[cfg(feature = "web-renderer")]
pub type BgraSrgbTexture = bgra_srgb::BgraSrgbTexture;

pub type RgbaMultiViewTexture = rgba_multiview::RgbaMultiViewTexture;
pub type RgbaLinearTexture = rgba_linear::RgbaLinearTexture;
pub type RgbaSrgbTexture = rgba_srgb::RgbaSrgbTexture;

pub type PlanarYuvTextures = planar_yuv::PlanarYuvTextures;
pub type InterleavedUyvy422Texture = interleaved_yuv422::InterleavedYuv422Texture;
pub type InterleavedYuyv422Texture = interleaved_yuv422::InterleavedYuv422Texture;
pub type NV12Texture = nv12::NV12Texture;

pub type PlanarYuvVariant = planar_yuv::YuvVariant;

pub use base::TextureExt;
pub use nv12::NV12TextureViewCreateError;
pub use planar_yuv::YuvPendingDownload as PlanarYuvPendingDownload;
