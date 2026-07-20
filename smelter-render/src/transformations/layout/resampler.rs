use std::sync::Arc;

use crate::{
    Resolution,
    state::node_texture::{NodeTexture, NodeTextureState},
    wgpu::{
        WgpuCtx, WgpuErrorScope,
        common_pipeline::{self, CreateShaderError},
    },
};

use super::Crop;

const LABEL: Option<&str> = Some("Lanczos3 resampler");

/// Ratios up to this use the scaled kernel alone; beyond it a single box pass
/// reduces the source first, keeping the tap count bounded and the box away from
/// ratios where its softening would show.
const KERNEL_BUDGET: f32 = 4.0;

/// Unreachable for real resolutions; only keeps degenerate float input from
/// overflowing the `1 << levels` shifts.
const MAX_PREDECIMATE_LEVELS: u32 = 16;

/// Separable Lanczos3 resampler: the kernel scales with the ratio, so one pair
/// of passes covers any up/downscale without mip pyramids. Everything before the
/// final pass runs in linear Rgba16Float, since a unorm intermediate would clamp
/// the kernel's negative lobes (~17 dB PSNR loss on upscaled content).
pub struct ResamplerShader {
    pipeline: wgpu::RenderPipeline,
    pipeline_f16: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    Horizontal = 0,
    Vertical = 1,
}

/// `dst_len` output texels covering `crop_len` source texels from `crop_offset`.
#[derive(Debug, Clone, Copy)]
struct AxisMapping {
    axis: Axis,
    crop_offset: f32,
    crop_len: f32,
    dst_len: usize,
}

impl AxisMapping {
    fn scale(&self) -> f32 {
        self.crop_len / self.dst_len as f32
    }

    /// Box halvings to apply before the kernel, leaving a residual <= budget.
    fn predecimate_levels(&self) -> u32 {
        ((self.scale() / KERNEL_BUDGET).log2().ceil().max(0.0) as u32).min(MAX_PREDECIMATE_LEVELS)
    }

    fn on_reduced_source(self, levels: u32) -> AxisMapping {
        let factor = (1u32 << levels) as f32;
        AxisMapping {
            crop_offset: self.crop_offset / factor,
            crop_len: self.crop_len / factor,
            ..self
        }
    }

    /// Whole-texel translation at 1:1 scale: applied as `perp_offset` by the
    /// other axis' pass instead of needing its own.
    fn as_direct(&self) -> Option<i32> {
        let direct = is_same_px(self.crop_len, self.dst_len as f32)
            && is_same_px(self.crop_offset, self.crop_offset.round());
        direct.then(|| self.crop_offset.round() as i32)
    }

    /// Resolution after resampling `src` along this axis.
    fn output_size(&self, src: Resolution) -> Resolution {
        match self.axis {
            Axis::Horizontal => Resolution {
                width: self.dst_len,
                height: src.height,
            },
            Axis::Vertical => Resolution {
                width: src.width,
                height: self.dst_len,
            },
        }
    }
}

/// One kernel pass along `mapping.axis`, translating the other axis by `perp_offset`.
#[derive(Debug, Clone, Copy)]
struct KernelPass {
    mapping: AxisMapping,
    perp_offset: i32,
}

impl KernelPass {
    fn to_immediates(self) -> [u8; 16] {
        let mut data = [0u8; 16];
        data[0..4].copy_from_slice(&(self.mapping.axis as u32).to_le_bytes());
        data[4..8].copy_from_slice(&self.mapping.scale().to_le_bytes());
        data[8..12].copy_from_slice(&self.mapping.crop_offset.to_le_bytes());
        data[12..16].copy_from_slice(&self.perp_offset.to_le_bytes());
        data
    }
}

/// Kernel passes covering both axes, or `None` when the crop lands on whole
/// texels and the layout shader can sample the source directly. Every part of
/// the crop is applied by exactly one pass: an axis either gets its own, or
/// rides along the other axis' pass as `perp_offset`.
#[derive(Debug, Clone, Copy)]
enum Passes {
    Single(KernelPass),
    Separable {
        first: KernelPass,
        second: KernelPass,
    },
}

