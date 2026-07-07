use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct Vertex {
    position: [f32; 3],
    texture_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBUTES: &[wgpu::VertexAttribute] =
        &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: Self::ATTRIBUTES,
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
    };
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, 1.0, 0.0],
        texture_coords: [0.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0, 0.0],
        texture_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0],
        texture_coords: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        texture_coords: [1.0, 0.0],
    },
];

const INDICES: &[u16] = &[0, 1, 3, 1, 2, 3];

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_configuration: wgpu::SurfaceConfiguration,
    sampler: wgpu::Sampler,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
}

impl Renderer {
    pub fn new(
        surface: wgpu::Surface<'static>,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        window: &Window,
    ) -> Self {
        let size = window.inner_size();
        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_texture_format = surface_capabilities
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_capabilities.formats[0]);

        let surface_configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            width: size.width,
            height: size.height,
            format: surface_texture_format,
            view_formats: vec![
                surface_texture_format,
                surface_texture_format.remove_srgb_suffix(),
            ],
            alpha_mode: surface_capabilities.alpha_modes[0],
            present_mode: surface_capabilities.present_modes[0],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };

        surface.configure(&device, &surface_configuration);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            usage: wgpu::BufferUsages::VERTEX,
            contents: bytemuck::cast_slice(VERTICES),
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index buffer"),
            usage: wgpu::BufferUsages::INDEX,
            contents: bytemuck::cast_slice(INDICES),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader_module_descriptor = wgpu::include_wgsl!("shader.wgsl");
        let shader_module = device.create_shader_module(shader_module_descriptor);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader_module,
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: Default::default(),
                entry_point: None,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_configuration.format.remove_srgb_suffix(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
                entry_point: None,
            }),

            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
                unclipped_depth: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            depth_stencil: None,
        });

        Self {
            surface,
            device,
            queue,
            surface_configuration,
            sampler,
            index_buffer,
            vertex_buffer,
            pipeline,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.surface_configuration.width = size.width;
            self.surface_configuration.height = size.height;
            self.surface
                .configure(&self.device, &self.surface_configuration);
        }
    }

    pub fn render(&mut self, frame: &wgpu::Texture, window: &Window) {
        let device = &self.device;
        let surface = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Validation
            | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize(window.inner_size());
                return;
            }
        };
        let surface_view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(surface.texture.format().remove_srgb_suffix()),
            ..Default::default()
        });
        let texture_view_y = frame.create_view(&wgpu::TextureViewDescriptor {
            label: Some("y texture"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        let texture_view_uv = frame.create_view(&wgpu::TextureViewDescriptor {
            label: Some("uv texture"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view_y),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view_uv),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut command_encoder = device.create_command_encoder(&Default::default());

        {
            let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }

        self.queue.submit(Some(command_encoder.finish()));
        window.pre_present_notify();
        self.queue.present(surface);
    }
}
