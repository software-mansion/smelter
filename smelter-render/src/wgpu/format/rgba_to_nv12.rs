use crate::wgpu::{
    common_pipeline::{PRIMITIVE_STATE, Sampler, Vertex},
    texture::NV12Texture,
};

use super::WgpuCtx;

#[derive(Debug)]
pub struct RgbaToNv12Converter {
    y_pipeline: wgpu::RenderPipeline,
    uv_pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl RgbaToNv12Converter {
    pub fn new(
        device: &wgpu::Device,
        single_texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("rgba_to_nv12.wgsl"));

        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RGBA to NV12 color converter render pipeline layout"),
            bind_group_layouts: &[single_texture_bind_group_layout, &sampler.bind_group_layout],
            immediate_size: 0,
        });
        let y_pipeline = create_converting_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            "fs_main_y",
            wgpu::TextureFormat::R8Unorm,
        );
        let uv_pipeline = create_converting_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            "fs_main_uv",
            wgpu::TextureFormat::Rg8Unorm,
        );

        Self {
            y_pipeline,
            uv_pipeline,
            sampler,
        }
    }

    pub fn convert(&self, ctx: &WgpuCtx, src_bg: &wgpu::BindGroup, dst_texture: &NV12Texture) {
        let (dst_y_view, dst_uv_view) = dst_texture.views();

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RGBA to NV12 color converter encoder"),
            });

        convert_plane(
            ctx,
            &mut encoder,
            &self.y_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_y_view,
        );
        convert_plane(
            ctx,
            &mut encoder,
            &self.uv_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_uv_view,
        );

        ctx.queue.submit(Some(encoder.finish()));
    }
}

fn create_converting_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fragment_entry_point: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("RGBA to NV12 color converter render pipeline"),
        layout: Some(pipeline_layout),
        cache: None,
        vertex: wgpu::VertexState {
            module: shader,
            buffers: &[Vertex::LAYOUT],
            entry_point: None,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry_point),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                blend: None,
                format,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: PRIMITIVE_STATE,
        multiview_mask: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        depth_stencil: None,
    })
}

fn convert_plane(
    ctx: &WgpuCtx,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    sampler_bg: &wgpu::BindGroup,
    src_bg: &wgpu::BindGroup,
    dst_view: &wgpu::TextureView,
) {
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("wgpu render pass"),
        timestamp_writes: None,
        occlusion_query_set: None,
        depth_stencil_attachment: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: dst_view,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
            resolve_target: None,
            depth_slice: None,
        })],
        multiview_mask: None,
    });

    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, src_bg, &[]);
    render_pass.set_bind_group(1, sampler_bg, &[]);

    ctx.plane.draw(&mut render_pass);
}
