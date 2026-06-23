use std::{collections::HashMap, sync::Arc, time::Duration};

use tracing::{error, warn};

use crate::{
    Frame, FrameData, FrameSet, InputId, OutputId, RenderingMode, Resolution,
    scene::RGBColor,
    state::{RenderCtx, node::RenderNode, render_graph::RenderGraph},
    wgpu::texture::{
        PlanarYuvPendingDownload, PlanarYuvVariant, RgbaLinearTexture,
        RgbaMultiViewTexture, RgbaSrgbTexture,
    },
};

use super::{
    input_texture::InputTexture, node_texture::NodeTexture, output_texture::OutputTexture,
};

pub(super) fn populate_inputs(
    ctx: &RenderCtx,
    scene: &mut RenderGraph,
    texture_upload_belt: &mut wgpu::util::StagingBelt,
    mut frame_set: FrameSet<InputId>,
) {
    let mut has_staged_uploads = false;
    let _ = ctx.wgpu_ctx.device.poll(wgpu::PollType::Poll);
    let mut upload_encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched input texture upload"),
        });

    for (input_id, (_node_texture, input_textures)) in &mut scene.inputs {
        let Some(frame) = frame_set.frames.remove(input_id) else {
            input_textures.clear();
            continue;
        };
        if Duration::saturating_sub(frame_set.pts, ctx.stream_fallback_timeout)
            > frame.pts
        {
            input_textures.clear();
            continue;
        }

        if input_textures.encode_upload(
            ctx.wgpu_ctx,
            &mut upload_encoder,
            texture_upload_belt,
            frame,
        ) {
            has_staged_uploads = true;
        }
    }

    if has_staged_uploads {
        texture_upload_belt.finish();
        ctx.wgpu_ctx.queue.submit(Some(upload_encoder.finish()));
        texture_upload_belt.recall();
    } else {
        ctx.wgpu_ctx.queue.submit([]);
    }

    let mut has_batched_conversions = false;
    let mut encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched input texture conversion"),
        });
    for (_input_id, (node_texture, input_textures)) in &mut scene.inputs {
        if input_textures.encode_convert_to_node_texture(
            ctx.wgpu_ctx,
            &mut encoder,
            node_texture,
        ) {
            has_batched_conversions = true;
        }
    }
    if has_batched_conversions {
        ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
    }
}

enum PartialOutputFrame<'a, F>
where
    F: FnOnce() -> Result<bytes::Bytes, wgpu::BufferAsyncError> + 'a,
{
    PendingYuvDownload {
        output_id: OutputId,
        pending_download: PlanarYuvPendingDownload<'a, F, wgpu::BufferAsyncError>,
        resolution: Resolution,
    },
    CompleteFrame {
        output_id: OutputId,
        frame: Frame,
    },
}

