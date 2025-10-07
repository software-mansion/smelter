use core::fmt;
use std::{str, sync::Arc};

use bytes::BytesMut;
use resvg::{
    tiny_skia,
    usvg::{self, TreeParsing},
};
use tracing::error;

use crate::{
    RenderingMode, Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{
        WgpuCtx,
        texture::{RgbaLinearTexture, RgbaMultiViewTexture, RgbaSrgbTexture, TextureExt},
        utils::ReinterpretToSrgb,
    },
};

use super::SvgError;

pub struct SvgNodeState {
    was_rendered: bool,
    renderer: SvgRenderer,
    resolution: Resolution,
}

pub struct SvgAsset {
    tree: UnsafeInternalRc<resvg::Tree>,
}

impl fmt::Debug for SvgAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgAsset")
            .field("size", &self.tree.0.size)
            .field("view_box", &self.tree.0.view_box)
            .finish()
    }
}

/// Safety: Should be fine as long as internal type T is never cloned
#[derive(Debug, Clone)]
pub struct UnsafeInternalRc<T>(Arc<T>);
unsafe impl<T> Send for UnsafeInternalRc<T> {}
unsafe impl<T> Sync for UnsafeInternalRc<T> {}

impl SvgAsset {
    pub fn new(_ctx: &WgpuCtx, data: bytes::Bytes) -> Result<Self, SvgError> {
        let text_svg = str::from_utf8(&data)?;
        let tree = usvg::Tree::from_str(text_svg, &Default::default())?;
        let tree = resvg::Tree::from_usvg(&tree);

        Ok(Self {
            tree: UnsafeInternalRc(tree.into()),
        })
    }

    pub fn render(&self, ctx: &WgpuCtx, target: &NodeTextureState, state: &mut SvgNodeState) {
        if state.was_rendered {
            return;
        }

        let resolution = state.resolution();

        match (&mut state.renderer, target) {
            (
                SvgRenderer::GpuOptimized(renderer),
                NodeTextureState::GpuOptimized { texture, .. },
            ) => {
                renderer.render(ctx, &self.tree.0, texture, resolution);
            }
            (SvgRenderer::CpuOptimized, NodeTextureState::CpuOptimized { texture, .. }) => {
                // input is already in sRGB with pre-multiplied alpha
                render_to_texture(ctx, &self.tree.0, texture.texture(), resolution);
            }
            (SvgRenderer::WebGl(renderer), NodeTextureState::WebGl { texture, .. }) => {
                renderer.render(ctx, &self.tree.0, texture, resolution)
            }
            _ => {
                error!("Wrong node texture type");
                return;
            }
        };

        state.was_rendered = true;
    }

    pub fn resolution(&self) -> Resolution {
        Resolution {
            width: self.tree.0.size.width() as usize,
            height: self.tree.0.size.height() as usize,
        }
    }
}

impl SvgNodeState {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self {
            was_rendered: false,
            renderer: match ctx.mode {
                RenderingMode::GpuOptimized => SvgRenderer::GpuOptimized(GpuSvgRenderer::new(ctx)),
                RenderingMode::CpuOptimized => SvgRenderer::CpuOptimized,
                RenderingMode::WebGl => SvgRenderer::WebGl(WebGlSvgRenderer::new(ctx)),
            },
            resolution,
        }
    }
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }
}

enum SvgRenderer {
    GpuOptimized(GpuSvgRenderer),
    CpuOptimized,
    WebGl(WebGlSvgRenderer),
}

/// Render order:
/// - upload sRGB pre-multiplied alpha as linear
/// - remove pre-multiplied alpha (treat as linear, operates on sRGB values directly in shader)
/// - add pre-multiplied alpha (treat as sRGB, operate on linear values in shader)
struct GpuSvgRenderer {
    original_texture: RgbaMultiViewTexture,
    original_texture_linear_bg: wgpu::BindGroup,
    non_premultiplied_texture: RgbaMultiViewTexture,
    non_premultiplied_texture_srgb_bg: wgpu::BindGroup,
}

impl GpuSvgRenderer {
    fn new(ctx: &WgpuCtx) -> Self {
        let original_texture = RgbaMultiViewTexture::new(ctx, Resolution::ONE_PIXEL);
        let non_premultiplied_texture = RgbaMultiViewTexture::new(ctx, Resolution::ONE_PIXEL);
        Self {
            original_texture_linear_bg: original_texture.new_linear_bind_group(ctx),
            non_premultiplied_texture_srgb_bg: non_premultiplied_texture.new_srgb_bind_group(ctx),

            original_texture,
            non_premultiplied_texture,
        }
    }

    fn render(
        &mut self,
        ctx: &WgpuCtx,
        tree: &resvg::Tree,
        target: &RgbaMultiViewTexture,
        resolution: Resolution,
    ) {
        self.ensure_texture_size(ctx, resolution);
        render_to_texture(ctx, tree, self.original_texture.texture(), resolution);

        // interpret source and destination as non-srgb when removing pre-multiplication
        ctx.utils.linear_rgba_remove_premult_alpha.render(
            ctx,
            &self.original_texture_linear_bg,
            self.non_premultiplied_texture.linear_view(),
        );

        // interpret source and destination as srgb when adding pre-multiplication
        ctx.utils.srgb_rgba_add_premult_alpha.render(
            ctx,
            &self.non_premultiplied_texture_srgb_bg,
            target.srgb_view(),
        );
    }

