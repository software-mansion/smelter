use wgpu::{util::DeviceExt, ShaderStages};

use crate::wgpu::{
    common_pipeline::{Sampler, Vertex, PRIMITIVE_STATE},
    texture::{PlanarYuvTextures, PlanarYuvVariant, RGBATexture},
};

use super::WgpuCtx;

#[derive(Debug)]
pub struct PlanarYuvToRgbaConverter {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
    uniform_bgl: wgpu::BindGroupLayout,
}

impl PlanarYuvToRgbaConverter {
    pub fn new(
        device: &wgpu::Device,
        yuv_textures_bind_group_layout: &wgpu::BindGroupLayout,
        single_uniform_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("planar_yuv_to_rgba.wgsl"));
        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Planar YUV 4:2:0 to RGBA color converter render pipeline layout"),
            bind_group_layouts: &[
                yuv_textures_bind_group_layout,
                &sampler.bind_group_layout,
                single_uniform_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Planar YUV 4:2:0 to RGBA color converter render pipeline"),
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

        Self {
            pipeline,
            sampler,
            uniform_bgl: single_uniform_bind_group_layout.clone(),
        }
    }

    pub fn convert(
        &self,
        ctx: &WgpuCtx,
        src: (&PlanarYuvTextures, &wgpu::BindGroup),
        dst: &RGBATexture,
    ) {
        let settings = YUVToRGBASettings::new(src.0.variant());
        let settings_uniform = SettingsUniform::new(&ctx.device, &self.uniform_bgl, settings);
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Planar YUV 4:2:0 to RGBA color converter encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Planar YUV 4:2:0 to RGBA color converter render pass"),
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
            render_pass.set_bind_group(2, &settings_uniform.bind_group, &[]);

            ctx.plane.draw(&mut render_pass);
        }

        ctx.queue.submit(Some(encoder.finish()));
    }
}

#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct YUVToRGBASettings {
    pixel_format: u32,
}

impl YUVToRGBASettings {
    fn new(variant: PlanarYuvVariant) -> Self {
        match variant {
            PlanarYuvVariant::YUV420 => Self { pixel_format: 0 },
            PlanarYuvVariant::YUVJ420 => Self { pixel_format: 1 },
        }
    }
}

struct SettingsUniform {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl SettingsUniform {
    fn new(
        device: &wgpu::Device,
        uniform_bgl: &wgpu::BindGroupLayout,
        settings: YUVToRGBASettings,
    ) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("YUV to RGBA settings uniform buffer"),
            contents: bytemuck::bytes_of(&settings),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("YUV to RGBA settings bind group"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self { buffer, bind_group }
    }
}
