use std::time::Duration;

use image::{codecs::gif::GifDecoder, AnimationDecoder, ImageFormat};

use crate::{
    state::node_texture::NodeTextureState,
    wgpu::{
        texture::{RgbaLinearTexture, RgbaSrgbTexture},
        WgpuCtx,
    },
    RenderingMode, Resolution,
};

use super::AnimatedError;

pub struct AnimatedNodeState {
    start_pts: Duration,
    resolution: Resolution,
}

#[derive(Debug)]
pub struct AnimatedAsset {
    frames: Vec<AnimationFrame>,
    animation_duration: Duration,
}

#[derive(Debug)]
enum AnimationFrame {
    Srgb {
        texture: RgbaSrgbTexture,
        bg: wgpu::BindGroup,
        pts: Duration,
        
    },
    Linear {
        texture: RgbaLinearTexture,
        bg: wgpu::BindGroup,
        pts: Duration,
    },
}

impl AnimatedAsset {
    pub(super) fn new(
        ctx: &WgpuCtx,
        data: bytes::Bytes,
        format: ImageFormat,
    ) -> Result<Self, AnimatedError> {
        let decoded_frames = match format {
            ImageFormat::Gif => GifDecoder::new(&data[..])?.into_frames(),
            other => return Err(AnimatedError::UnsupportedImageFormat(other)),
        };

        let mut animation_duration: Duration = Duration::ZERO;
        let mut frames = vec![];
        for frame in decoded_frames {
            let frame = &frame?;
            let buffer = frame.buffer();

            let resolution = Resolution {
                width: buffer.width() as usize,
                height: buffer.height() as usize,
            };
            // let resolution = maybe_resolution.unwrap_or(original_resolution);

            match ctx.mode {
                RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                    let texture = RgbaSrgbTexture::new(ctx, resolution);
                    texture.upload(ctx, buffer);

                    frames.push(AnimationFrame::Srgb {
                        bg: texture.new_bind_group(ctx),
                        texture,
                        pts: animation_duration,
                    });
                }
                RenderingMode::CpuOptimized => {
                    let texture = RgbaLinearTexture::new(ctx, resolution);
                    texture.upload(ctx, buffer);
                    frames.push(AnimationFrame::Linear {
                        bg: texture.new_bind_group(ctx),
                        texture,
                        pts: animation_duration,
                    });
                }
            }

            let delay: Duration = frame.delay().into();
            animation_duration += delay;

            if frames.len() > 1000 {
                return Err(AnimatedError::TooManyFrames);
            }
        }

        let Some(first_frame) = frames.first() else {
            return Err(AnimatedError::NoFrames);
        };
        if frames.len() == 1 {
            return Err(AnimatedError::SingleFrame);
        }
        let first_frame_size = first_frame.texture().size();
        if !frames
            .iter()
            .all(|frame| frame.texture().size() == first_frame_size)
        {
            return Err(AnimatedError::UnsupportedVariableResolution);
        }

        ctx.queue.submit([]);

        // In case only one frame, where first delay is zero
        if animation_duration.is_zero() {
            animation_duration = Duration::from_nanos(1)
        }

        Ok(Self {
            frames,
            animation_duration,
        })
    }

    pub(super) fn render(
        &self,
        ctx: &WgpuCtx,
        target: &NodeTextureState,
        state: &mut AnimatedNodeState,
        pts: Duration,
    ) {
        let animation_pts = Duration::from_nanos(
            ((pts.as_nanos() - state.start_pts.as_nanos()) % self.animation_duration.as_nanos())
                as u64,
        );

        let closest_frame = self
            .frames
            .iter()
            .min_by_key(|frame| u128::abs_diff(frame.pts().as_nanos(), animation_pts.as_nanos()))
            .unwrap();
        match &closest_frame {
            AnimationFrame::Srgb { bg, .. } => {
                ctx.utils
                    .srgb_rgba_add_premult_alpha
                    .render(ctx, bg, target.view());
            }
            AnimationFrame::Linear { bg, .. } => {
                ctx.utils
                    .linear_rgba_add_premult_alpha
                    .render(ctx, bg, target.view());
            }
        }
    }

    pub(super) fn resolution(&self) -> Resolution {
        self.frames.first().unwrap().texture().size().into()
    }
}

impl AnimatedNodeState {
    pub fn new(start_pts: Duration, resolution: Resolution) -> Self {
        Self { start_pts, resolution }
    }
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }
}

impl AnimationFrame {
    fn texture(&self) -> &wgpu::Texture {
        match &self {
            AnimationFrame::Srgb { texture, .. } => texture.texture(),
            AnimationFrame::Linear { texture, .. } => texture.texture(),
        }
    }

    fn pts(&self) -> Duration {
        match &self {
            AnimationFrame::Srgb { pts, .. } => *pts,
            AnimationFrame::Linear { pts, .. } => *pts,
        }
    }
}
