use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{error, warn};

use std::any::Any;

use crate::{
    ExternalNv12FramePool, Frame, FrameData, FrameSet, InputId, OutputId, RenderingMode,
    Resolution,
    scene::RGBColor,
    state::{
        node::{NodeRenderStats, RenderNode},
        RenderCtx,
        render_graph::RenderGraph,
    },
    wgpu::texture::{
        PlanarYuvPendingDownload, PlanarYuvVariant, RgbaLinearTexture,
        RgbaMultiViewTexture, RgbaSrgbTexture,
    },
};

use super::{
    input_texture::InputTexture,
    node_texture::NodeTexture,
    output_texture::OutputTexture,
};

#[derive(Debug, Default)]
pub(super) struct InputStageStats {
    pub(super) total_ms: f64,
    pub(super) poll_ms: f64,
    pub(super) encode_upload_ms: f64,
    pub(super) upload_ms: f64,
    pub(super) encode_convert_ms: f64,
    pub(super) convert_ms: f64,
    pub(super) convert_wait_ms: f64,
    pub(super) upload_passes: usize,
    pub(super) convert_passes: usize,
}

#[derive(Debug, Default)]
pub(super) struct TransformStageStats {
    pub(super) total_ms: f64,
    pub(super) submit_ms: f64,
    pub(super) wait_ms: f64,
    pub(super) timestamp_queries: usize,
    pub(super) layout_ms: f64,
    pub(super) submits: usize,
    pub(super) lanczos_passes: usize,
    pub(super) layout_passes: usize,
    pub(super) intermediate_4k_textures: usize,
}

#[derive(Debug, Default)]
pub(super) struct OutputStageStats {
    pub(super) total_ms: f64,
    pub(super) encode_ms: f64,
    pub(super) submit_ms: f64,
    pub(super) wait_ms: f64,
    pub(super) timestamp_queries: usize,
    pub(super) assemble_ms: f64,
    pub(super) rgba_copy_passes: usize,
    pub(super) nv12_outputs: usize,
    pub(super) nv12_conversion_passes: usize,
    pub(super) planar_yuv_outputs: usize,
    pub(super) planar_yuv_passes: usize,
    pub(super) intermediate_4k_textures: usize,
}

pub(super) fn emit_input_signal_snapshots(frame_set: &FrameSet<InputId>) {
    let _ = frame_set;
}

pub(super) fn emit_render_frame_telemetry(
    pts: Duration,
    total_ms: f64,
    input: &InputStageStats,
    transform: &TransformStageStats,
    output: &OutputStageStats,
) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static FRAME: AtomicU64 = AtomicU64::new(0);
    let n = FRAME.fetch_add(1, Ordering::Relaxed);
    if n % 60 != 0 {
        return;
    }
    let _ = pts;
    tracing::warn!(
        total_ms = format!("{total_ms:.2}"),
        in_ms = format!("{:.2}", input.total_ms),
        tf_ms = format!("{:.2}", transform.total_ms),
        out_ms = format!("{:.2}", output.total_ms),
        lanczos_passes = transform.lanczos_passes,
        layout_passes = transform.layout_passes,
        tf_4k = transform.intermediate_4k_textures,
        out_4k = output.intermediate_4k_textures,
        nv12_outputs = output.nv12_outputs,
        nv12_passes = output.nv12_conversion_passes,
        submits = transform.submits,
        "render_telemetry"
    );
}

