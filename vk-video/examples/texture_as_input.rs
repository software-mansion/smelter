#[cfg(vulkan)]
fn main() {
    use vk_video::VulkanInstance;

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();

    if args.len() != 4 {
        println!("usage: {} WIDTH HEIGHT FRAME_COUNT", args[0]);
        return;
    }

    let width = args[1].parse::<u32>().expect("parse width");
    let height = args[2].parse::<u32>().expect("parse height");
    let frame_count = args[3].parse::<u32>().expect("parse frame count");

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_device = vulkan_instance
        .create_device(
            wgpu::Features::PUSH_CONSTANTS,
            wgpu::Limits {
                max_push_constant_size: 4,
                ..Default::default()
            },
            None,
        )
        .unwrap();

    let wgpu_state = WgpuState::new(
        vulkan_device.wgpu_device(),
        vulkan_device.wgpu_queue(),
        width,
        height,
    );

    let mut encoder = vulkan_device
        .create_converter(
            vk_video::H264Profile::High,
            1280,
            720,
            30,
            vk_video::RateControl::Vbr {
                average_bitrate: 500000,
                max_bitrate: 2000000,
            },
        )
        .unwrap();

    let mut output_file = std::fs::File::create("output.h264").unwrap();

    for i in 0..frame_count {
        use std::io::Write;

        let time = 1.0 / 30.0 * i as f32;
        wgpu_state.render(time);

        let res = encoder
            .encode_texture(wgpu_state.texture.clone(), false)
            .unwrap();

        output_file.write_all(&res).unwrap();
    }
}

struct WgpuState {
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl WgpuState {
    fn new(device: wgpu::Device, queue: wgpu::Queue, width: u32, height: u32) -> WgpuState {
        let shader = wgpu::include_wgsl!("texture_as_input.wgsl");
        let shader = device.create_shader_module(shader);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wgpu pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[wgpu::PushConstantRange {
                range: 0..4,
                stages: wgpu::ShaderStages::VERTEX,
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wgpu pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                buffers: &[],
                entry_point: None,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    blend: None,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                front_face: wgpu::FrontFace::Ccw,
                conservative: false,
                unclipped_depth: false,
                strip_index_format: None,
            },
            multiview: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: None,
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wgpu render target"),
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 1,
            view_formats: &[],
            mip_level_count: 1,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("wgpu render target view"),
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(wgpu::TextureUsages::RENDER_ATTACHMENT),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            aspect: wgpu::TextureAspect::All,
        });

        WgpuState {
            pipeline,
            texture,
            view,
            device,
            queue,
        }
    }

    fn render(&self, time: f32) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wgpu encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wgpu render pass"),
                timestamp_writes: None,
                occlusion_query_set: None,
                depth_stencil_attachment: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                })],
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, &time.to_ne_bytes());
            render_pass.draw(0..3, 0..1);
        }

        encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: &self.texture,
                state: wgpu::TextureUses::RESOURCE,
                selector: None,
            }]
            .into_iter(),
        );

        let buffer = encoder.finish();

        let submission_index = self.queue.submit([buffer]);

        while let Err(_) = self.device.poll(wgpu::PollType::WaitForSubmissionIndex(
            submission_index.clone(),
        )) {}
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
