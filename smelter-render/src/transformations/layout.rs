use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    Resolution,
    scene::{BorderRadius, BoxShadow, ImageScalingFilter, RGBAColor, Size},
    state::{RenderCtx, node_texture::NodeTexture},
    types::RenderingMode,
};

mod flatten;
mod layout_renderer;
mod params;
mod resampler;
mod shader;

use self::{
    resampler::{ResampledChild, ResamplerShader},
    shader::LayoutShader,
};

pub(crate) use layout_renderer::LayoutRenderer;
use tracing::error;

pub const DEFAULT_MAX_LAYOUTS_COUNT: usize = 100;

pub(crate) trait LayoutProvider: Send {
    fn layouts(&mut self, pts: Duration, inputs: &[Option<Resolution>]) -> NestedLayout;
    fn resolution(&self, pts: Duration) -> Resolution;
}

pub(crate) struct LayoutNode {
    layout_provider: Box<dyn LayoutProvider>,
    shader: Arc<LayoutShader>,
    resampler: Arc<ResamplerShader>,
    resample_cache: HashMap<usize, ResampledChild>,
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
    },
    #[allow(dead_code)]
    BoxShadow { color: RGBAColor, blur_radius: f32 },
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
        let resampler = ctx.renderers.layout.resampler.clone();

        Self {
            layout_provider,
            shader,
            resampler,
            resample_cache: HashMap::new(),
        }
    }

    pub fn render(
        &mut self,
        ctx: &RenderCtx,
        sources: &[&NodeTexture],
        target: &mut NodeTexture,
        pts: Duration,
    ) {
        let input_resolutions: Vec<Option<Resolution>> = sources
            .iter()
            .map(|node_texture| node_texture.resolution())
            .collect();
        let output_resolution = self.layout_provider.resolution(pts);
        let layouts = self.layout_provider.layouts(pts, &input_resolutions);
        let mut layouts = layouts.flatten(&input_resolutions, output_resolution);

        let global_filter = match ctx.wgpu_ctx.mode {
            RenderingMode::GpuOptimized | RenderingMode::WebGl => ImageScalingFilter::Lanczos3,
            RenderingMode::CpuOptimized => ImageScalingFilter::Bilinear,
        };

        // Resample scaled child nodes to their exact on-screen size, so the
        // layout shader always samples 1:1.
        struct ResampleJob {
            layout_index: usize,
            source_index: usize,
            crop: Crop,
            dst: Resolution,
        }
        let mut resample_jobs: Vec<ResampleJob> = Vec::new();
        if global_filter == ImageScalingFilter::Lanczos3 {
            for (layout_index, layout) in layouts.iter_mut().enumerate() {
                let (width, height) = (layout.width, layout.height);
                let RenderLayoutContent::ChildNode { index, crop, .. } = &mut layout.content else {
                    continue;
                };
                if sources.get(*index).and_then(|t| t.state()).is_none() {
                    continue;
                }
                let dst = Resolution {
                    width: (width.round() as usize).max(1),
                    height: (height.round() as usize).max(1),
                };
                if !ResampledChild::is_needed(crop, dst) {
                    continue;
                }
                resample_jobs.push(ResampleJob {
                    layout_index,
                    source_index: *index,
                    crop: crop.clone(),
                    dst,
                });
                *crop = Crop {
                    top: 0.0,
                    left: 0.0,
                    width: dst.width as f32,
                    height: dst.height as f32,
                };
            }
        }

        let mut encoder =
            ctx.wgpu_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("layout node"),
                });

        self.resample_cache.retain(|layout_index, _| {
            resample_jobs
                .iter()
                .any(|job| job.layout_index == *layout_index)
        });
        for job in &resample_jobs {
            let Some(source) = sources.get(job.source_index).and_then(|s| s.state()) else {
                continue;
            };
            self.resample_cache
                .entry(job.layout_index)
                .or_default()
                .render(
                    ctx.wgpu_ctx,
                    &self.resampler,
                    source,
                    &job.crop,
                    job.dst,
                    &mut encoder,
                );
        }

        let resolved_views: Vec<&wgpu::TextureView> = layouts
            .iter()
            .enumerate()
            .map(|(layout_index, layout)| match &layout.content {
                RenderLayoutContent::ChildNode { index, .. } => {
                    if let Some(resampled) = self.resample_cache.get(&layout_index)
                        && let Some(state) = resampled.output_state()
                    {
                        return state.view();
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
                RenderLayoutContent::Color { .. } | RenderLayoutContent::BoxShadow { .. } => {
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
            &mut encoder,
        );

        ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
    }
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
