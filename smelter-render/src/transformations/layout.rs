use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    Resolution,
    scene::{BorderRadius, BoxShadow, ImageScalingFilter, RGBAColor, Size},
    state::{RenderCtx, node_texture::NodeTexture},
    wgpu::utils::MippedTexture,
};

mod flatten;
mod lanczos_horizontal;
mod layout_renderer;
mod params;
mod shader;

use self::shader::LayoutShader;

pub(crate) use layout_renderer::LayoutRenderer;
use tracing::error;

pub(crate) trait LayoutProvider: Send {
    fn layouts(&mut self, pts: Duration, inputs: &[Option<Resolution>]) -> NestedLayout;
    fn resolution(&self, pts: Duration) -> Resolution;
}

pub(crate) struct LayoutNode {
    layout_provider: Box<dyn LayoutProvider>,
    shader: Arc<LayoutShader>,
    lanczos_horizontal: Arc<lanczos_horizontal::LanczosHorizontalShader>,
    mip_cache: HashMap<usize, MippedTexture>,
    lanczos_cache: HashMap<usize, NodeTexture>,
    passthrough_child_index: Option<usize>,
    direct_nv12_passthrough: bool,
}

#[derive(Debug, Default)]
pub(crate) struct LayoutRenderStats {
    pub(crate) total_ms: f64,
    pub(crate) lanczos_passes: usize,
    pub(crate) layout_passes: usize,
    pub(crate) intermediate_4k_textures: usize,
}

