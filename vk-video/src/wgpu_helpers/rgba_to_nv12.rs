use std::marker::PhantomData;

use crate::{Nv12Texture, RgbaTexture, WgpuTextureMapping};

/// Helper that lets you convert RGBA [`wgpu::Texture`] into NV12 [`wgpu::Texture`].
/// Use [`WgpuRgbaToNv12Converter::create_mapping`] to create [`WgpuTextureMapping`] which represents
/// converter's input and output.
pub struct WgpuRgbaToNv12Converter {
    y_plane_renderer: PlaneRenderer,
    uv_plane_renderer: PlaneRenderer,

    rgba_view_bgl: wgpu::BindGroupLayout,
    sampler_bg: wgpu::BindGroup,

    device: wgpu::Device,
}

impl WgpuRgbaToNv12Converter {
    pub fn new(device: &wgpu::Device) -> Self {
        let rgba_view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let sampler_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        });
        let sampler_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &sampler_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/rgba_to_nv12.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vk-video rgba to nv12 converter pipeline layout"),
            bind_group_layouts: &[&rgba_view_bgl, &sampler_bgl],
            immediate_size: 0,
        });

        let y_plane_renderer = PlaneRenderer::new(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureAspect::Plane0,
        );
        let uv_plane_renderer = PlaneRenderer::new(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureAspect::Plane1,
        );

        Self {
            y_plane_renderer,
            uv_plane_renderer,
            rgba_view_bgl,
            sampler_bg,
            device: device.clone(),
        }
    }

    pub fn create_mapping(
        &self,
        rgba_texture: &wgpu::Texture,
    ) -> Result<WgpuTextureMapping<RgbaTexture, Nv12Texture>, RgbaToNv12ConverterError> {
        if rgba_texture.format().remove_srgb_suffix() != wgpu::TextureFormat::Rgba8Unorm {
            return Err(RgbaToNv12ConverterError::ExpectedRgbaTextureView {
                received: rgba_texture.format(),
            });
        }
        if !rgba_texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(RgbaToNv12ConverterError::ExpectedTextureBindingUsage);
        }

        let rgba_view = rgba_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let input_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.rgba_view_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&rgba_view),
            }],
        });
        let output_texture = Nv12Texture::new(&self.device, rgba_texture.size());

        Ok(WgpuTextureMapping {
            input_bg,
            _input_texture: PhantomData,
            output_texture,
        })
    }

    /// Converts RGBA texture into NV12 texture. Input and output textures are taken from the `mapping`
    pub fn convert(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        mapping: &WgpuTextureMapping<RgbaTexture, Nv12Texture>,
    ) {
        self.y_plane_renderer.draw(
            command_encoder,
            &mapping.output_texture.y_plane_view,
            &self.sampler_bg,
            &mapping.input_bg,
        );
        self.uv_plane_renderer.draw(
            command_encoder,
            &mapping.output_texture.uv_plane_view,
            &self.sampler_bg,
            &mapping.input_bg,
        );

        command_encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: &mapping.output().texture,
                state: wgpu::TextureUses::COPY_SRC,
                selector: None,
            }]
            .into_iter(),
        );
    }
}

struct PlaneRenderer {
    pipeline: wgpu::RenderPipeline,
}

impl PlaneRenderer {
    fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader_module: &wgpu::ShaderModule,
        plane: wgpu::TextureAspect,
    ) -> Self {
        let (format, fragment_entry_point) = match plane {
            wgpu::TextureAspect::Plane0 => (wgpu::TextureFormat::R8Unorm, "fs_main_y"),
            wgpu::TextureAspect::Plane1 => (wgpu::TextureFormat::Rg8Unorm, "fs_main_uv"),
            aspect => unreachable!("Not a NV12 plane: {aspect:?}"),
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vk-video nv12 plane renderer"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader_module,
                entry_point: Some(fragment_entry_point),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline }
    }

    fn draw(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        plane_view: &wgpu::TextureView,
        sampler_bg: &wgpu::BindGroup,
        texture_bg: &wgpu::BindGroup,
    ) {
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: plane_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_bind_group(0, texture_bg, &[]);
        render_pass.set_bind_group(1, sampler_bg, &[]);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RgbaToNv12ConverterError {
    #[error("Expected Rgba8 texture view format for the input texture, but received {received:?}")]
    ExpectedRgbaTextureView { received: wgpu::TextureFormat },

    #[error("Expected TEXTURE_BINDING usage for the input texture")]
    ExpectedTextureBindingUsage,
}
