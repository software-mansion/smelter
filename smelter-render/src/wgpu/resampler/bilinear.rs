use crate::wgpu::{
    WgpuCtx,
    common_pipeline::{PRIMITIVE_STATE, Sampler, Vertex},
};

const LABEL: Option<&str> = Some("Bilinear resampler");

/// Single-pass resampler using hardware bilinear filtering of the whole source.
#[derive(Debug)]
pub struct BilinearResampler {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
}

impl BilinearResampler {
    pub fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        dst_format: wgpu::TextureFormat,
    ) -> Self {
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("bilinear.wgsl"));
        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: LABEL,
            bind_group_layouts: &[Some(texture_layout), Some(&sampler.bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: LABEL,
            layout: Some(&pipeline_layout),
            primitive: PRIMITIVE_STATE,

            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },

            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: dst_format,
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
            multiview_mask: None,
            depth_stencil: None,
            cache: None,
        });

        Self { pipeline, sampler }
    }

    /// `src` MUST be a view whose format matches the source storage, so sRGB textures auto-decode
    /// on sample and filtering runs in linear space. `dst` must match the `dst_format` this
    /// resampler was built with; sRGB targets re-encode linear shader values on write.
    pub fn resample(
        &self,
        ctx: &WgpuCtx,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let src_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: LABEL,
            layout: &ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
            }],
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: LABEL,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                view: dst,
                resolve_target: None,
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &src_bg, &[]);
        render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);

        ctx.plane.draw(&mut render_pass);
    }
}
