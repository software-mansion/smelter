use tracing::error;
use wgpu::{
    BindGroupLayoutDescriptor, BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    Resolution,
    scene::RGBAColor,
    wgpu::{WgpuCtx, utils::convert_to_shader_color},
};

use super::{BorderRadius, RenderLayout};

const MAX_MASKS: usize = 20;
const MAX_LAYOUTS_COUNT: usize = 100;
const TEXTURE_PARAMS_BUFFER_SIZE: usize = MAX_LAYOUTS_COUNT * 80;
const COLOR_PARAMS_SIZE: usize = MAX_LAYOUTS_COUNT * 80;
const BOX_SHADOW_PARAMS_SIZE: usize = MAX_LAYOUTS_COUNT * 80;

#[derive(Debug)]
pub struct LayoutInfo {
    pub layout_type: u32,
    pub index: u32,
    pub masks_len: u32,
}

impl LayoutInfo {
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut result = [0u8; 16];
        result[0..4].copy_from_slice(&self.layout_type.to_le_bytes());
        result[4..8].copy_from_slice(&self.index.to_le_bytes());
        result[8..12].copy_from_slice(&self.masks_len.to_le_bytes());
        result
    }
}

#[derive(Debug)]
pub struct ParamsBindGroups {
    pub bind_group_1: wgpu::BindGroup,
    pub bind_group_1_layout: wgpu::BindGroupLayout,
    output_resolution_buffer: wgpu::Buffer,
    texture_params_buffer: wgpu::Buffer,
    color_params_buffer: wgpu::Buffer,
    box_shadow_params_buffer: wgpu::Buffer,
    pub bind_groups_2: Vec<(wgpu::BindGroup, wgpu::Buffer)>,
    pub bind_group_2_layout: wgpu::BindGroupLayout,
}