pub(super) fn read_outputs(
    ctx: &RenderCtx,
    scene: &mut RenderGraph,
    pts: Duration,
) -> HashMap<OutputId, Frame> {
    let mut partial_textures = Vec::with_capacity(scene.outputs.len());
    let mut nv12_encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched NV12 output conversion"),
        });
    let mut nv12_conversions = 0usize;
    for (output_id, output) in &scene.outputs {
        let root = &output.root;
        match root.output_texture(&scene.inputs).state() {
            Some(node) => match &output.output_texture {
                OutputTexture::PlanarYuvTextures(yuv_output) => {
                    ctx.wgpu_ctx.format.rgba_to_yuv.convert(
                        ctx.wgpu_ctx,
                        node.output_texture_bind_group(),
                        yuv_output.yuv_textures(),
                    );
                    let pending_download = yuv_output.start_download(ctx.wgpu_ctx);
                    partial_textures.push(PartialOutputFrame::PendingYuvDownload {
                        output_id: output_id.clone(),
                        pending_download,
                        resolution: yuv_output.resolution(),
                    });
                }
                OutputTexture::Rgba8UnormWgpuTexture(rgba_output) => {
                    let view_formats = match ctx.wgpu_ctx.mode {
                        RenderingMode::GpuOptimized => &[
                            wgpu::TextureFormat::Rgba8Unorm,
                            wgpu::TextureFormat::Rgba8UnormSrgb,
                        ][..],
                        RenderingMode::CpuOptimized => {
                            &[wgpu::TextureFormat::Rgba8Unorm][..]
                        }
                        RenderingMode::WebGl => {
                            &[wgpu::TextureFormat::Rgba8UnormSrgb][..]
                        }
                    };
                    let texture =
                        rgba_output.copy_from(ctx.wgpu_ctx, node.texture(), view_formats);
                    let frame = Frame {
                        resolution: texture.size().into(),
                        data: FrameData::Rgba8UnormWgpuTexture(texture),
                        pts,
                    };
                    partial_textures.push(PartialOutputFrame::CompleteFrame {
                        output_id: output_id.clone(),
                        frame,
                    })
                }
                OutputTexture::Nv12WgpuTexture(nv12_output) => {
                    let texture = nv12_output.convert_from_with_encoder(
                        ctx.wgpu_ctx,
                        &mut nv12_encoder,
                        node.output_texture_bind_group(),
                    );
                    nv12_conversions += 1;
                    let resolution = nv12_output.resolution();
                    let frame = Frame {
                        resolution,
                        data: FrameData::Nv12WgpuTexture(texture),
                        pts,
                    };
                    partial_textures.push(PartialOutputFrame::CompleteFrame {
                        output_id: output_id.clone(),
                        frame,
                    })
                }
            },
            // fallback if root node in render graph is empty
            None => match &output.output_texture {
                OutputTexture::PlanarYuvTextures(yuv_output) => {
                    yuv_output
                        .yuv_textures()
                        .fill_with_color(ctx.wgpu_ctx, RGBColor::BLACK);

                    let pending_download = yuv_output.start_download(ctx.wgpu_ctx);
                    partial_textures.push(PartialOutputFrame::PendingYuvDownload {
                        output_id: output_id.clone(),
                        pending_download,
                        resolution: yuv_output.resolution(),
                    });
                }
                OutputTexture::Rgba8UnormWgpuTexture(rgba_output) => {
                    let resolution = rgba_output.resolution();
                    let wgpu_texture = match ctx.wgpu_ctx.mode {
                        RenderingMode::GpuOptimized => {
                            RgbaMultiViewTexture::new(ctx.wgpu_ctx, resolution)
                                .texture_owned()
                        }
                        RenderingMode::WebGl => {
                            RgbaSrgbTexture::new(ctx.wgpu_ctx, resolution).texture_owned()
                        }
                        RenderingMode::CpuOptimized => {
                            RgbaLinearTexture::new(ctx.wgpu_ctx, resolution)
                                .texture_owned()
                        }
                    };
                    let frame = Frame {
                        data: FrameData::Rgba8UnormWgpuTexture(Arc::new(wgpu_texture)),
                        resolution,
                        pts,
                    };
                    partial_textures.push(PartialOutputFrame::CompleteFrame {
                        output_id: output_id.clone(),
                        frame,
                    })
                }
                OutputTexture::Nv12WgpuTexture(nv12_output) => {
                    let texture =
                        nv12_output.fill_with_color(ctx.wgpu_ctx, RGBColor::BLACK);
                    let frame = Frame {
                        data: FrameData::Nv12WgpuTexture(texture),
                        resolution: nv12_output.resolution(),
                        pts,
                    };
                    partial_textures.push(PartialOutputFrame::CompleteFrame {
                        output_id: output_id.clone(),
                        frame,
                    });
                }
            },
        };
    }

    if nv12_conversions > 0 {
        ctx.wgpu_ctx.queue.submit(Some(nv12_encoder.finish()));
    }

    if partial_textures.iter().any(PartialOutputFrame::needs_download) {
        while let Err(wgpu::PollError::Timeout) =
            ctx.wgpu_ctx.device.poll(wgpu::PollType::wait_indefinitely())
        {
            warn!("Device poll failed.")
        }
    }

    let mut result = HashMap::new();
    for partial in partial_textures {
        match partial {
            PartialOutputFrame::PendingYuvDownload {
                output_id,
                pending_download,
                resolution,
            } => {
                let yuv_planes = match pending_download.wait() {
                    Ok(data) => data,
                    Err(err) => {
                        error!("Failed to download frame: {}", err);
                        continue;
                    }
                };

                let Some(output) = &scene.outputs.get(&output_id) else {
                    error!("Output_id {} not found", output_id);
                    continue;
                };
                let data = match &output.output_texture {
                    OutputTexture::PlanarYuvTextures(planar_yuv_output) => {
                        match planar_yuv_output.yuv_textures().variant() {
                            PlanarYuvVariant::YUV420 => {
                                FrameData::PlanarYuv420(yuv_planes)
                            }
                            PlanarYuvVariant::YUV422 => {
                                FrameData::PlanarYuv422(yuv_planes)
                            }
                            PlanarYuvVariant::YUV444 => {
                                FrameData::PlanarYuv444(yuv_planes)
                            }
                            PlanarYuvVariant::YUVJ420 => {
                                FrameData::PlanarYuvJ420(yuv_planes)
                            }
                        }
                    }
                    _ => FrameData::PlanarYuv420(yuv_planes),
                };
                let frame = Frame { data, resolution, pts };
                result.insert(output_id.clone(), frame);
            }

            PartialOutputFrame::CompleteFrame { output_id, frame } => {
                result.insert(output_id, frame);
            }
        }
    }
    result
}

impl<'a, F> PartialOutputFrame<'a, F>
where
    F: FnOnce() -> Result<bytes::Bytes, wgpu::BufferAsyncError>,
{
    fn needs_download(&self) -> bool {
        matches!(self, Self::PendingYuvDownload { .. })
    }
}

pub(super) fn run_transforms(
    ctx: &mut RenderCtx,
    scene: &mut RenderGraph,
    pts: Duration,
) {
    for output in scene.outputs.values_mut() {
        render_node(ctx, &scene.inputs, pts, &mut output.root);
    }
}

pub(super) fn render_node(
    ctx: &mut RenderCtx,
    inputs: &HashMap<InputId, (NodeTexture, InputTexture)>,
    pts: Duration,
    node: &mut RenderNode,
) {
    for child_node in node.children.iter_mut() {
        render_node(ctx, inputs, pts, child_node);
    }

    let input_textures: Vec<_> =
        node.children.iter().map(|node| node.output_texture(inputs)).collect();
    node.renderer.render(ctx, &input_textures, &mut node.output, pts);
}
