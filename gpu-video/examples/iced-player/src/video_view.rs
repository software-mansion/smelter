use gpu_video::{
    OutputFrame, VideoTexture, WgpuNv12ToRgbaConverter,
    parameters::{ColorRange, ColorSpace, WgpuConverterParameters},
};
use iced::widget::shader::{self, Action, Viewport};
use iced::{Event, Rectangle, mouse, window};
use iced_custom_wgpu::wgpu;

use crate::Message;

pub struct VideoView {
    device: wgpu::Device,
    queue: wgpu::Queue,
    converter: WgpuNv12ToRgbaConverter,
    texture: Option<wgpu::Texture>,
}

impl VideoView {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            converter: WgpuNv12ToRgbaConverter::new(
                device,
                WgpuConverterParameters {
                    color_space: ColorSpace::BT709,
                    color_range: ColorRange::Limited,
                },
            )
            .unwrap(),
            texture: None,
        }
    }

    fn output_texture(&mut self, frame: &OutputFrame<VideoTexture>) -> wgpu::TextureView {
        let size = frame.data.size();
        if let Some(texture) = &self.texture
            && texture.size() == size
        {
            return texture.create_view(&Default::default());
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let view = texture.create_view(&Default::default());
        self.texture = Some(texture);
        view
    }

    pub fn update(&mut self, frame: &OutputFrame<VideoTexture>) {
        let view = self.output_texture(frame);
        let bind_group = self.converter.create_input_bind_group(frame).unwrap();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.converter.convert(&mut encoder, &bind_group, &view);
        self.queue.submit([encoder.finish()]);
    }

    pub fn size(&self) -> Option<wgpu::Extent3d> {
        self.texture.as_ref().map(|texture| texture.size())
    }
}

impl shader::Program<Message> for VideoView {
    type State = ();
    type Primitive = Primitive;

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                Some(Action::publish(Message::Tick(*now)))
            }
            _ => None,
        }
    }

    fn draw(&self, _state: &Self::State, _cursor: mouse::Cursor, _bounds: Rectangle) -> Primitive {
        Primitive {
            texture: self.texture.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    texture: Option<wgpu::Texture>,
}

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let Some(texture) = &self.texture else {
            pipeline.bind_group = None;
            return;
        };

        if pipeline
            .bind_group
            .as_ref()
            .is_none_or(|(t, _)| t != texture)
        {
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(pipeline.view_format),
                ..Default::default()
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });
            pipeline.bind_group = Some((texture.clone(), bind_group));
        }

        let bounds = *bounds * viewport.scale_factor();
        let (width, height) = (texture.width() as f32, texture.height() as f32);
        let scale = (bounds.width / width).min(bounds.height / height);
        let (width, height) = (width * scale, height * scale);
        pipeline.viewport = Rectangle {
            x: bounds.x + (bounds.width - width) / 2.0,
            y: bounds.y + (bounds.height - height) / 2.0,
            width,
            height,
        };
    }

    fn draw(&self, pipeline: &Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some((_, bind_group)) = &pipeline.bind_group else {
            return true;
        };

        let viewport = pipeline.viewport;
        render_pass.set_viewport(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            0.0,
            1.0,
        );
        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        true
    }
}

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    view_format: wgpu::TextureFormat,
    bind_group: Option<(wgpu::Texture, wgpu::BindGroup)>,
    viewport: Rectangle,
}

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
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
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let view_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            view_format,
            bind_group: None,
            viewport: Rectangle::default(),
        }
    }
}
