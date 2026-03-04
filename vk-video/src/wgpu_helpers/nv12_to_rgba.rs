use crate::wgpu_helpers::WgpuSampler;

/// Helper that lets you convert NV12 [`wgpu::Texture`] into RGBA [`wgpu::Texture`].
/// Use [`WgpuNv12ToRgbaConverter::create_input_bind_group`] to create [`wgpu::BindGroup`] which represents
/// NV12 bind group acceptable by the converter.
pub struct WgpuNv12ToRgbaConverter {
    pipeline: wgpu::RenderPipeline,

    nv12_planes_bgl: wgpu::BindGroupLayout,
    sampler: WgpuSampler,

    device: wgpu::Device,
}

impl WgpuNv12ToRgbaConverter {
    pub fn new(device: &wgpu::Device) -> Self {
        let nv12_planes_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let sampler = WgpuSampler::new(device);
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/nv12_to_rgba.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vk-video nv12 to rgba converter pipeline layout"),
            bind_group_layouts: &[&nv12_planes_bgl, &sampler.bgl],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vk-video nv12 to rgba converter pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            nv12_planes_bgl,
            sampler,
            device: device.clone(),
        }
    }

    /// Creates [`wgpu::BindGroup`] for NV12 [`wgpu::Texture`].
    /// The texture's usage must contain [`wgpu::TextureUsages::TEXTURE_BINDING`].
    pub fn create_input_bind_group(&self, nv12_texture: &wgpu::Texture) -> wgpu::BindGroup {
        let y_plane_view = nv12_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vk-video nv12 to rgba converter y plane view"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_plane_view = nv12_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vk-video nv12 to rgba converter uv plane view"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.nv12_planes_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_plane_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_plane_view),
                },
            ],
        })
    }

    /// Converts NV12 texture into RGBA texture.
    /// RGBA texture's usage must contain [`wgpu::TextureUsages::RENDER_ATTACHMENT`].
    pub fn convert(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        src_nv12_bing_group: &wgpu::BindGroup,
        dst_rgba_view: &wgpu::TextureView,
    ) {
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_rgba_view,
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

        render_pass.set_bind_group(0, src_nv12_bing_group, &[]);
        render_pass.set_bind_group(1, &self.sampler.bg, &[]);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}
