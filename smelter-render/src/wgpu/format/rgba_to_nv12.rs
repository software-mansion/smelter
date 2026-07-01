use crate::wgpu::{
    common_pipeline::{PRIMITIVE_STATE, Sampler, Vertex},
    texture::NV12Texture,
};

use super::WgpuCtx;

#[derive(Debug)]
pub struct RgbaToNv12Converter {
    y_pipeline: wgpu::RenderPipeline,
    uv_pipeline: wgpu::RenderPipeline,
    vertical_lanczos_y_pipeline: wgpu::RenderPipeline,
    vertical_lanczos_uv_pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl RgbaToNv12Converter {
    pub fn new(
        device: &wgpu::Device,
        single_texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("rgba_to_nv12.wgsl"));

        let sampler = Sampler::new(device);

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("RGBA to NV12 color converter render pipeline layout"),
                bind_group_layouts: &[
                    Some(single_texture_bind_group_layout),
                    Some(&sampler.bind_group_layout),
                ],
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
        let vertical_lanczos_y_pipeline = create_converting_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            "fs_lanczos_vertical_y",
            wgpu::TextureFormat::R8Unorm,
        );
        let vertical_lanczos_uv_pipeline = create_converting_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            "fs_lanczos_vertical_uv",
            wgpu::TextureFormat::Rg8Unorm,
        );

        Self {
            y_pipeline,
            uv_pipeline,
            vertical_lanczos_y_pipeline,
            vertical_lanczos_uv_pipeline,
            sampler,
        }
    }

    pub fn encode_convert(
        &self,
        ctx: &WgpuCtx,
        encoder: &mut wgpu::CommandEncoder,
        src_bg: &wgpu::BindGroup,
        dst_texture: &NV12Texture,
    ) {
        let (dst_y_view, dst_uv_view) = dst_texture.views();
        convert_plane(
            ctx,
            encoder,
            &self.y_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_y_view,
        );
        convert_plane(
            ctx,
            encoder,
            &self.uv_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_uv_view,
        );
    }

    /// Convert into a coded-size NV12 dma-buf target, rendering the composited
    /// content only into the top-left `visible` region (via a per-plane viewport)
    /// and clearing the surrounding coded padding to the encoder's padding luma /
    /// chroma. Used by the Quick Sync zero-copy path.
    pub fn encode_convert_external(
        &self,
        ctx: &WgpuCtx,
        encoder: &mut wgpu::CommandEncoder,
        src_bg: &wgpu::BindGroup,
        dst_texture: &NV12Texture,
        visible: crate::Resolution,
        padding_luma: f64,
        padding_chroma: f64,
    ) {
        let (dst_y_view, dst_uv_view) = dst_texture.views();
        convert_plane_viewport(
            ctx,
            encoder,
            &self.y_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_y_view,
            (visible.width as f32, visible.height as f32),
            wgpu::Color { r: padding_luma, g: 0.0, b: 0.0, a: 1.0 },
        );
        convert_plane_viewport(
            ctx,
            encoder,
            &self.uv_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_uv_view,
            ((visible.width / 2) as f32, (visible.height / 2) as f32),
            wgpu::Color { r: padding_chroma, g: padding_chroma, b: 0.0, a: 1.0 },
        );
    }

    pub fn encode_lanczos_vertical_convert(
        &self,
        ctx: &WgpuCtx,
        encoder: &mut wgpu::CommandEncoder,
        src_bg: &wgpu::BindGroup,
        dst_texture: &NV12Texture,
    ) {
        let (dst_y_view, dst_uv_view) = dst_texture.views();
        convert_plane(
            ctx,
            encoder,
            &self.vertical_lanczos_y_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_y_view,
        );
        convert_plane(
            ctx,
            encoder,
            &self.vertical_lanczos_uv_pipeline,
            &self.sampler.bind_group,
            src_bg,
            dst_uv_view,
        );
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
            buffers: &[Some(Vertex::LAYOUT)],
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

#[allow(clippy::too_many_arguments)]
fn convert_plane_viewport(
    ctx: &WgpuCtx,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    sampler_bg: &wgpu::BindGroup,
    src_bg: &wgpu::BindGroup,
    dst_view: &wgpu::TextureView,
    viewport: (f32, f32),
    clear: wgpu::Color,
) {
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("rgba to nv12 external viewport pass"),
        timestamp_writes: None,
        occlusion_query_set: None,
        depth_stencil_attachment: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: dst_view,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
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
    render_pass.set_viewport(0.0, 0.0, viewport.0, viewport.1, 0.0, 1.0);

    ctx.plane.draw(&mut render_pass);
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
