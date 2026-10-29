use eframe::egui;
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use gpu_video::VideoTexture;

pub struct VideoRenderer {
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    bind_group: Option<wgpu::BindGroup>,
}

impl VideoRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
        };

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader_module,
                buffers: &[],
                compilation_options: Default::default(),
                entry_point: None,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
                entry_point: None,
            }),
            primitive: wgpu::PrimitiveState::default(),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            depth_stencil: None,
        });

        Self {
            sampler,
            pipeline,
            bind_group: None,
        }
    }

    fn prepare(&mut self, device: &wgpu::Device, frame: &VideoTexture) {
        let texture_view_y = frame.y_plane().create_view(Some("y texture"));
        let texture_view_uv = frame.uv_plane().create_view(Some("uv texture"));

        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        }));
    }

    fn draw(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

/// Draws `frame` into the rect of the paint callback, using the [`VideoRenderer`] stored in
/// egui's callback resources.
pub struct VideoCallback {
    pub frame: VideoTexture,
}

impl CallbackTrait for VideoCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let renderer: &mut VideoRenderer = callback_resources.get_mut().unwrap();
        renderer.prepare(device, &self.frame);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let renderer: &VideoRenderer = callback_resources.get().unwrap();
        renderer.draw(render_pass);
    }
}
