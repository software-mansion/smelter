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

/// Separable Lanczos3 resampler: the kernel scales with the ratio, so one pair
/// of passes covers any up/downscale without mip pyramids. Everything before the
/// final pass runs in linear Rgba16Float, since a unorm intermediate would clamp
/// the kernel's negative lobes (~17 dB PSNR loss on upscaled content).
pub struct ResamplerShader {
    pipeline: wgpu::RenderPipeline,
    pipeline_f16: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
}

#[derive(Debug, Clone, Copy)]
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
        (self.scale() / KERNEL_BUDGET).log2().ceil().max(0.0) as u32
    }

    fn on_reduced_source(self, levels: u32) -> AxisMapping {
        let factor = (1u32 << levels) as f32;
        AxisMapping {
            crop_offset: self.crop_offset / factor,
            crop_len: self.crop_len / factor,
            ..self
        }
    }

    /// Output maps 1:1 onto source, so the layout shader already samples exactly.
    fn is_identity(&self) -> bool {
        is_same_px(self.crop_len, self.dst_len as f32)
            && is_same_px(self.crop_offset, self.crop_offset.round())
    }

    fn to_immediates(self) -> [u8; 12] {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&(self.axis as u32).to_le_bytes());
        data[4..8].copy_from_slice(&self.scale().to_le_bytes());
        data[8..12].copy_from_slice(&self.crop_offset.to_le_bytes());
        data
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
        let resample_layout = make_layout(12);
        let build = |format| {
            common_pipeline::create_render_pipeline(
                "Lanczos3 resampler",
                &wgpu_ctx.device,
                &resample_layout,
                &resample,
                format,
            )
        };

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
            pipeline: build(wgpu_ctx.default_view_format()),
            pipeline_f16: build(wgpu::TextureFormat::Rgba16Float),
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
        !axis_mappings(crop, dst)
            .into_iter()
            .all(|mapping| mapping.is_identity())
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
            (source.view(), source.resolution())
        } else {
            let full = source.resolution();
            let reduced = Resolution {
                width: full.width / factor[0] as usize,
                height: full.height / factor[1] as usize,
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

        // Stronger shrink first, to minimize the intermediate size and tap count.
        let mut passes: Vec<AxisMapping> = mappings
            .iter()
            .zip(levels)
            .map(|(mapping, levels)| mapping.on_reduced_source(levels))
            .filter(|mapping| !mapping.is_identity())
            .collect();
        passes.sort_by(|a, b| b.scale().total_cmp(&a.scale()));

        match passes.as_slice() {
            [only] => {
                self.intermediate = None;
                let target = self.output.ensure_size(wgpu_ctx, dst).view();
                let bytes = only.to_immediates();
                shader.pass(
                    wgpu_ctx,
                    &shader.pipeline,
                    src_view,
                    &bytes,
                    target,
                    encoder,
                );
            }
            [first, second] => {
                let size = match first.axis {
                    Axis::Horizontal => Resolution {
                        width: first.dst_len,
                        height: src_res.height,
                    },
                    Axis::Vertical => Resolution {
                        width: src_res.width,
                        height: first.dst_len,
                    },
                };
                let mid = Intermediate::ensure(&mut self.intermediate, wgpu_ctx, size);
                let bytes = first.to_immediates();
                shader.pass(
                    wgpu_ctx,
                    &shader.pipeline_f16,
                    src_view,
                    &bytes,
                    mid,
                    encoder,
                );
                let target = self.output.ensure_size(wgpu_ctx, dst).view();
                let bytes = second.to_immediates();
                shader.pass(wgpu_ctx, &shader.pipeline, mid, &bytes, target, encoder);
            }
            _ => {}
        }
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
