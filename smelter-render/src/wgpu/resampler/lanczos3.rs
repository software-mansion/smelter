use crate::{
    Resolution,
    wgpu::{WgpuCtx, common_pipeline},
};

const LABEL: Option<&str> = Some("Lanczos3 resampler");

/// When rendering we cut this fragment from texture and stretch it on
/// the expected position
#[derive(Debug, Clone, Copy)]
pub struct Crop {
    pub top: f32,
    pub left: f32,
    pub width: f32,
    pub height: f32,
}

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
#[derive(Debug)]
pub struct Lanczos3Resampler {
    pipeline: wgpu::RenderPipeline,
    pipeline_f16: wgpu::RenderPipeline,
    box_reduce: wgpu::RenderPipeline,
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

/// Intermediate textures reused across frames. Owned by each use site, since the resampler itself
/// is shared.
#[derive(Default)]
pub struct Lanczos3TextureCache {
    reduced: Option<Intermediate>,
    intermediate: Option<Intermediate>,
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

impl Lanczos3Resampler {
    pub fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        dst_format: wgpu::TextureFormat,
    ) -> Self {
        let make_layout = |immediate_size| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: LABEL,
                bind_group_layouts: &[Some(texture_layout)],
                immediate_size,
            })
        };

        let lanczos3 = device.create_shader_module(wgpu::include_wgsl!("lanczos3.wgsl"));
        let lanczos3_layout = make_layout(16);
        let build = |format| {
            common_pipeline::create_render_pipeline(
                "Lanczos3 resampler",
                device,
                &lanczos3_layout,
                &lanczos3,
                format,
            )
        };
        let pipeline = build(dst_format);
        let pipeline_f16 = build(wgpu::TextureFormat::Rgba16Float);

        let box_reduce_shader = device.create_shader_module(wgpu::include_wgsl!("box_reduce.wgsl"));
        let box_reduce = common_pipeline::create_render_pipeline(
            "Lanczos3 resampler (box reduce)",
            device,
            &make_layout(8),
            &box_reduce_shader,
            wgpu::TextureFormat::Rgba16Float,
        );

        Self {
            pipeline,
            pipeline_f16,
            box_reduce,
        }
    }

    /// `false` when `crop` maps 1:1 onto whole texels of `dst`, so the source can be sampled
    /// directly.
    pub fn needs_resample(crop: Crop, dst: Resolution) -> bool {
        plan_passes(axis_mappings(crop, dst)).is_some()
    }

    /// Resamples `crop` of `src` into the whole of `dst`. `src` MUST be a view whose format
    /// matches the source storage, so sRGB textures auto-decode on load and filtering runs in
    /// linear space. `dst` must match the `dst_format` this resampler was built with.
    pub fn resample(
        &self,
        ctx: &WgpuCtx,
        src: &wgpu::TextureView,
        crop: Crop,
        dst: &wgpu::TextureView,
        texture_cache: &mut Lanczos3TextureCache,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let src_res = Resolution::from(src.texture().size());
        let dst_res = Resolution::from(dst.texture().size());
        let mappings = axis_mappings(crop, dst_res);
        let levels = mappings.map(|m| m.predecimate_levels());
        let factor = levels.map(|l| 1u32 << l);

        // Box-reduce the ratio above the budget (a 2^k box == k cascaded 2:1 halvings, so one pass
        // suffices), then the kernel takes the residual.
        let (src, src_res) = if factor == [1, 1] {
            texture_cache.reduced = None;
            (src, src_res)
        } else {
            let reduced = Resolution {
                width: src_res.width.div_ceil(factor[0] as usize),
                height: src_res.height.div_ceil(factor[1] as usize),
            };
            let view = Intermediate::ensure(&mut texture_cache.reduced, ctx, reduced);
            let bytes = bytemuck::bytes_of(&factor);
            self.pass(ctx, &self.box_reduce, src, bytes, view, encoder);
            (view, reduced)
        };

        let residual = std::array::from_fn(|axis| mappings[axis].on_reduced_source(levels[axis]));
        let passes = plan_passes(residual)
            .expect("box reduction leaves a residual scale, axes stay off the direct path");

        // A second axis goes through the f16 intermediate before the last pass.
        let (view, last) = match passes {
            Passes::Single(pass) => {
                texture_cache.intermediate = None;
                (src, pass)
            }
            Passes::Separable { first, second } => {
                let mid = Intermediate::ensure(
                    &mut texture_cache.intermediate,
                    ctx,
                    first.mapping.output_size(src_res),
                );
                let immediates = first.to_immediates();
                self.pass(ctx, &self.pipeline_f16, src, &immediates, mid, encoder);
                (mid, second)
            }
        };
        self.pass(
            ctx,
            &self.pipeline,
            view,
            &last.to_immediates(),
            dst,
            encoder,
        );
    }

    fn pass(
        &self,
        wgpu_ctx: &WgpuCtx,
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

fn axis_mappings(crop: Crop, dst: Resolution) -> [AxisMapping; 2] {
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
        plan_passes(axis_mappings(crop, dst))
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
