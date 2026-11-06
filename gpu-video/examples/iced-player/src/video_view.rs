use gpu_video::{
    OutputFrame, VideoTexture, WgpuNv12ToRgbaConverter,
    parameters::{ColorRange, ColorSpace, WgpuConverterParameters},
};
use iced_wgpu::wgpu;
use iced_widget::{
    core::{Rectangle, mouse},
    shader::{self, Viewport},
};

pub struct VideoView {
    converter: WgpuNv12ToRgbaConverter,
    texture: Option<wgpu::Texture>,
}

impl VideoView {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
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

    fn output_texture(
        &mut self,
        device: &wgpu::Device,
        frame: &OutputFrame<VideoTexture>,
    ) -> &wgpu::Texture {
        let size = frame.data.size();
        if self.texture.as_ref().is_none_or(|t| t.size() != size) {
            self.texture = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
            }));
        }

        self.texture.as_ref().unwrap()
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &OutputFrame<VideoTexture>,
    ) {
        let bind_group = self.converter.create_input_bind_group(frame).unwrap();
        let view = self
            .output_texture(device, frame)
            .create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        self.converter.convert(&mut encoder, &bind_group, &view);
        queue.submit([encoder.finish()]);
    }

    pub fn size(&self) -> Option<wgpu::Extent3d> {
        self.texture.as_ref().map(|texture| texture.size())
    }
}

impl<Message> shader::Program<Message> for VideoView {
    type State = ();
    type Primitive = VideoFrame;

    fn draw(&self, _state: &Self::State, _cursor: mouse::Cursor, _bounds: Rectangle) -> VideoFrame {
        VideoFrame {
            texture: self.texture.clone(),
        }
    }
}

#[derive(Debug)]
pub struct VideoFrame {
    texture: Option<wgpu::Texture>,
}

impl shader::Primitive for VideoFrame {
    type Pipeline = BlitPipeline;

    fn prepare(
        &self,
        pipeline: &mut BlitPipeline,
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
            .is_none_or(|(bound, _)| bound != texture)
        {
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(pipeline.sample_format),
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
        let scale =
            (bounds.width / texture.width() as f32).min(bounds.height / texture.height() as f32);
        let width = texture.width() as f32 * scale;
        let height = texture.height() as f32 * scale;
        pipeline.viewport = Rectangle {
            x: bounds.x + (bounds.width - width) / 2.0,
            y: bounds.y + (bounds.height - height) / 2.0,
            width,
            height,
        };
    }

    fn draw(&self, pipeline: &BlitPipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        if let Some((_, bind_group)) = &pipeline.bind_group {
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
        }

        true
    }
}

pub struct BlitPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sample_format: wgpu::TextureFormat,
    bind_group: Option<(wgpu::Texture, wgpu::BindGroup)>,
    viewport: Rectangle,
}

impl shader::Pipeline for BlitPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("video_view.wgsl"));

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
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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

        let sample_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            sample_format,
            bind_group: None,
            viewport: Rectangle::default(),
        }
    }
}