pub(super) fn populate_inputs(
    ctx: &RenderCtx,
    scene: &mut RenderGraph,
    texture_upload_belt: &mut wgpu::util::StagingBelt,
    mut frame_set: FrameSet<InputId>,
) -> InputStageStats {
    let total_started = Instant::now();
    let mut stats = InputStageStats::default();
    let mut has_staged_uploads = false;
    let poll_started = Instant::now();
    let _ = ctx.wgpu_ctx.device.poll(wgpu::PollType::Poll);
    stats.poll_ms = poll_started.elapsed().as_secs_f64() * 1000.0;
    let mut upload_encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched input texture upload"),
        });

    let encode_upload_started = Instant::now();
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
    stats.encode_upload_ms = encode_upload_started.elapsed().as_secs_f64() * 1000.0;

    let upload_started = Instant::now();
    if has_staged_uploads {
        texture_upload_belt.finish();
        ctx.wgpu_ctx.queue.submit(Some(upload_encoder.finish()));
        texture_upload_belt.recall();
        stats.upload_passes = 1;
    } else {
        ctx.wgpu_ctx.queue.submit([]);
    }
    stats.upload_ms = upload_started.elapsed().as_secs_f64() * 1000.0;

    let mut has_batched_conversions = false;
    let mut encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched input texture conversion"),
        });
    let encode_convert_started = Instant::now();
    for (_input_id, (node_texture, input_textures)) in &mut scene.inputs {
        if input_textures.encode_convert_to_node_texture(
            ctx.wgpu_ctx,
            &mut encoder,
            node_texture,
        ) {
            has_batched_conversions = true;
        }
    }
    stats.encode_convert_ms = encode_convert_started.elapsed().as_secs_f64() * 1000.0;
    let convert_started = Instant::now();
    if has_batched_conversions {
        ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
        stats.convert_wait_ms = diagnostic_wait_for_gpu(ctx.wgpu_ctx);
        stats.convert_passes = 1;
    }
    stats.convert_ms = convert_started.elapsed().as_secs_f64() * 1000.0;
    stats.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;
    stats
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
) -> (HashMap<OutputId, Frame>, OutputStageStats) {
    let started = Instant::now();
    let mut stats = OutputStageStats::default();
    let mut partial_textures = Vec::with_capacity(scene.outputs.len());
    let mut nv12_encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched NV12 output conversion"),
        });
    let timestamp_query =
        TimestampQuery::new(ctx.wgpu_ctx, nv12_encoder_label());
    if let Some(query) = &timestamp_query {
        nv12_encoder.write_timestamp(&query.query_set, 0);
    }
    let mut nv12_conversions = 0usize;
    // Zero-copy external NV12 outputs all record into ONE shared encoder and stage
    // their dma-buf write fence (no per-output submit). After the loop we do a
    // single `queue.submit` that consumes every staged fence at once, then finish
    // each token — collapsing the former N sequential fenced submits per frame.
    let mut external_encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched external NV12 zero-copy convert"),
        });
    let mut external_tokens: Vec<(Arc<dyn ExternalNv12FramePool>, Box<dyn Any + Send>)> =
        Vec::new();
    let encode_started = Instant::now();
    for (output_id, output) in &scene.outputs {
        let root = &output.root;
        if let OutputTexture::Nv12WgpuTexture(nv12_output) = &output.output_texture
            && let Some(node) = root
                .direct_nv12_passthrough_texture()
                .and_then(|texture| texture.state())
        {
            stats.nv12_outputs += 1;
            stats.nv12_conversion_passes += 2;
            if nv12_output.resolution().width >= 3840
                && nv12_output.resolution().height >= 2160
            {
                stats.intermediate_4k_textures += 1;
            }
            let texture = nv12_output.convert_lanczos_vertical_from_with_encoder(
                ctx.wgpu_ctx,
                &mut nv12_encoder,
                node.output_texture_bind_group(),
            );
            nv12_conversions += 1;
            let frame = Frame {
                resolution: nv12_output.resolution(),
                data: FrameData::Nv12WgpuTexture(texture),
                pts,
            };
            partial_textures.push(PartialOutputFrame::CompleteFrame {
                output_id: output_id.clone(),
                frame,
            });
            continue;
        }
        match root.output_texture(&scene.inputs).state() {
            Some(node) => match &output.output_texture {
                OutputTexture::PlanarYuvTextures(yuv_output) => {
                    stats.planar_yuv_outputs += 1;
                    stats.planar_yuv_passes += 1;
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
                    stats.rgba_copy_passes += 1;
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
                    stats.nv12_outputs += 1;
                    stats.nv12_conversion_passes += 2;
                    if nv12_output.resolution().width >= 3840
                        && nv12_output.resolution().height >= 2160
                    {
                        stats.intermediate_4k_textures += 1;
                    }
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
                OutputTexture::ExternalNv12WgpuTexture(external) => {
                    stats.nv12_outputs += 1;
                    stats.nv12_conversion_passes += 2;
                    // Zero-copy: record the convert into the shared external
                    // encoder and stage the dma-buf fence; the single batched
                    // submit below replaces the former per-output submit.
                    if let Some((texture, token)) = external.render_into_pool(
                        ctx.wgpu_ctx,
                        &mut external_encoder,
                        node.output_texture_bind_group(),
                    ) {
                        external_tokens.push((external.pool(), token));
                        let frame = Frame {
                            resolution: external.resolution(),
                            data: FrameData::Nv12WgpuTexture(texture),
                            pts,
                        };
                        partial_textures.push(PartialOutputFrame::CompleteFrame {
                            output_id: output_id.clone(),
                            frame,
                        })
                    }
                }
            },
            // fallback if root node in render graph is empty
            None => match &output.output_texture {
                OutputTexture::PlanarYuvTextures(yuv_output) => {
                    stats.planar_yuv_outputs += 1;
                    stats.planar_yuv_passes += 1;
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
                    stats.rgba_copy_passes += 1;
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
                    stats.nv12_outputs += 1;
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
                OutputTexture::ExternalNv12WgpuTexture(external) => {
                    stats.nv12_outputs += 1;
                    if let Some((texture, token)) =
                        external.fill_black(&mut external_encoder)
                    {
                        external_tokens.push((external.pool(), token));
                        let frame = Frame {
                            data: FrameData::Nv12WgpuTexture(texture),
                            resolution: external.resolution(),
                            pts,
                        };
                        partial_textures.push(PartialOutputFrame::CompleteFrame {
                            output_id: output_id.clone(),
                            frame,
                        });
                    }
                }
            },
        };
    }
    stats.encode_ms = encode_started.elapsed().as_secs_f64() * 1000.0;

    // Single batched submit for ALL external zero-copy outputs: one `queue.submit`
    // consumes every staged dma-buf fence at once, then each token is finished
    // (release fence imported into its dma-buf). This MUST precede the nv12_encoder
    // submit so it — and not that unrelated submit — consumes the staged fences.
    if !external_tokens.is_empty() {
        ctx.wgpu_ctx.queue.submit(Some(external_encoder.finish()));
        for (pool, token) in external_tokens {
            if let Err(err) = pool.finish_write(token) {
                error!("External NV12 pool write finish failed: {err}");
            }
        }
    }

    let submit_started = Instant::now();
    if nv12_conversions > 0 {
        if let Some(query) = &timestamp_query {
            nv12_encoder.write_timestamp(&query.query_set, 1);
            query.resolve(&mut nv12_encoder);
            stats.timestamp_queries = 1;
        }
        ctx.wgpu_ctx.queue.submit(Some(nv12_encoder.finish()));
        if let Some(query) = timestamp_query {
            query.read(ctx.wgpu_ctx, "output");
        }
    }
    stats.submit_ms = submit_started.elapsed().as_secs_f64() * 1000.0;
    stats.wait_ms = diagnostic_wait_for_gpu(ctx.wgpu_ctx);

    if partial_textures.iter().any(PartialOutputFrame::needs_download) {
        while let Err(wgpu::PollError::Timeout) =
            ctx.wgpu_ctx.device.poll(wgpu::PollType::wait_indefinitely())
        {
            warn!("Device poll failed.")
        }
    }

    let mut result = HashMap::new();
    let assemble_started = Instant::now();
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
    stats.assemble_ms = assemble_started.elapsed().as_secs_f64() * 1000.0;
    stats.total_ms = started.elapsed().as_secs_f64() * 1000.0;
    (result, stats)
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
) -> TransformStageStats {
    let started = Instant::now();
    let mut stats = TransformStageStats::default();
    let mut encoder =
        ctx.wgpu_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batched layout transforms"),
        });
    let timestamp_query =
        TimestampQuery::new(ctx.wgpu_ctx, transform_encoder_label());
    if let Some(query) = &timestamp_query {
        encoder.write_timestamp(&query.query_set, 0);
    }
    for output in scene.outputs.values_mut() {
        stats += render_node(ctx, &scene.inputs, pts, &mut output.root, &mut encoder);
    }
    if stats.layout_passes > 0 || stats.lanczos_passes > 0 {
        if let Some(query) = &timestamp_query {
            encoder.write_timestamp(&query.query_set, 1);
            query.resolve(&mut encoder);
            stats.timestamp_queries = 1;
        }
        let submit_started = Instant::now();
        ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
        if let Some(query) = timestamp_query {
            query.read(ctx.wgpu_ctx, "transform");
        }
        stats.submit_ms = submit_started.elapsed().as_secs_f64() * 1000.0;
        stats.wait_ms = diagnostic_wait_for_gpu(ctx.wgpu_ctx);
        stats.submits = 1;
    }
    stats.total_ms = started.elapsed().as_secs_f64() * 1000.0;
    stats
}

