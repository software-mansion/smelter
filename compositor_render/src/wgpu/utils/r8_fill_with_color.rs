use wgpu::ShaderStages;

use crate::wgpu::{
    common_pipeline::{Vertex, PRIMITIVE_STATE},
    texture::Texture,
    WgpuCtx,
};

#[derive(Debug)]
pub struct R8FillWithValue {
    pipeline: wgpu::RenderPipeline,
    value_buffer: wgpu::Buffer,
    value_bind_group: wgpu::BindGroup,
}

impl R8FillWithValue {
    pub fn new(device: &wgpu::Device, single_uniform_bgl: &wgpu::BindGroupLayout) -> Self {
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("r8_fill_value.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fill with value render pipeline layout"),
            bind_group_layouts: &[single_uniform_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..4,
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fill with value pipeline"),
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
                    format: wgpu::TextureFormat::R8Unorm,
                    write_mask: wgpu::ColorWrites::all(),
                    blend: None,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let value_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill with value buffer"),
            size: std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let value_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fill with value bind group"),
            layout: single_uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: value_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            value_buffer,
            value_bind_group,
        }
    }

    pub fn fill(&self, ctx: &WgpuCtx, dst: &Texture, value: f32) {
        ctx.queue.write_buffer(&self.value_buffer, 0, bytemuck::bytes_of(&value));
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Fill R8 texture with value command encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Fill R8 texture with value render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    view: &dst.view,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.value_bind_group, &[]);
            ctx.plane.draw(&mut render_pass);
        }

        ctx.queue.submit(Some(encoder.finish()));
    }
}
