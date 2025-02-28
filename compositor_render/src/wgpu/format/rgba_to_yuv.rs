use bytemuck::bytes_of;
use tracing::info;
use wgpu::util::DeviceExt;

use crate::wgpu::{
    common_pipeline::{Sampler, Vertex, PRIMITIVE_STATE},
    texture::{PlanarYuvTextures, RGBATexture},
};

use super::WgpuCtx;

#[derive(Debug)]
pub struct RgbaToYuvConverter {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
    uniform_bgl: wgpu::BindGroupLayout,
}

pub struct PlaneUniform {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl PlaneUniform {
    pub fn new(wgpu_ctx: &WgpuCtx, uniform_bgl: &wgpu::BindGroupLayout, plane: u32) -> Self {
        let buffer = wgpu_ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("PlaneUniform"),
                contents: &(plane as u32).to_le_bytes(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = wgpu_ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("PlaneUniform bind group"),
                layout: uniform_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });

        Self { buffer, bind_group }
    }
}

impl RgbaToYuvConverter {
    pub fn new(
        device: &wgpu::Device,
        single_texture_bind_group_layout: &wgpu::BindGroupLayout,
        single_uniform_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RGBA to YUV color converter pipeline layout"),
            bind_group_layouts: &[
                single_texture_bind_group_layout,
                &sampler.bind_group_layout,
                single_uniform_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let shader_module = device.create_shader_module(wgpu::include_wgsl!("rgba_to_yuv.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RGBA to YUV color converter pipeline"),
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

        Self {
            pipeline,
            sampler,
            uniform_bgl: single_uniform_bind_group_layout.clone(),
        }
    }

    pub fn convert(
        &self,
        ctx: &WgpuCtx,
        src: (&RGBATexture, &wgpu::BindGroup),
        dst: &PlanarYuvTextures,
    ) {
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RGBA to YUV color converter command encoder"),
            });

        // TODO(noituri): This should be created only once
        let plane_uniforms = &[
            PlaneUniform::new(ctx, &self.uniform_bgl, 0),
            PlaneUniform::new(ctx, &self.uniform_bgl, 1),
            PlaneUniform::new(ctx, &self.uniform_bgl, 2),
        ];
        for plane in [0, 1, 2] {
            //plane_uniform.update(ctx, plane);
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("YUV to RGBA color converter render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        // We want the background to be black. Black in YUV is y = 0, u = 0.5, v = 0.5
                        // Therefore, we set the clear color to 0, 0, 0 when drawing the y plane
                        // and to 0.5, 0.5, 0.5 when drawing the u and v planes.
                        load: wgpu::LoadOp::Clear(if plane == 0 {
                            wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    view: &dst.plane(plane as usize).view,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, src.1, &[]);
            render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);
            render_pass.set_bind_group(2, &plane_uniforms[plane].bind_group, &[]);
            ctx.plane.draw(&mut render_pass);
        }
        ctx.queue.submit(Some(encoder.finish()));
    }
}