pub(super) fn render_node(
    ctx: &mut RenderCtx,
    inputs: &HashMap<InputId, (NodeTexture, InputTexture)>,
    pts: Duration,
    node: &mut RenderNode,
    encoder: &mut wgpu::CommandEncoder,
) -> TransformStageStats {
    let mut stats = TransformStageStats::default();
    for child_node in node.children.iter_mut() {
        stats += render_node(ctx, inputs, pts, child_node, encoder);
    }

    let input_textures: Vec<_> =
        node.children.iter().map(|node| node.output_texture(inputs)).collect();
    stats += node.renderer.render(ctx, &input_textures, &mut node.output, pts, encoder).into();
    stats
}

impl std::ops::AddAssign for TransformStageStats {
    fn add_assign(&mut self, rhs: Self) {
        self.total_ms += rhs.total_ms;
        self.submit_ms += rhs.submit_ms;
        self.wait_ms += rhs.wait_ms;
        self.timestamp_queries += rhs.timestamp_queries;
        self.layout_ms += rhs.layout_ms;
        self.submits += rhs.submits;
        self.lanczos_passes += rhs.lanczos_passes;
        self.layout_passes += rhs.layout_passes;
        self.intermediate_4k_textures += rhs.intermediate_4k_textures;
    }
}

impl From<NodeRenderStats> for TransformStageStats {
    fn from(stats: NodeRenderStats) -> Self {
        Self {
            total_ms: 0.0,
            submit_ms: 0.0,
            wait_ms: 0.0,
            timestamp_queries: 0,
            layout_ms: stats.layout_ms,
            submits: 0,
            lanczos_passes: stats.lanczos_passes,
            layout_passes: stats.layout_passes,
            intermediate_4k_textures: stats.intermediate_4k_textures,
        }
    }
}

