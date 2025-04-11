use std::collections::HashMap;

use compositor_render::{FrameData, FrameSet, OutputId, Resolution};
use wasm_bindgen::JsValue;
use web_sys::OffscreenCanvasRenderingContext2d;
use wgpu::TextureFormat;

use super::{
    types::to_js_error,
    wgpu::{Quad, Vertex, WgpuContext},
};

pub struct OutputDownloader {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    output_contexts: HashMap<OutputId, OffscreenCanvasRenderingContext2d>,
}

impl OutputDownloader {
    pub fn new(wgpu_ctx: &WgpuContext) -> Self {
        let shader = wgpu_ctx
            .device
            .create_shader_module(wgpu::include_wgsl!("copy_texture_to_canvas.wgsl"));

        let bind_group_layout =
            wgpu_ctx
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Copy Texture Bind Group Layout"),
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

        let pipeline_layout =
            wgpu_ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Copy Texture Pipeline Layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let pipeline = wgpu_ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Copy Texture Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[Vertex::LAYOUT],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TextureFormat::Rgba8UnormSrgb,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let sampler = wgpu_ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Copy Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            sampler,
            output_contexts: HashMap::new(),
        }
    }
}

impl OutputDownloader {
    pub fn download_outputs(
        &mut self,
        wgpu_ctx: &WgpuContext,
        outputs: FrameSet<OutputId>,
    ) -> Result<(), JsValue> {
        for (id, frame) in outputs.frames.iter() {
            let FrameData::Rgba8UnormWgpuTexture(texture) = &frame.data else {
                panic!("Expected Rgba8UnormWgpuTexture");
            };

            wgpu_ctx.ensure_surface_resolution(frame.resolution);
            let bind_group = self.new_bind_group(&wgpu_ctx.device, &texture);
            let mut encoder = wgpu_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            let out_texture = wgpu_ctx
                .surface
                .get_current_texture()
                .map_err(to_js_error)?;
            let out_texture_view = out_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("OutputDownloader"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &out_texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(&self.pipeline);
                render_pass.set_bind_group(0, &bind_group, &[]);
                render_pass.set_vertex_buffer(0, wgpu_ctx.quad.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    wgpu_ctx.quad.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(0..Quad::INDICES.len() as u32, 0, 0..1);
            }

            wgpu_ctx.queue.submit(Some(encoder.finish()));
            // TODO(noituri): This may not work correctly because it only schedules the render
            // result to be present
            out_texture.present();
            // TODO(noituri): This should be called after the render is done
            self.update_output_canvas(id, wgpu_ctx, frame.resolution)?;
        }

        Ok(())
    }

    pub fn add_output(&mut self, output_id: OutputId, ctx: OffscreenCanvasRenderingContext2d) {
        self.output_contexts.insert(output_id, ctx);
    }

    pub fn remove_output(&mut self, output_id: &OutputId) {
        self.output_contexts.remove(output_id);
    }

    fn new_bind_group(&self, device: &wgpu::Device, texture: &wgpu::Texture) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Copy Texture Bind Group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    fn update_output_canvas(
        &self,
        id: &OutputId,
        wgpu_ctx: &WgpuContext,
        resolution: Resolution,
    ) -> Result<(), JsValue> {
        let Some(ctx) = self.output_contexts.get(id) else {
            return Err(JsValue::from_str("Output context not found"));
        };

        let width = resolution.width as f64;
        let height = resolution.height as f64;
        ctx.fill_rect(0.0, 0.0, width, height);
        ctx.draw_image_with_offscreen_canvas_and_dw_and_dh(
            &wgpu_ctx.canvas,
            0.0,
            0.0,
            width,
            height,
        )
    }
}
