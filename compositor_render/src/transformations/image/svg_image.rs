use core::fmt;
use std::{str, sync::Mutex};

use bytes::BytesMut;
use resvg::{
    tiny_skia,
    usvg::{self, TreeParsing},
};

use crate::{
    wgpu::{
        texture::{NodeTexture, RgbaMultiViewTexture},
        RenderingMode, WgpuCtx,
    },
    Resolution,
};

use super::SvgError;

pub struct SvgNodeState {
    pub was_rendered: bool,
    pub renderer: Renderer,
}

pub struct SvgAsset {
    tree: resvg::Tree,
    maybe_resolution: Option<Resolution>,
}

impl fmt::Debug for SvgAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgAsset")
            .field("size", &self.tree.size)
            .field("view_box", &self.tree.view_box)
            .finish()
    }
}

impl SvgAsset {
    pub fn new(
        _ctx: &WgpuCtx,
        data: bytes::Bytes,
        maybe_resolution: Option<Resolution>,
    ) -> Result<Self, SvgError> {
        let text_svg = str::from_utf8(&data)?;
        let tree = usvg::Tree::from_str(text_svg, &Default::default())?;
        let tree = resvg::Tree::from_usvg(&tree);

        Ok(Self {
            tree,
            maybe_resolution,
        })
    }

    /* server with GPU:
     *   - remove pre-multiplied(non srgb view) -> add pre-multiplied(srgb view)
     *   - [no intermediate texture]
     * server with CPU:
     *   - do nothing take bytes as they are
     * webGL:
     *   - remove pre-multiplied (to non-srgb) -> copy-to-srgb-texture -> add pre-multiplied
     *   - [two intermediate textures]
     */
    pub fn render(&self, ctx: &WgpuCtx, target: &mut NodeTexture, state: &Mutex<SvgNodeState>) {
        let mut state = state.lock().unwrap();
        if state.was_rendered {
            return;
        }

        let resolution = self.maybe_resolution.unwrap_or_else(|| Resolution {
            width: self.tree.size.width() as usize,
            height: self.tree.size.height() as usize,
        });

        let target_texture_state = target.ensure_size(ctx, resolution);

        state.renderer.render(
            ctx,
            &self.tree,
            target_texture_state.rgba_texture(),
            resolution,
        );

        state.was_rendered = true;
    }

    pub fn resolution(&self) -> Resolution {
        self.maybe_resolution.unwrap_or_else(|| Resolution {
            width: self.tree.size.width() as usize,
            height: self.tree.size.height() as usize,
        })
    }
}

impl SvgNodeState {
    pub fn new(ctx: &WgpuCtx) -> Self {
        return Self {
            was_rendered: false,
            renderer: Renderer::new(ctx),
        };
    }
}

enum Renderer {
    Gpu(GpuRenderer),
    CpuOptimizded,
}

impl Renderer {
    fn new(ctx: &WgpuCtx) -> Self {
        match ctx.mode {
            RenderingMode::Gpu => Self::Gpu(GpuRenderer::new(ctx)),
            RenderingMode::CpuOptimzied => todo!(),
            RenderingMode::WebGl => todo!(),
        }
    }

    fn render(
        &mut self,
        ctx: &WgpuCtx,
        tree: &resvg::Tree,
        target: &RgbaMultiViewTexture,
        resolution: Resolution,
    ) {
        match self {
            Renderer::Gpu(renderer) => renderer.render(ctx, tree, target, resolution),
            Renderer::CpuOptimizded => {
                render_to_texture(ctx, tree, target, resolution);
            }
        }
    }
}

struct GpuRenderer {
    original_texture: RgbaMultiViewTexture,
    original_texture_bg: wgpu::BindGroup,
    non_premultiplied_texture: RgbaMultiViewTexture,
    non_premultiplied_texture_bg: wgpu::BindGroup,
}

impl GpuRenderer {
    fn new(ctx: &WgpuCtx) -> Self {
        let original_texture = RgbaMultiViewTexture::new(
            ctx,
            Resolution {
                width: 1,
                height: 1,
            },
        );
        let non_premultiplied_texture = RgbaMultiViewTexture::new(
            ctx,
            Resolution {
                width: 1,
                height: 1,
            },
        );
        Self {
            original_texture_bg: original_texture.new_raw_bind_group(ctx, ctx.format.rgba_layout()),
            non_premultiplied_texture_bg: non_premultiplied_texture
                .new_bind_group(ctx, ctx.format.rgba_layout()),

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
        render_to_texture(ctx, tree, &self.original_texture, resolution);

        // interpret source and destination as non-srgb when removing pre-multiplication
        ctx.utils.linear_rgba_remove_premult_alpha.render(
            ctx,
            &self.original_texture_bg,
            self.non_premultiplied_texture
                .raw_view()
                .unwrap_or(self.non_premultiplied_texture.default_view()),
        );

        // interpret source and destination as srgb when adding pre-multiplication
        ctx.utils.srgb_rgba_add_premult_alpha.render(
            ctx,
            &self.non_premultiplied_texture_bg,
            target.default_view(),
        );
    }

    fn ensure_texture_size(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if Resolution::from(self.original_texture.size()) != resolution {
            self.original_texture = RgbaMultiViewTexture::new(ctx, resolution)
        }
        if Resolution::from(self.non_premultiplied_texture.size()) != resolution {
            self.non_premultiplied_texture = RgbaMultiViewTexture::new(ctx, resolution)
        }
    }
}

fn render_to_texture(
    ctx: &WgpuCtx,
    tree: &resvg::Tree,
    texture: &RgbaMultiViewTexture,
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

    let should_resize = resolution.width == (tree.size.width() as usize)
        && resolution.height == (tree.size.height() as usize);
    let transform = if should_resize {
        let scale_multiplier = f32::min(
            resolution.width as f32 / tree.size.width(),
            resolution.height as f32 / tree.size.height(),
        );
        tiny_skia::Transform::from_scale(scale_multiplier, scale_multiplier)
    } else {
        tiny_skia::Transform::default()
    };

    tree.render(transform, &mut pixmap);

    texture.upload(ctx, pixmap.data_mut());
    ctx.queue.submit([]);
}
