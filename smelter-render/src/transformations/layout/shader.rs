use std::sync::Arc;

use tracing::error;

use crate::{
    Resolution,
    state::node_texture::{NodeTexture, NodeTextureState},
    wgpu::{
        WgpuCtx, WgpuErrorScope,
        common_pipeline::{self, CreateShaderError, Sampler},
    },
};

use super::{RenderLayout, params::ParamsBindGroups};

const LABEL: Option<&str> = Some("layout node");

#[derive(Debug)]
pub struct LayoutShader {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
    params_bind_groups: ParamsBindGroups,
}

impl LayoutShader {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let scope = WgpuErrorScope::push(&wgpu_ctx.device);

        let shader_module = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("./apply_layouts.wgsl"));
        let result = Self::new_pipeline(wgpu_ctx, shader_module)?;

        scope.pop(&wgpu_ctx.device)?;

        Ok(result)
    }

    fn new_pipeline(
        wgpu_ctx: &Arc<WgpuCtx>,
        shader_module: wgpu::ShaderModule,
    ) -> Result<Self, CreateShaderError> {
        let sampler = Sampler::new(&wgpu_ctx.device);
        let params_bind_groups = ParamsBindGroups::new(wgpu_ctx);

        let pipeline_layout =
            wgpu_ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: LABEL,
                    bind_group_layouts: &[
                        &wgpu_ctx.format.single_texture_layout,
                        &params_bind_groups.bind_group_1_layout,
                        &params_bind_groups.bind_group_2_layout,
                        &sampler.bind_group_layout,
                    ],
                    push_constant_ranges: &[wgpu::PushConstantRange {
                        stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        range: 0..16,
                    }],
                });

        let pipeline = common_pipeline::create_render_pipeline(
            "Layout node",
            &wgpu_ctx.device,
            &pipeline_layout,
            &shader_module,
            wgpu_ctx.default_view_format(),
        );

        Ok(Self {
            pipeline,
            sampler,
            params_bind_groups,
        })
    }

    pub fn render(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        output_resolution: Resolution,
        layouts: Vec<RenderLayout>,
        textures: &[Option<&NodeTexture>],
        target: &NodeTextureState,
    ) {
        let layout_infos = self
            .params_bind_groups
            .update(wgpu_ctx, output_resolution, layouts);
        let input_texture_bgs: Vec<wgpu::BindGroup> = self.input_textures_bg(wgpu_ctx, textures);

        if layout_infos.len() != input_texture_bgs.len() {
            error!(
                "Layout infos len ({:?}) and textures bind groups count ({:?}) mismatch",
                layout_infos.len(),
                input_texture_bgs.len()
            );
        }

        let mut encoder = wgpu_ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: LABEL });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: LABEL,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    view: target.view(),
                    resolve_target: None,
                })],
                // TODO: depth stencil attachments
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            for (index, (texture_bg, layout_info)) in input_texture_bgs
                .iter()
                .zip(layout_infos.iter())
                .take(100)
                .enumerate()
            {
                render_pass.set_pipeline(&self.pipeline);

                render_pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    &layout_info.to_bytes(),
                );

                render_pass.set_bind_group(0, texture_bg, &[]);
                render_pass.set_bind_group(1, &self.params_bind_groups.bind_group_1, &[]);
                render_pass.set_bind_group(2, &self.params_bind_groups.bind_groups_2[index].0, &[]);
                render_pass.set_bind_group(3, &self.sampler.bind_group, &[]);

                wgpu_ctx.plane.draw(&mut render_pass);
            }
        }
        wgpu_ctx.queue.submit(Some(encoder.finish()));
    }

    fn input_textures_bg(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        textures: &[Option<&NodeTexture>],
    ) -> Vec<wgpu::BindGroup> {
        textures
            .iter()
            .map(|texture| {
                texture
                    .and_then(|texture| texture.state())
                    .map(|state| state.view())
                    .unwrap_or(wgpu_ctx.default_empty_view())
            })
            .map(|view| {
                wgpu_ctx
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &wgpu_ctx.format.single_texture_layout,
                        label: LABEL,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        }],
                    })
            })
            .collect()
    }
}