/// When rendering we cut this fragment from texture and stretch it on
/// the expected position
#[derive(Debug, Clone)]
pub struct Crop {
    pub top: f32,
    pub left: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct Mask {
    pub radius: BorderRadius,
    // position of parent on the output frame
    pub top: f32,
    pub left: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
struct RenderLayout {
    // top-left corner, includes border
    top: f32,
    left: f32,

    // size on the output texture, includes border
    width: f32,
    height: f32,

    // Defines what should be cut from the content.
    // - for texture defines part of the texture that will be stretched to
    //   the `self.width/self.height`. It might cut off border radius.
    // - for box shadow

    // Rotated around the center
    rotation_degrees: f32,
    // border radius needs to applied before cropping, so we can't just make it a part of a parent
    // mask
    border_radius: BorderRadius,
    masks: Vec<Mask>,
    content: RenderLayoutContent,
}

#[derive(Debug, Clone)]
enum RenderLayoutContent {
    Color {
        color: RGBAColor,
        border_color: RGBAColor,
        border_width: f32,
    },
    ChildNode {
        index: usize,
        border_color: RGBAColor,
        border_width: f32,
        crop: Crop,
        scaling_filter: ImageScalingFilter,
        lanczos_vertical: bool,
        mip_level: f32,
    },
    #[allow(dead_code)]
    BoxShadow {
        color: RGBAColor,
        blur_radius: f32,
    },
}

#[derive(Debug, Clone)]
pub enum LayoutContent {
    Color(RGBAColor),
    ChildNode { index: usize, size: Size },
    None,
}

#[derive(Debug, Clone)]
pub struct NestedLayout {
    // top-left corner, includes border of current element
    // (0, 0) represents top-left corner of a parent (inner corner if parent has border too)
    //
    // e.g. if parent layout and current layout have border 10 and current layout is at (0, 0) then
    // their top  and left edges will be next to each other without overlapping
    pub top: f32,
    pub left: f32,

    // size on the output texture, includes border
    pub width: f32,
    pub height: f32,

    pub rotation_degrees: f32,
    /// scale will affect content/children, but not the properties of current layout like
    /// top/left/width/height
    pub scale_x: f32,
    pub scale_y: f32,
    /// Crop is applied before scaling.
    ///
    /// If you need to scale before cropping use 2 nested layouts:
    /// - child to scale
    /// - parent to crop
    ///
    /// Depending on content
    /// - For texture it describes what chunk of texture should be cut and stretched on
    ///   width/height
    /// - For layout it cuts of part of it (defined in coordinates system of this component)
    pub crop: Option<Crop>,
    /// Everything outside this mask should not be rendered. Coordinates are relative to
    /// the layouts top-left corner (and not to the 0,0 point that top-left are defined in)
    pub mask: Option<Mask>,
    pub content: LayoutContent,

    pub border_width: f32,
    pub border_color: RGBAColor,
    pub border_radius: BorderRadius,
    pub box_shadow: Vec<BoxShadow>,

    pub(crate) children: Vec<NestedLayout>,
    /// Describes how many children of this component are nodes. This value also
    /// counts `layout` if its content is a `LayoutContent::ChildNode`.
    ///
    /// `child_nodes_count` is not necessarily equal to number of `LayoutContent::ChildNode` in
    /// a sub-tree. For example, if we have a component that conditionally shows one
    /// of its children then child_nodes_count will count all of those components even
    /// though only one of those children will be present in the layouts tree.
    pub(crate) child_nodes_count: usize,
}

impl LayoutNode {
    pub fn new(ctx: &RenderCtx, layout_provider: Box<dyn LayoutProvider>) -> Self {
        let shader = ctx.renderers.layout.shader.clone();
        let lanczos_horizontal = ctx.renderers.layout.lanczos_horizontal.clone();

        Self {
            layout_provider,
            shader,
            lanczos_horizontal,
            mip_cache: HashMap::new(),
            lanczos_cache: HashMap::new(),
            passthrough_child_index: None,
            direct_nv12_passthrough: false,
        }
    }

    pub(crate) fn passthrough_child_index(&self) -> Option<usize> {
        self.passthrough_child_index
    }

    pub(crate) fn direct_nv12_passthrough_texture(&self) -> Option<&NodeTexture> {
        self.direct_nv12_passthrough.then(|| self.lanczos_cache.get(&0)).flatten()
    }

    pub fn render(
        &mut self,
        ctx: &RenderCtx,
        sources: &[&NodeTexture],
        target: &mut NodeTexture,
        pts: Duration,
        encoder: &mut wgpu::CommandEncoder,
    ) -> LayoutRenderStats {
        let started = Instant::now();
        let mut stats = LayoutRenderStats::default();
        let input_resolutions: Vec<Option<Resolution>> =
            sources.iter().map(|node_texture| node_texture.resolution()).collect();
        let output_resolution = self.layout_provider.resolution(pts);
        let layouts = self.layout_provider.layouts(pts, &input_resolutions);
        let mut layouts = layouts.flatten(&input_resolutions, output_resolution);
        self.passthrough_child_index = Self::passthrough_child_index_for(
            &layouts,
            &input_resolutions,
            output_resolution,
        );
        self.direct_nv12_passthrough = false;
        if self.passthrough_child_index.is_some() {
            self.mip_cache.clear();
            self.lanczos_cache.clear();
            return stats;
        }

        // Apply global filter and compute mip levels for Lanczos3 child nodes.
        // Only apply to sources that actually have texture data
        let mut mip_levels_needed: HashMap<usize, u32> = HashMap::new();
        for layout in &mut layouts {
            let layout_width = layout.width;
            let layout_height = layout.height;
            if let RenderLayoutContent::ChildNode {
                index,
                crop,
                scaling_filter,
                mip_level,
                ..
            } = &mut layout.content
            {
                let has_texture = sources.get(*index).and_then(|t| t.state()).is_some();
                if !has_texture {
                    continue;
                }

                *scaling_filter = ctx.scaling_filter;
                let ratio =
                    f32::max(crop.width / layout_width, crop.height / layout_height);
                if ratio > 1.0 {
                    let levels_needed = match scaling_filter {
                        ImageScalingFilter::Lanczos3 => {
                            *mip_level = ratio.log2().floor();
                            // Lanczos3 samples from a single integer mip level
                            *mip_level as u32
                        }
                        ImageScalingFilter::Bilinear => continue,
                    };
                    let entry = mip_levels_needed.entry(*index).or_insert(0);
                    *entry = (*entry).max(levels_needed);
                }
            }
        }

        if let Some((index, resolution)) = Self::direct_nv12_passthrough_for(
            &layouts,
            &input_resolutions,
            output_resolution,
        ) {
            self.mip_cache.clear();
            self.lanczos_cache.retain(|layout_index, _| *layout_index == 0);
            let Some(source) = sources.get(index).and_then(|source| source.state()) else {
                return stats;
            };
            let target = self
                .lanczos_cache
                .entry(0)
                .or_default()
                .ensure_size(ctx.wgpu_ctx, resolution);
            self.lanczos_horizontal.render(
                ctx.wgpu_ctx,
                source,
                source.resolution(),
                target,
                encoder,
            );
            self.direct_nv12_passthrough = true;
            stats.lanczos_passes += 1;
            if resolution.width >= 3840 && resolution.height >= 2160 {
                stats.intermediate_4k_textures += 1;
            }
            stats.total_ms = started.elapsed().as_secs_f64() * 1000.0;
            return stats;
        }

        let format = ctx.wgpu_ctx.default_view_format();
        // Generate mipped textures for sources that need them
        let mut mipped_textures: HashMap<usize, MippedTexture> = HashMap::new();
        for (source_index, max_level) in &mip_levels_needed {
            if let Some(node_texture) = sources.get(*source_index)
                && let Some(state) = node_texture.state()
            {
                let existing = self.mip_cache.remove(source_index);
                let mipped = ctx.wgpu_ctx.utils.mipmap_generator.generate(
                    encoder,
                    ctx.wgpu_ctx,
                    state.texture(),
                    format,
                    // +1 because max_level is 0-indexed but generate expects a count
                    *max_level + 1,
                    existing,
                );
                mipped_textures.insert(*source_index, mipped);
            }
        }

        let mut lanczos_textures_needed = Vec::new();
        for (layout_index, layout) in layouts.iter_mut().enumerate() {
            if self.should_use_separable_lanczos(layout, &input_resolutions) {
                if let RenderLayoutContent::ChildNode {
                    crop,
                    lanczos_vertical,
                    mip_level,
                    ..
                } = &mut layout.content
                {
                    *lanczos_vertical = true;
                    *mip_level = 0.0;
                    lanczos_textures_needed.push((
                        layout_index,
                        Resolution {
                            width: layout.width.round() as usize,
                            height: crop.height.round() as usize,
                        },
                    ));
                    crop.top = 0.0;
                    crop.left = 0.0;
                    crop.width = layout.width.round();
                    crop.height = crop.height.round();
                }
            }
        }

        self.lanczos_cache
            .retain(|layout_index, _| lanczos_textures_needed.iter().any(|(needed, _)| needed == layout_index));
        for (layout_index, resolution) in &lanczos_textures_needed {
            let RenderLayoutContent::ChildNode { index, .. } =
                &layouts[*layout_index].content
            else {
                continue;
            };
            let Some(source) = sources.get(*index).and_then(|source| source.state()) else {
                continue;
            };
            let target = self
                .lanczos_cache
                .entry(*layout_index)
                .or_default()
                .ensure_size(ctx.wgpu_ctx, *resolution);
            self.lanczos_horizontal.render(
                ctx.wgpu_ctx,
                source,
                source.resolution(),
                target,
                encoder,
            );
            stats.lanczos_passes += 1;
            if resolution.width >= 3840 && resolution.height >= 2160 {
                stats.intermediate_4k_textures += 1;
            }
        }

        let resolved_views: Vec<&wgpu::TextureView> = layouts
            .iter()
            .enumerate()
            .map(|(layout_index, layout)| match &layout.content {
                RenderLayoutContent::ChildNode {
                    index,
                    mip_level,
                    lanczos_vertical,
                    ..
                } => {
                    if *lanczos_vertical
                        && let Some(texture) = self.lanczos_cache.get(&layout_index)
                        && let Some(state) = texture.state()
                    {
                        return state.view();
                    }
                    if *mip_level > 0.0
                        && let Some(mipped) = mipped_textures.get(index)
                    {
                        return &mipped.view;
                    }

                    match sources.get(*index) {
                        Some(node_texture) => node_texture
                            .state()
                            .map(|s| s.view())
                            .unwrap_or_else(|| ctx.wgpu_ctx.default_empty_view()),
                        None => {
                            error!("Invalid source index in layout");
                            ctx.wgpu_ctx.default_empty_view()
                        }
                    }
                }
                RenderLayoutContent::Color { .. }
                | RenderLayoutContent::BoxShadow { .. } => {
                    ctx.wgpu_ctx.default_empty_view()
                }
            })
            .collect();

        let target = target.ensure_size(ctx.wgpu_ctx, output_resolution);
        self.shader.render(
            ctx.wgpu_ctx,
            output_resolution,
            layouts,
            &resolved_views,
            target,
            encoder,
        );
        stats.layout_passes += 1;
        if output_resolution.width >= 3840 && output_resolution.height >= 2160 {
            stats.intermediate_4k_textures += 1;
        }

        self.mip_cache = mipped_textures;
        stats.total_ms = started.elapsed().as_secs_f64() * 1000.0;
        stats
    }

    fn should_use_separable_lanczos(
        &self,
        layout: &RenderLayout,
        input_resolutions: &[Option<Resolution>],
    ) -> bool {
        let RenderLayoutContent::ChildNode {
            index,
            crop,
            scaling_filter: ImageScalingFilter::Lanczos3,
            mip_level,
            ..
        } = &layout.content
        else {
            return false;
        };
        if *mip_level != 0.0
            || layout.width <= 0.0
            || layout.height <= 0.0
            || !is_same_px(layout.width, layout.width.round())
            || !is_same_px(crop.height, crop.height.round())
        {
            return false;
        }
        let Some(input_resolution) = input_resolutions.get(*index).copied().flatten()
        else {
            return false;
        };
        let full_source = is_same_px(crop.top, 0.0)
            && is_same_px(crop.left, 0.0)
            && is_same_px(crop.width, input_resolution.width as f32)
            && is_same_px(crop.height, input_resolution.height as f32);
        let upscale = layout.width > crop.width || layout.height > crop.height;
        full_source && upscale
    }

    fn passthrough_child_index_for(
        layouts: &[RenderLayout],
        input_resolutions: &[Option<Resolution>],
        output_resolution: Resolution,
    ) -> Option<usize> {
        let [layout] = layouts else { return None };
        let RenderLayoutContent::ChildNode {
            index,
            border_color: RGBAColor(_, _, _, border_alpha),
            border_width,
            crop,
            ..
        } = &layout.content
        else {
            return None;
        };
        let input_resolution = input_resolutions.get(*index).copied().flatten()?;

        let same_resolution = input_resolution == output_resolution;
        let full_output = is_same_px(layout.top, 0.0)
            && is_same_px(layout.left, 0.0)
            && is_same_px(layout.width, output_resolution.width as f32)
            && is_same_px(layout.height, output_resolution.height as f32);
        let full_crop = is_same_px(crop.top, 0.0)
            && is_same_px(crop.left, 0.0)
            && is_same_px(crop.width, input_resolution.width as f32)
            && is_same_px(crop.height, input_resolution.height as f32);
        let no_effects = is_same_px(layout.rotation_degrees, 0.0)
            && layout.masks.is_empty()
            && layout.border_radius == BorderRadius::ZERO
            && (*border_width == 0.0 || *border_alpha == 0);

        (same_resolution && full_output && full_crop && no_effects).then_some(*index)
    }

    fn direct_nv12_passthrough_for(
        layouts: &[RenderLayout],
        input_resolutions: &[Option<Resolution>],
        output_resolution: Resolution,
    ) -> Option<(usize, Resolution)> {
        let [layout] = layouts else { return None };
        let RenderLayoutContent::ChildNode {
            index,
            border_color: RGBAColor(_, _, _, border_alpha),
            border_width,
            crop,
            scaling_filter: ImageScalingFilter::Lanczos3,
            ..
        } = &layout.content
        else {
            return None;
        };
        let input_resolution = input_resolutions.get(*index).copied().flatten()?;

        let full_output = is_same_px(layout.top, 0.0)
            && is_same_px(layout.left, 0.0)
            && is_same_px(layout.width, output_resolution.width as f32)
            && is_same_px(layout.height, output_resolution.height as f32);
        let full_crop = is_same_px(crop.top, 0.0)
            && is_same_px(crop.left, 0.0)
            && is_same_px(crop.width, input_resolution.width as f32)
            && is_same_px(crop.height, input_resolution.height as f32);
        let no_effects = is_same_px(layout.rotation_degrees, 0.0)
            && layout.masks.is_empty()
            && layout.border_radius == BorderRadius::ZERO
            && (*border_width == 0.0 || *border_alpha == 0);
        let upscale = output_resolution.width > input_resolution.width
            || output_resolution.height > input_resolution.height;

        (full_output && full_crop && no_effects && upscale).then_some((
            *index,
            Resolution {
                width: output_resolution.width,
                height: input_resolution.height,
            },
        ))
    }
}

fn is_same_px(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

impl NestedLayout {
    /// NestedLayout that won't ever be rendered. It's intended to be optimized out
    /// in the flattening process. Its only purpose is to keep track of child nodes that are not
    /// currently used so the index offset can be calculated correctly.
    pub(crate) fn child_nodes_placeholder(child_nodes_count: usize) -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            width: 0.0,
            height: 0.0,
            rotation_degrees: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            crop: None,
            mask: None,
            content: LayoutContent::None,
            children: vec![],
            child_nodes_count,
            border_width: 0.0,
            border_color: RGBAColor(0, 0, 0, 0),
            border_radius: BorderRadius::ZERO,
            box_shadow: vec![],
        }
    }
}