    fn ensure_texture_size(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if Resolution::from(self.original_texture.size()) != resolution {
            self.original_texture = RgbaMultiViewTexture::new(ctx, resolution);
            self.original_texture_linear_bg = self.original_texture.new_linear_bind_group(ctx);
        }
        if Resolution::from(self.non_premultiplied_texture.size()) != resolution {
            self.non_premultiplied_texture = RgbaMultiViewTexture::new(ctx, resolution);
            self.non_premultiplied_texture_srgb_bg =
                self.non_premultiplied_texture.new_srgb_bind_group(ctx);
        }
    }
}

/// Render order:
/// - upload sRGB pre-multiplied alpha as linear
/// - remove pre-multiplied alpha (treat as linear, operates on sRGB values directly in shader)
/// - copy from linear to srgb texture
/// - add pre-multiplied alpha (treat as sRGB, operate on linear values in shader)
struct WebGlSvgRenderer {
    original_texture: RgbaLinearTexture,
    original_texture_linear_bg: wgpu::BindGroup,

    reinterpret_to_srgb: ReinterpretToSrgb,

    non_premultiplied_texture_linear: RgbaLinearTexture,
    non_premultiplied_texture_srgb: RgbaSrgbTexture,

    non_premultiplied_texture_srgb_bg: wgpu::BindGroup,
}

impl WebGlSvgRenderer {
    fn new(ctx: &WgpuCtx) -> Self {
        let original_texture = RgbaLinearTexture::new(ctx, Resolution::ONE_PIXEL);
        let non_premultiplied_texture_linear = RgbaLinearTexture::new(ctx, Resolution::ONE_PIXEL);
        let non_premultiplied_texture_srgb = RgbaSrgbTexture::new(ctx, Resolution::ONE_PIXEL);

        let original_texture_linear_bg = original_texture.new_bind_group(ctx);
        let non_premultiplied_texture_srgb_bg = non_premultiplied_texture_srgb.new_bind_group(ctx);

        Self {
            original_texture,
            original_texture_linear_bg,
            non_premultiplied_texture_linear,
            non_premultiplied_texture_srgb,
            non_premultiplied_texture_srgb_bg,
            reinterpret_to_srgb: ReinterpretToSrgb::new(ctx),
        }
    }

    fn render(
        &mut self,
        ctx: &WgpuCtx,
        tree: &resvg::Tree,
        target: &RgbaSrgbTexture,
        resolution: Resolution,
    ) {
        self.ensure_texture_size(ctx, resolution);
        render_to_texture(ctx, tree, self.original_texture.texture(), resolution);

        // interpret source and destination as non-srgb when removing pre-multiplication
        ctx.utils.linear_rgba_remove_premult_alpha.render(
            ctx,
            &self.original_texture_linear_bg,
            self.non_premultiplied_texture_linear.view(),
        );

        self.reinterpret_to_srgb.convert(
            ctx,
            self.non_premultiplied_texture_linear.texture(),
            self.non_premultiplied_texture_srgb.texture(),
        );

        // interpret source and destination as srgb when adding pre-multiplication
        ctx.utils.srgb_rgba_add_premult_alpha.render(
            ctx,
            &self.non_premultiplied_texture_srgb_bg,
            target.view(),
        );
    }

    fn ensure_texture_size(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if Resolution::from(self.original_texture.size()) != resolution {
            self.original_texture = RgbaLinearTexture::new(ctx, resolution);
            self.non_premultiplied_texture_linear = RgbaLinearTexture::new(ctx, resolution);
            self.non_premultiplied_texture_srgb = RgbaSrgbTexture::new(ctx, resolution);

            self.original_texture_linear_bg = self.original_texture.new_bind_group(ctx);
            self.non_premultiplied_texture_srgb_bg =
                self.non_premultiplied_texture_srgb.new_bind_group(ctx);
        }
    }
}

fn render_to_texture(
    ctx: &WgpuCtx,
    tree: &resvg::Tree,
    texture: &wgpu::Texture,
    resolution: Resolution,
) {
    let mut buffer = BytesMut::zeroed(resolution.width * resolution.height * 4);
    // pre-multiplied sRGB, but in the wrong order
    // we need to remove pre-multiplication -> convert to linear -> add pre-multiplication
    let mut pixmap = tiny_skia::PixmapMut::from_bytes(
        &mut buffer,
        resolution.width as u32,
        resolution.height as u32,
    )
    .unwrap();

    let should_resize = resolution.width != (tree.size.width() as usize)
        || resolution.height != (tree.size.height() as usize);
    let transform = if should_resize {
        let scale_x = resolution.width as f32 / tree.size.width();
        let scale_y = resolution.height as f32 / tree.size.height();
        tiny_skia::Transform::from_scale(scale_x, scale_y)
    } else {
        tiny_skia::Transform::default()
    };

    tree.render(transform, &mut pixmap);

    texture.upload_data(&ctx.queue, pixmap.data_mut(), 4);
    ctx.queue.submit([]);
}
