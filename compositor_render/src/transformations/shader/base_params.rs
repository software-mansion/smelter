use std::time::Duration;

use wgpu::util::DeviceExt;

use crate::{wgpu::WgpuCtx, Resolution};

#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
pub struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: [u32; 2],
    texture_count: u32,
}

impl BaseShaderParameters {
    pub fn new(
        plane_id: i32,
        time: Duration,
        texture_count: u32,
        output_resolution: Resolution,
    ) -> Self {
        Self {
            time: time.as_secs_f32(),
            texture_count,
            output_resolution: [
                output_resolution.width as u32,
                output_resolution.height as u32,
            ],
            plane_id,
        }
    }

    pub fn push_constant_size() -> u32 {
        let size = std::mem::size_of::<BaseShaderParameters>() as u32;
        match size % 4 {
            0 => size,
            rest => size + (4 - rest),
        }
    }

    pub fn push_constant(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

pub struct BaseShaderParamsUniform {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl BaseShaderParamsUniform {
    pub fn new(wgpu_ctx: &WgpuCtx, params: BaseShaderParameters) -> Self {
        let buffer = wgpu_ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BaseShaderParamsUniform"),
                contents: params.push_constant(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = wgpu_ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("BaseShaderParamsUniform bind group"),
                layout: &wgpu_ctx.uniform_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });

        Self { buffer, bind_group }
    }

    pub fn update(&mut self, wgpu_ctx: &WgpuCtx, params: BaseShaderParameters) {
        wgpu_ctx
            .queue
            .write_buffer(&self.buffer, 0, params.push_constant());
    }
}