fn plan_passes(mappings: [AxisMapping; 2]) -> Option<Passes> {
    let pass = |mapping, perp_offset| KernelPass {
        mapping,
        perp_offset,
    };
    let [horizontal, vertical] = mappings;
    match (horizontal.as_direct(), vertical.as_direct()) {
        (Some(_), Some(_)) => None,
        (None, Some(perp)) => Some(Passes::Single(pass(horizontal, perp))),
        (Some(perp), None) => Some(Passes::Single(pass(vertical, perp))),
        (None, None) => {
            // Stronger shrink first, to minimize the intermediate size and tap count.
            let (first, second) = match vertical.scale() > horizontal.scale() {
                true => (vertical, horizontal),
                false => (horizontal, vertical),
            };
            Some(Passes::Separable {
                first: pass(first, 0),
                second: pass(second, 0),
            })
        }
    }
}

#[derive(Default)]
pub(super) struct ResampledChild {
    reduced: Option<Intermediate>,
    intermediate: Option<Intermediate>,
    output: NodeTexture,
}

/// Cached linear Rgba16Float scratch texture.
struct Intermediate {
    resolution: Resolution,
    view: wgpu::TextureView,
    _texture: wgpu::Texture,
}

impl Intermediate {
    fn ensure<'a>(
        slot: &'a mut Option<Intermediate>,
        wgpu_ctx: &WgpuCtx,
        resolution: Resolution,
    ) -> &'a wgpu::TextureView {
        if slot.as_ref().is_none_or(|c| c.resolution != resolution) {
            let texture = wgpu_ctx.device.create_texture(&wgpu::TextureDescriptor {
                label: LABEL,
                size: wgpu::Extent3d {
                    width: resolution.width as u32,
                    height: resolution.height as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            *slot = Some(Intermediate {
                resolution,
                view: texture.create_view(&Default::default()),
                _texture: texture,
            });
        }
        &slot.as_ref().unwrap().view
    }
}

impl ResamplerShader {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let scope = WgpuErrorScope::push(&wgpu_ctx.device);

        let make_layout = |immediate_size| {
            wgpu_ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: LABEL,
                    bind_group_layouts: &[Some(&wgpu_ctx.format.single_texture_layout)],
                    immediate_size,
                })
        };

        let resample = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("resample.wgsl"));
        let resample_layout = make_layout(16);
        let build = |format| {
            common_pipeline::create_render_pipeline(
                "Lanczos3 resampler",
                &wgpu_ctx.device,
                &resample_layout,
                &resample,
                format,
            )
        };
        let pipeline = build(wgpu_ctx.default_view_format());
        let pipeline_f16 = build(wgpu::TextureFormat::Rgba16Float);

        let downsample_shader = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("downsample.wgsl"));
        let downsample = common_pipeline::create_render_pipeline(
            "Lanczos3 resampler (box reduce)",
            &wgpu_ctx.device,
            &make_layout(8),
            &downsample_shader,
            wgpu::TextureFormat::Rgba16Float,
        );

        scope.pop()?;
        Ok(Self {
            pipeline,
            pipeline_f16,
            downsample,
        })
    }

    fn pass(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        pipeline: &wgpu::RenderPipeline,
        source_view: &wgpu::TextureView,
        immediates: &[u8],
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let source_bg = wgpu_ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: LABEL,
                layout: &wgpu_ctx.format.single_texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                }],
            });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: LABEL,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                view: target,
                resolve_target: None,
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(pipeline);
        render_pass.set_immediates(0, immediates);
        render_pass.set_bind_group(0, &source_bg, &[]);
        wgpu_ctx.plane.draw(&mut render_pass);
    }
}

impl ResampledChild {
    pub(super) fn is_needed(crop: &Crop, dst: Resolution) -> bool {
        plan_passes(axis_mappings(crop, dst)).is_some()
    }

    /// Crop left for the layout shader: the resampler consumes the entire
    /// original crop and emits the exact `dst`-sized rect.
    pub(super) fn output_crop(dst: Resolution) -> Crop {
        Crop {
            top: 0.0,
            left: 0.0,
            width: dst.width as f32,
            height: dst.height as f32,
        }
    }

    pub(super) fn output_state(&self) -> Option<&NodeTextureState> {
        self.output.state()
    }

