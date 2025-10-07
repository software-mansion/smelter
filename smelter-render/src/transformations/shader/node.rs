use std::{sync::Arc, time::Duration};

use wgpu::util::DeviceExt;

use crate::{
    Resolution,
    scene::ShaderParam,
    state::{RenderCtx, node_texture::NodeTexture},
    wgpu::WgpuCtx,
};

use super::Shader;

pub struct ShaderNode {
    params_bind_group: wgpu::BindGroup,
    _custom_params_buffer: wgpu::Buffer,
    shader: Arc<Shader>,
    resolution: Resolution,
}

impl ShaderNode {
    pub fn new(
        ctx: &RenderCtx,
        shader: Arc<Shader>,
        shader_params: &Option<ShaderParam>,
        resolution: &Resolution,
    ) -> Self {
        let custom_params_buffer = Self::new_params_buffer(ctx.wgpu_ctx, shader_params);
        let params_bind_group = Self::new_params_bind_group(ctx.wgpu_ctx, &custom_params_buffer);

        Self {
            params_bind_group,
            _custom_params_buffer: custom_params_buffer,
            shader,
            resolution: *resolution,
        }
    }

    fn new_params_buffer(ctx: &WgpuCtx, shader_params: &Option<ShaderParam>) -> wgpu::Buffer {
        match shader_params {
            Some(params) => {
                let params = params.to_bytes();
                ctx.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("shader node custom params buffer"),
                        usage: wgpu::BufferUsages::UNIFORM,
                        contents: &params,
                    })
            }
            None => ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("shader node empty custom params buffer"),
                    contents: &[0],
                    usage: wgpu::BufferUsages::UNIFORM,
                }),
        }
    }

    fn new_params_bind_group(ctx: &WgpuCtx, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader node params bind group"),
            layout: &ctx.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    pub fn render(
        &self,
        wgpu_ctx: &Arc<WgpuCtx>,
        sources: &[&NodeTexture],
        target: &mut NodeTexture,
        pts: Duration,
    ) {
        let target = target.ensure_size(wgpu_ctx, self.resolution);

        self.shader.pipeline.render(
            wgpu_ctx,
            &self.params_bind_group,
            sources,
            target,
            pts,
            self.shader.clear_color,
        )
    }
}

pub trait ShaderParamExt {
    fn to_bytes(&self) -> bytes::Bytes;
}

impl ShaderParamExt for ShaderParam {
    fn to_bytes(&self) -> bytes::Bytes {
        fn extend_bytes(s: &ShaderParam, bytes: &mut bytes::BytesMut) {
            match s {
                ShaderParam::F32(v) => bytes.extend_from_slice(&v.to_le_bytes()),
                ShaderParam::U32(v) => bytes.extend_from_slice(&v.to_le_bytes()),
                ShaderParam::I32(v) => bytes.extend_from_slice(&v.to_le_bytes()),
                ShaderParam::List(v) => v.iter().for_each(|v| extend_bytes(v, bytes)),
                ShaderParam::Struct(v) => v.iter().for_each(|v| extend_bytes(&v.value, bytes)),
            }
        }

        let mut result = bytes::BytesMut::new();

        extend_bytes(self, &mut result);

        result.freeze()
    }
}
