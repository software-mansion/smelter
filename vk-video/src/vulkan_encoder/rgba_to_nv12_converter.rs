pub struct RgbaNV12Converter {
    y_plane_renderer: PlaneRenderer,
    uv_plane_renderer: PlaneRenderer,

    rgba_view_bgl: wgpu::BindGroupLayout,
    sampler_bg: wgpu::BindGroup,

    _sampler: wgpu::Sampler,
    device: wgpu::Device,
}

impl RgbaNV12Converter {
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
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
            _sampler: sampler,
            device: device.clone(),
        }
    }

    pub fn create_state(
        &mut self,
        rgba_view: &wgpu::TextureView,
    ) -> Result<ConverterState, RgbaNV12ConverterError> {
        let input_format = rgba_view.texture().format();
        if input_format.remove_srgb_suffix() != wgpu::TextureFormat::Rgba8Unorm {
            return Err(RgbaNV12ConverterError::ExpectedRgbaTextureView {
                received: input_format,
            });
        }

        let nv12_texture = NV12Texture::new(&self.device, rgba_view.texture().size());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.rgba_view_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(rgba_view),
            }],
        });

        Ok(ConverterState {
            input_rgba_bg: bg,
            output_nv12: nv12_texture,
        })
    }

    pub fn convert(&self, command_encoder: &mut wgpu::CommandEncoder, state: &ConverterState) {
        self.y_plane_renderer.draw(
            command_encoder,
            &state.output_nv12.y_plane_view,
            &self.sampler_bg,
            &state.input_rgba_bg,
        );
        self.uv_plane_renderer.draw(
            command_encoder,
            &state.output_nv12.uv_plane_view,
            &self.sampler_bg,
            &state.input_rgba_bg,
        );

        command_encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: state.output_nv12(),
                state: wgpu::TextureUses::COPY_SRC,
                selector: None,
            }]
            .into_iter(),
        );
    }
}

#[derive(Clone)]
pub struct ConverterState {
    input_rgba_bg: wgpu::BindGroup,
    output_nv12: NV12Texture,
}

impl ConverterState {
    pub fn output_nv12(&self) -> &wgpu::Texture {
        &self.output_nv12.texture
    }
}

#[derive(Clone)]
struct NV12Texture {
    texture: wgpu::Texture,
    y_plane_view: wgpu::TextureView,
    uv_plane_view: wgpu::TextureView,
}

impl NV12Texture {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vk-video nv12 converter texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::NV12,
            // TODO: Should this be configurable or do we expect that this texture will be directly
            // used by the encoder after conversion?
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let y_plane_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vk-video nv12 converter y plane view"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_plane_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vk-video nv12 converter uv plane view"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        Self {
            texture,
            y_plane_view,
            uv_plane_view,
        }
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
pub enum RgbaNV12ConverterError {
    #[error("Expected Rgba8 texture view format, instead of received {received:?}")]
    ExpectedRgbaTextureView { received: wgpu::TextureFormat },
}