    pub(super) fn render(
        &mut self,
        wgpu_ctx: &Arc<WgpuCtx>,
        shader: &ResamplerShader,
        source: &NodeTextureState,
        crop: &Crop,
        dst: Resolution,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mappings = axis_mappings(crop, dst);
        let levels = mappings.map(|m| m.predecimate_levels());
        let factor = levels.map(|l| 1u32 << l);

        // Box-reduce the ratio above the budget (a 2^k box == k cascaded 2:1
        // halvings, so one pass suffices), then the kernel takes the residual.
        let (src_view, src_res) = if factor == [1, 1] {
            self.reduced = None;
            (source.view(), source.resolution())
        } else {
            let full = source.resolution();
            let reduced = Resolution {
                width: full.width.div_ceil(factor[0] as usize),
                height: full.height.div_ceil(factor[1] as usize),
            };
            let view = Intermediate::ensure(&mut self.reduced, wgpu_ctx, reduced);
            let bytes = bytemuck::bytes_of(&factor);
            shader.pass(
                wgpu_ctx,
                &shader.downsample,
                source.view(),
                bytes,
                view,
                encoder,
            );
            (view, reduced)
        };

        let residual = std::array::from_fn(|axis| mappings[axis].on_reduced_source(levels[axis]));
        let passes = plan_passes(residual)
            .expect("box reduction leaves a residual scale, axes stay off the direct path");

        // A second axis goes through the f16 intermediate before the last pass.
        let (view, last) = match passes {
            Passes::Single(pass) => {
                self.intermediate = None;
                (src_view, pass)
            }
            Passes::Separable { first, second } => {
                let mid = Intermediate::ensure(
                    &mut self.intermediate,
                    wgpu_ctx,
                    first.mapping.output_size(src_res),
                );
                shader.pass(
                    wgpu_ctx,
                    &shader.pipeline_f16,
                    src_view,
                    &first.to_immediates(),
                    mid,
                    encoder,
                );
                (mid, second)
            }
        };
        let target = self.output.ensure_size(wgpu_ctx, dst).view();
        shader.pass(
            wgpu_ctx,
            &shader.pipeline,
            view,
            &last.to_immediates(),
            target,
            encoder,
        );
    }
}

fn axis_mappings(crop: &Crop, dst: Resolution) -> [AxisMapping; 2] {
    [
        AxisMapping {
            axis: Axis::Horizontal,
            crop_offset: crop.left,
            crop_len: crop.width,
            dst_len: dst.width,
        },
        AxisMapping {
            axis: Axis::Vertical,
            crop_offset: crop.top,
            crop_len: crop.height,
            dst_len: dst.height,
        },
    ]
}

fn is_same_px(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(left: f32, top: f32, width: f32, height: f32, dst: (usize, usize)) -> Option<Passes> {
        let crop = Crop {
            top,
            left,
            width,
            height,
        };
        let dst = Resolution {
            width: dst.0,
            height: dst.1,
        };
        plan_passes(axis_mappings(&crop, dst))
    }

    #[test]
    fn plans_a_pass_for_every_non_direct_axis() {
        // Both axes on whole texels: the layout shader samples 1:1 directly.
        assert!(plan(0.0, 0.0, 640.0, 360.0, (640, 360)).is_none());
        assert!(plan(100.0, 40.0, 640.0, 360.0, (640, 360)).is_none());

        // A 1:1 axis with a whole-texel offset rides the other axis' pass.
        let Some(Passes::Single(pass)) = plan(100.0, 0.0, 640.0, 360.0, (640, 300)) else {
            panic!()
        };
        assert_eq!((pass.mapping.axis, pass.perp_offset), (Axis::Vertical, 100));
        let Some(Passes::Single(pass)) = plan(0.0, 42.0, 640.0, 360.0, (320, 360)) else {
            panic!()
        };
        assert_eq!(
            (pass.mapping.axis, pass.perp_offset),
            (Axis::Horizontal, 42)
        );

        // A subpixel offset needs its own kernel pass to interpolate.
        let fractional = plan(100.5, 0.0, 640.0, 360.0, (640, 300));
        assert!(matches!(fractional, Some(Passes::Separable { .. })));

        // Stronger shrink first, to minimize the intermediate size.
        let Some(Passes::Separable { first, second }) = plan(0.0, 0.0, 1920.0, 1080.0, (960, 270))
        else {
            panic!()
        };
        assert_eq!(
            (first.mapping.axis, second.mapping.axis),
            (Axis::Vertical, Axis::Horizontal)
        );
    }

    #[test]
    fn degenerate_scales_do_not_overflow_predecimation() {
        let mapping = |crop_len| AxisMapping {
            axis: Axis::Horizontal,
            crop_offset: 0.0,
            crop_len,
            dst_len: 10,
        };
        assert_eq!(
            mapping(f32::INFINITY).predecimate_levels(),
            MAX_PREDECIMATE_LEVELS
        );
        assert_eq!(mapping(f32::NAN).predecimate_levels(), 0);
    }
}