impl ParamsBindGroups {
    pub fn new(ctx: &WgpuCtx) -> ParamsBindGroups {
        let output_resolution_buffer = create_buffer(ctx, 16);
        let texture_params_buffer = create_buffer(ctx, TEXTURE_PARAMS_BUFFER_SIZE);
        let color_params_buffer = create_buffer(ctx, COLOR_PARAMS_SIZE);
        let box_shadow_params_buffer = create_buffer(ctx, BOX_SHADOW_PARAMS_SIZE);

        let bind_group_1_layout = ctx
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Bind group 1 layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        count: None,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        count: None,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        count: None,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        count: None,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    },
                ],
            });

        let bind_group_1 = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind group 1"),
            layout: &bind_group_1_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output_resolution_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: texture_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: color_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: box_shadow_params_buffer.as_entire_binding(),
                },
            ],
        });

        let bind_group_2_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Bind group 2 layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let mut bind_groups_2 = Vec::with_capacity(100);
        for _ in 0..100 {
            let buffer = create_buffer(ctx, 20 * 32);

            let bind_group_2 = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Bind group 2"),
                layout: &bind_group_2_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            bind_groups_2.push((bind_group_2, buffer));
        }

        Self {
            bind_group_1,
            output_resolution_buffer,
            texture_params_buffer,
            color_params_buffer,
            box_shadow_params_buffer,
            bind_groups_2,
            bind_group_1_layout,
            bind_group_2_layout,
        }
    }

    pub fn update(
        &self,
        ctx: &WgpuCtx,
        output_resolution: Resolution,
        layouts: Vec<RenderLayout>,
    ) -> Vec<LayoutInfo> {
        if layouts.len() > MAX_LAYOUTS_COUNT {
            error!(
                "Max layouts count ({}) exceeded ({}). Skipping rendering some of them.",
                MAX_LAYOUTS_COUNT,
                layouts.len()
            )
        }

        let mut output_resolution_bytes = [0u8; 8];
        output_resolution_bytes[0..4]
            .copy_from_slice(&(output_resolution.width as f32).to_le_bytes());
        output_resolution_bytes[4..8]
            .copy_from_slice(&(output_resolution.height as f32).to_le_bytes());

        ctx.queue
            .write_buffer(&self.output_resolution_buffer, 0, &output_resolution_bytes);

        let mut layout_infos = Vec::new();

        let mut texture_params = Vec::new();
        let mut color_params = Vec::new();
        let mut box_shadow_params = Vec::new();

        for (index, layout) in layouts.iter().enumerate().take(MAX_LAYOUTS_COUNT) {
            let RenderLayout {
                top,
                left,
                width,
                height,
                rotation_degrees,
                border_radius,
                masks,
                content,
            } = layout;
            let border_radius_bytes = borders_radius_to_bytes(*border_radius);

            match content {
                super::RenderLayoutContent::Color {
                    color,
                    border_color,
                    border_width,
                } => {
                    let layout_info = LayoutInfo {
                        layout_type: 1,
                        index: color_params.len() as u32,
                        masks_len: masks.len() as u32,
                    };
                    let mut color_params_bytes = [0u8; 80];
                    color_params_bytes[0..16].copy_from_slice(&border_radius_bytes);
                    color_params_bytes[16..32].copy_from_slice(&color_to_bytes(ctx, border_color));
                    color_params_bytes[32..48].copy_from_slice(&color_to_bytes(ctx, color));
                    color_params_bytes[48..52].copy_from_slice(&top.to_le_bytes());
                    color_params_bytes[52..56].copy_from_slice(&left.to_le_bytes());
                    color_params_bytes[56..60].copy_from_slice(&width.to_le_bytes());
                    color_params_bytes[60..64].copy_from_slice(&height.to_le_bytes());
                    color_params_bytes[64..68].copy_from_slice(&rotation_degrees.to_le_bytes());
                    color_params_bytes[68..72].copy_from_slice(&border_width.to_le_bytes());
                    color_params.push(color_params_bytes);
                    layout_infos.push(layout_info);
                }
                super::RenderLayoutContent::ChildNode {
                    index: _,
                    crop,
                    border_color,
                    border_width,
                } => {
                    let layout_info = LayoutInfo {
                        layout_type: 0,
                        index: texture_params.len() as u32,
                        masks_len: masks.len() as u32,
                    };
                    let mut texture_params_bytes = [0u8; 80];
                    texture_params_bytes[0..16].copy_from_slice(&border_radius_bytes);
                    texture_params_bytes[16..32]
                        .copy_from_slice(&color_to_bytes(ctx, border_color));
                    texture_params_bytes[32..36].copy_from_slice(&top.to_le_bytes());
                    texture_params_bytes[36..40].copy_from_slice(&left.to_le_bytes());
                    texture_params_bytes[40..44].copy_from_slice(&width.to_le_bytes());
                    texture_params_bytes[44..48].copy_from_slice(&height.to_le_bytes());
                    texture_params_bytes[48..52].copy_from_slice(&crop.top.to_le_bytes());
                    texture_params_bytes[52..56].copy_from_slice(&crop.left.to_le_bytes());
                    texture_params_bytes[56..60].copy_from_slice(&crop.width.to_le_bytes());
                    texture_params_bytes[60..64].copy_from_slice(&crop.height.to_le_bytes());
                    texture_params_bytes[64..68].copy_from_slice(&rotation_degrees.to_le_bytes());
                    texture_params_bytes[68..72].copy_from_slice(&border_width.to_le_bytes());
                    texture_params.push(texture_params_bytes);
                    layout_infos.push(layout_info);
                }
                super::RenderLayoutContent::BoxShadow { color, blur_radius } => {
                    let layout_info = LayoutInfo {
                        layout_type: 2,
                        index: box_shadow_params.len() as u32,
                        masks_len: masks.len() as u32,
                    };
                    let mut box_shadow_params_bytes = [0u8; 64];
                    box_shadow_params_bytes[0..16].copy_from_slice(&border_radius_bytes);
                    box_shadow_params_bytes[16..32].copy_from_slice(&color_to_bytes(ctx, color));
                    box_shadow_params_bytes[32..36].copy_from_slice(&top.to_le_bytes());
                    box_shadow_params_bytes[36..40].copy_from_slice(&left.to_le_bytes());
                    box_shadow_params_bytes[40..44].copy_from_slice(&width.to_le_bytes());
                    box_shadow_params_bytes[44..48].copy_from_slice(&height.to_le_bytes());
                    box_shadow_params_bytes[48..52]
                        .copy_from_slice(&rotation_degrees.to_le_bytes());
                    box_shadow_params_bytes[52..56].copy_from_slice(&blur_radius.to_le_bytes());
                    box_shadow_params.push(box_shadow_params_bytes);
                    layout_infos.push(layout_info);
                }
            }
            if masks.len() > MAX_MASKS {
                error!(
                    "Max parent border radiuses count ({}) exceeded ({}). Skipping rendering some og them.",
                    MAX_MASKS,
                    masks.len()
                );
            }

            let mut masks_bytes = Vec::new();

            for mask in masks.iter().take(20) {
                let mut mask_bytes = [0u8; 32];
                mask_bytes[0..16].copy_from_slice(&borders_radius_to_bytes(mask.radius));
                mask_bytes[16..20].copy_from_slice(&mask.top.to_le_bytes());
                mask_bytes[20..24].copy_from_slice(&mask.left.to_le_bytes());
                mask_bytes[24..28].copy_from_slice(&mask.width.to_le_bytes());
                mask_bytes[28..32].copy_from_slice(&mask.height.to_le_bytes());

                masks_bytes.push(mask_bytes);
            }

            masks_bytes.resize_with(20, || [0u8; 32]);
            match self.bind_groups_2.get(index) {
                Some((_bg, buffer)) => {
                    ctx.queue.write_buffer(buffer, 0, &masks_bytes.concat());
                }
                None => {
                    error!("Not enought parent border radiuses bind groups preallocated");
                }
            }

            ctx.queue
                .write_buffer(&self.bind_groups_2[index].1, 0, &masks_bytes.concat());
        }
        texture_params.resize_with(100, || [0u8; 80]);
        color_params.resize_with(100, || [0u8; 80]);
        box_shadow_params.resize_with(100, || [0u8; 64]);

        ctx.queue
            .write_buffer(&self.texture_params_buffer, 0, &texture_params.concat());
        ctx.queue
            .write_buffer(&self.color_params_buffer, 0, &color_params.concat());
        ctx.queue.write_buffer(
            &self.box_shadow_params_buffer,
            0,
            &box_shadow_params.concat(),
        );

        layout_infos
    }
}

