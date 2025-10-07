use crate::wgpu::common_pipeline::{PRIMITIVE_STATE, Sampler, Vertex};

use super::WgpuCtx;

#[derive(Debug)]
pub struct RemovePremultipliedAlphaPipeline {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl RemovePremultipliedAlphaPipeline {
    pub fn new(
        device: &wgpu::Device,
        rgba_textures_bind_group_layout: &wgpu::BindGroupLayout,
        dst_view_format: wgpu::TextureFormat,
    ) -> Self {
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("remove_premultiplied_alpha.wgsl"));
        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Remove pre-multiplied alpha pipeline layout"),
            bind_group_layouts: &[rgba_textures_bind_group_layout, &sampler.bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Remove pre-multiplied alpha render pipeline"),
            layout: Some(&pipeline_layout),
            primitive: PRIMITIVE_STATE,

            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::LAYOUT],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },

            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: dst_view_format,
                    write_mask: wgpu::ColorWrites::all(),
                    blend: Some(wgpu::BlendState::REPLACE),
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),

            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            depth_stencil: None,
            cache: None,
        });

        Self { pipeline, sampler }
    }

    pub fn render(&self, ctx: &WgpuCtx, src_bg: &wgpu::BindGroup, dst_view: &wgpu::TextureView) {
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Remove pre-multiplied alpha encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Remove pre-multiplied alpha render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    view: dst_view,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, src_bg, &[]);
            render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);

            ctx.plane.draw(&mut render_pass);
        }

        ctx.queue.submit(Some(encoder.finish()));
    }
}
