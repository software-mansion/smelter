use std::{borrow::Cow, num::NonZeroU32, sync::Arc, time::Duration};

use wgpu::{ShaderStages, naga};

use crate::{
    scene::ShaderParam,
    state::node_texture::{NodeTexture, NodeTextureState},
    wgpu::{
        WgpuCtx, WgpuErrorScope,
        common_pipeline::{self, CreateShaderError, Sampler},
    },
};

use super::{
    base_params::BaseShaderParameters,
    validation::{
        error::{ParametersValidationError, ShaderParseError},
        validate_contains_header, validate_params,
    },
};

pub(super) const USER_DEFINED_BUFFER_BINDING: u32 = 0;
pub(super) const USER_DEFINED_BUFFER_GROUP: u32 = 1;

#[derive(Debug)]
pub(super) struct ShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    sampler: Sampler,
    textures_bgl: wgpu::BindGroupLayout,
    module: naga::Module,
}

impl ShaderPipeline {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>, shader_src: Arc<str>) -> Result<Self, CreateShaderError> {
        let scope = WgpuErrorScope::push(&wgpu_ctx.device);

        let module = naga::front::wgsl::parse_str(&shader_src)
            .map_err(|err| CreateShaderError::ParseError(ShaderParseError::new(err, shader_src)))?;

        validate_contains_header(&wgpu_ctx.shader_header, &module)?;

        let shader_source = wgpu::ShaderSource::Naga(Cow::Owned(module.clone()));
        let sampler = Sampler::new(&wgpu_ctx.device);
        let textures_bgl = Self::input_textures_bgl(wgpu_ctx);
        let shader_module = wgpu_ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: shader_source,
            });
        let pipeline_layout =
            wgpu_ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shader transformation pipeline layout"),
                    bind_group_layouts: &[
                        &textures_bgl,
                        &wgpu_ctx.uniform_bgl,
                        &sampler.bind_group_layout,
                    ],
                    push_constant_ranges: &[wgpu::PushConstantRange {
                        stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        range: 0..BaseShaderParameters::push_constant_size(),
                    }],
                });
        let pipeline = common_pipeline::create_render_pipeline(
            "Shader node",
            &wgpu_ctx.device,
            &pipeline_layout,
            &shader_module,
            wgpu_ctx.default_view_format(),
        );

        scope.pop(&wgpu_ctx.device)?;

        Ok(Self {
            pipeline,
            sampler,
            textures_bgl,
            module,
        })
    }

    pub fn render(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        params: &wgpu::BindGroup,
        sources: &[&NodeTexture],
        target: &NodeTextureState,
        pts: Duration,
        clear_color: Option<wgpu::Color>,
    ) {
        let input_textures_bg = self.input_textures_bg(wgpu_ctx, sources);

        let mut encoder = wgpu_ctx.device.create_command_encoder(&Default::default());
        let clear_color = clear_color.unwrap_or(wgpu::Color::TRANSPARENT);

        let mut render_plane = |plane_id: i32, clear: bool| {
            let load = match clear {
                true => wgpu::LoadOp::Clear(clear_color),
                false => wgpu::LoadOp::Load,
            };

            let base_params =
                BaseShaderParameters::new(plane_id, pts, sources.len() as u32, target.resolution());
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                    view: target.view(),
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);

            render_pass.set_push_constants(
                ShaderStages::VERTEX_FRAGMENT,
                0,
                base_params.push_constant(),
            );

            render_pass.set_bind_group(0, &input_textures_bg, &[]);
            render_pass.set_bind_group(USER_DEFINED_BUFFER_GROUP, params, &[]);
            render_pass.set_bind_group(2, &self.sampler.bind_group, &[]);

            wgpu_ctx.plane.draw(&mut render_pass);
        };

        if sources.is_empty() {
            render_plane(-1, true)
        } else {
            render_plane(0, true);
            for plane_id in 1..sources.len() {
                render_plane(plane_id as i32, false);
            }
        }

        wgpu_ctx.queue.submit(Some(encoder.finish()));
    }

    pub fn validate_params(&self, params: &ShaderParam) -> Result<(), ParametersValidationError> {
        let ty = self
            .module
            .global_variables
            .iter()
            .find(|(_, global)| match global.binding.as_ref() {
                Some(binding) => {
                    (binding.group, binding.binding)
                        == (USER_DEFINED_BUFFER_GROUP, USER_DEFINED_BUFFER_BINDING)
                }

                None => false,
            })
            .map(|(_, handle)| handle.ty)
            .ok_or(ParametersValidationError::NoBindingInShader)?;

        validate_params(params, ty, &self.module)
    }

    fn input_textures_bgl(wgpu_ctx: &Arc<WgpuCtx>) -> wgpu::BindGroupLayout {
        let count = match cfg!(target_arch = "wasm32") {
            false => NonZeroU32::new(super::SHADER_INPUT_TEXTURES_AMOUNT),
            true => None,
        };

        wgpu_ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shader transformation textures bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                }],
            })
    }

    fn input_textures_bg(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        sources: &[&NodeTexture],
    ) -> wgpu::BindGroup {
        match cfg!(target_arch = "wasm32") {
            false => {
                let mut texture_views: Vec<&wgpu::TextureView> = sources
                    .iter()
                    .map(|texture| {
                        texture
                            .state()
                            .map(NodeTextureState::view)
                            .unwrap_or_else(|| wgpu_ctx.default_empty_view())
                    })
                    .collect();

                texture_views.extend(
                    (sources.len()..super::SHADER_INPUT_TEXTURES_AMOUNT as usize)
                        .map(|_| wgpu_ctx.default_empty_view()),
                );

                wgpu_ctx
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.textures_bgl,
                        label: None,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureViewArray(&texture_views),
                        }],
                    })
            }
            true => {
                let texture_view = sources
                    .first()
                    .and_then(|texture| texture.state())
                    .map(|state| state.view())
                    .unwrap_or_else(|| wgpu_ctx.default_empty_view());

                wgpu_ctx
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.textures_bgl,
                        label: None,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(texture_view),
                        }],
                    })
            }
        }
    }
}
