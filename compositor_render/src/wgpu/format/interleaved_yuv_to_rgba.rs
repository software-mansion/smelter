use crate::wgpu::{
    common_pipeline::{Sampler, Vertex, PRIMITIVE_STATE},
    texture::{InterleavedYuv422Texture, RGBATexture},
};

use super::WgpuCtx;

#[derive(Debug)]
pub struct InterleavedYuv422ToRgbaConverter {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl InterleavedYuv422ToRgbaConverter {
    pub fn new(
        device: &wgpu::Device,
        yuv_textures_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("interleaved_yuv_to_rgba.wgsl"));
        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Interleaved YUV 4:2:2 to RGBA color converter render pipeline layout"),
            bind_group_layouts: &[yuv_textures_bind_group_layout, &sampler.bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Interleaved YUV 4:2:2 to RGBA color converter render pipeline"),
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    write_mask: wgpu::ColorWrites::all(),
                    blend: None,
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

    pub fn convert(
        &self,
        ctx: &WgpuCtx,
        src: (&InterleavedYuv422Texture, &wgpu::BindGroup),
        dst: &RGBATexture,
    ) {
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Interleaved YUV 4:2:2 to RGBA color converter encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Interleaved YUV 4:2:2 to RGBA color converter render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    view: &dst.texture().view,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, src.1, &[]);
            render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);

            ctx.plane.draw(&mut render_pass);
        }

        ctx.queue.submit(Some(encoder.finish()));
    }
}
