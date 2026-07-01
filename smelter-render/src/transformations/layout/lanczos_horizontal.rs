use std::sync::Arc;

use crate::{
    Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{
        WgpuCtx, WgpuErrorScope,
        common_pipeline::{self, CreateShaderError},
        common_pipeline::Sampler,
    },
};

const LABEL: Option<&str> = Some("Lanczos3 horizontal prefilter");

pub(super) struct LanczosHorizontalShader {
    pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    sampler: Sampler,
}

impl LanczosHorizontalShader {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let scope = WgpuErrorScope::push(&wgpu_ctx.device);
        let shader = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("lanczos_horizontal.wgsl"));
        let result = Self::new_pipeline(wgpu_ctx, shader)?;
        scope.pop()?;
        Ok(result)
    }

    fn new_pipeline(
        wgpu_ctx: &Arc<WgpuCtx>,
        shader: wgpu::ShaderModule,
    ) -> Result<Self, CreateShaderError> {
        let params_buffer = wgpu_ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: LABEL,
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let params_layout =
            wgpu_ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: LABEL,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let params_bind_group =
            wgpu_ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: LABEL,
                layout: &params_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                }],
            });
        let sampler = Sampler::new(&wgpu_ctx.device);
        let pipeline_layout =
            wgpu_ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: LABEL,
                bind_group_layouts: &[
                    Some(&wgpu_ctx.format.single_texture_layout),
                    Some(&params_layout),
                    Some(&sampler.bind_group_layout),
                ],
                immediate_size: 0,
            });
        let pipeline = common_pipeline::create_render_pipeline(
            "Lanczos3 horizontal prefilter",
            &wgpu_ctx.device,
            &pipeline_layout,
            &shader,
            wgpu_ctx.default_view_format(),
        );

        Ok(Self { pipeline, params_buffer, params_bind_group, sampler })
    }

    pub fn render(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        source: &NodeTextureState,
        source_resolution: Resolution,
        target: &NodeTextureState,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut params = [0u8; 16];
        params[0..4].copy_from_slice(&(source_resolution.width as f32).to_le_bytes());
        wgpu_ctx.queue.write_buffer(&self.params_buffer, 0, &params);

        let source_bg = wgpu_ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: LABEL,
            layout: &wgpu_ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source.view()),
            }],
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: LABEL,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                view: target.view(),
                resolve_target: None,
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &source_bg, &[]);
        render_pass.set_bind_group(1, &self.params_bind_group, &[]);
        render_pass.set_bind_group(2, &self.sampler.bind_group, &[]);
        wgpu_ctx.plane.draw(&mut render_pass);
    }
}