fn create_buffer(ctx: &WgpuCtx, size: usize) -> wgpu::Buffer {
    ctx.device.create_buffer_init(&BufferInitDescriptor {
        label: Some("params buffer"),
        contents: &vec![0u8; size],
        usage: BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn borders_radius_to_bytes(border_radius: BorderRadius) -> [u8; 16] {
    let mut result = [0u8; 16];
    result[0..4].copy_from_slice(&border_radius.top_left.to_le_bytes());
    result[4..8].copy_from_slice(&border_radius.top_right.to_le_bytes());
    result[8..12].copy_from_slice(&border_radius.bottom_right.to_le_bytes());
    result[12..16].copy_from_slice(&border_radius.bottom_left.to_le_bytes());
    result
}

fn color_to_bytes(ctx: &WgpuCtx, color: &RGBAColor) -> [u8; 16] {
    let [r, g, b, a] = convert_to_shader_color(ctx, color);
    let mut result = [0u8; 16];
    result[0..4].copy_from_slice(&(r as f32).to_le_bytes());
    result[4..8].copy_from_slice(&(g as f32).to_le_bytes());
    result[8..12].copy_from_slice(&(b as f32).to_le_bytes());
    result[12..16].copy_from_slice(&(a as f32).to_le_bytes());
    result
}
