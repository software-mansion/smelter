use std::sync::Arc;

use wgpu::ShaderStages;

use crate::{
    state::node_texture::NodeTextureState,
    wgpu::{
        common_pipeline::{self, CreateShaderError, Sampler},
        WgpuCtx, WgpuErrorScope,
    },
};

use super::embedder::RenderInfo;

#[derive(Debug)]
pub(super) struct WebRendererShader {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl WebRendererShader {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let scope = WgpuErrorScope::push(&wgpu_ctx.device);

        let shader_module = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("./render_website.wgsl"));
        let sampler = Sampler::new(&wgpu_ctx.device);

        let pipeline_layout =
            wgpu_ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Web renderer pipeline layout"),
                    bind_group_layouts: &[
                        &wgpu_ctx.format.single_texture_layout,
                        &sampler.bind_group_layout,
                    ],
                    push_constant_ranges: &[wgpu::PushConstantRange {
                        stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        range: 0..RenderInfo::size(),
                    }],
                });

        let pipeline = common_pipeline::create_render_pipeline(
            "Web renderer node",
            &wgpu_ctx.device,
            &pipeline_layout,
            &shader_module,
            wgpu_ctx.default_view_format(),
        );

        scope.pop(&wgpu_ctx.device)?;

        Ok(Self { pipeline, sampler })
    }

    pub(super) fn render(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        textures: &[(Option<&wgpu::TextureView>, RenderInfo)],
        target: &NodeTextureState,
    ) {
        let mut encoder = wgpu_ctx.device.create_command_encoder(&Default::default());

        let mut render_plane =
            |(texture_view, render_info): &(Option<&wgpu::TextureView>, RenderInfo),
             clear: bool| {
                let load = match clear {
                    true => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    false => wgpu::LoadOp::Load,
                };

                let texture_view = texture_view.unwrap_or(wgpu_ctx.default_empty_view());

                let input_texture_bg =
                    wgpu_ctx
                        .device
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("Web renderer input textures bgl"),
                            layout: &wgpu_ctx.format.single_texture_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(texture_view),
                            }],
                        });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                        view: target.view(),
                        resolve_target: None,
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(&self.pipeline);

                render_pass.set_push_constants(
                    ShaderStages::VERTEX_FRAGMENT,
                    0,
                    &render_info.bytes(),
                );
                render_pass.set_bind_group(0, &input_texture_bg, &[]);
                render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);

                wgpu_ctx.plane.draw(&mut render_pass);
            };

        for (id, render_texture) in textures.iter().enumerate() {
            render_plane(render_texture, id == 0);
        }

        wgpu_ctx.queue.submit(Some(encoder.finish()));
    }
}
