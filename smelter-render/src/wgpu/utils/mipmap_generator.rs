use crate::wgpu::common_pipeline::{PRIMITIVE_STATE, Sampler, Vertex};

use super::WgpuCtx;

#[derive(Debug)]
pub struct MippedTexture {
    #[allow(dead_code)]
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    #[allow(dead_code)]
    pub mip_count: u32,
}

#[derive(Debug)]
pub struct MipMapGenerator {
    pipeline_unorm: wgpu::RenderPipeline,
    pipeline_srgb: wgpu::RenderPipeline,
    sampler: Sampler,
    single_texture_layout: wgpu::BindGroupLayout,
}

impl MipMapGenerator {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("mipmap_blit.wgsl"));

        let single_texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("MipMapGenerator single texture layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                }],
            });

        let sampler = Sampler::new(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MipMapGenerator pipeline layout"),
            bind_group_layouts: &[
                Some(&single_texture_layout),
                Some(&sampler.bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline_unorm = Self::create_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let pipeline_srgb = Self::create_pipeline(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        Self {
            pipeline_unorm,
            pipeline_srgb,
            sampler,
            single_texture_layout,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader_module: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MipMapGenerator render pipeline"),
            layout: Some(pipeline_layout),
            primitive: PRIMITIVE_STATE,
            vertex: wgpu::VertexState {
                module: shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::LAYOUT],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
            multiview_mask: None,
            depth_stencil: None,
            cache: None,
        })
    }

    /// Generate mipmaps for `source`. Encodes all blit passes into `encoder`.
    /// Returns a `MippedTexture` with a full mip chain.
    pub fn generate(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &WgpuCtx,
        source: &wgpu::Texture,
        format: wgpu::TextureFormat,
        max_levels: u32,
    ) -> MippedTexture {
        let w = source.width();
        let h = source.height();
        let max_possible = (w.max(h) as f32).log2().floor() as u32 + 1;
        let mip_count = max_levels.clamp(1, max_possible);

        let mipped_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MipMapGenerator mipped texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Copy source mip 0 → mipped mip 0
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &mipped_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let pipeline = if format == wgpu::TextureFormat::Rgba8UnormSrgb {
            &self.pipeline_srgb
        } else {
            &self.pipeline_unorm
        };

        for level in 1..mip_count {
            let src_view = mipped_texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst_view = mipped_texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });

            let src_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("MipMapGenerator src bind group"),
                layout: &self.single_texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                }],
            });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("MipMapGenerator blit pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    view: &dst_view,
                    resolve_target: None,
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, &src_bg, &[]);
            render_pass.set_bind_group(1, &self.sampler.bind_group, &[]);
            ctx.plane.draw(&mut render_pass);
        }

        let view = mipped_texture.create_view(&wgpu::TextureViewDescriptor::default());

        MippedTexture {
            texture: mipped_texture,
            view,
            mip_count,
        }
    }
}