fn transform_encoder_label() -> &'static str {
    "batched layout transforms"
}

fn nv12_encoder_label() -> &'static str {
    "batched NV12 output conversion"
}

struct TimestampQuery {
    query_set: wgpu::QuerySet,
    resolve_buffer: Arc<wgpu::Buffer>,
    readback_buffer: Arc<wgpu::Buffer>,
}

impl TimestampQuery {
    fn new(ctx: &crate::wgpu::WgpuCtx, label: &'static str) -> Option<Self> {
        if !ctx.device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }

        let query_set = ctx.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some(label),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve_buffer = Arc::new(ctx.device.create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("render timestamp resolve"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            },
        ));
        let readback_buffer = Arc::new(ctx.device.create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("render timestamp readback"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            },
        ));

        Some(Self { query_set, resolve_buffer, readback_buffer })
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.query_set, 0..2, &self.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &self.readback_buffer,
            0,
            16,
        );
    }

    fn read(self, ctx: &crate::wgpu::WgpuCtx, stage: &'static str) {
        // TEMP: readback/logging reverted to the clean-baseline discard behavior to measure the
        // production (non-perturbing) frame rate. Re-add a non-perturbing readback later.
        let _ = stage;
        let buffer = Arc::clone(&self.readback_buffer);
        let callback_buffer = Arc::clone(&buffer);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                callback_buffer.unmap();
            }
        });
        let _ = ctx.device.poll(wgpu::PollType::Poll);
    }
}

pub(super) fn diagnostic_wait_for_gpu(ctx: &crate::wgpu::WgpuCtx) -> f64 {
    let _ = ctx;
    0.0
}
