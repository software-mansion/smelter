use core::panic;
use std::{io::Write, sync::OnceLock, time::Duration};

use bytes::BufMut;
use compositor_render::{
    create_wgpu_ctx, web_renderer::WebRendererInitOptions, Frame, FrameData, Framerate, Renderer,
    RendererOptions, RenderingMode, WgpuComponents, YuvPlanes,
};
use crossbeam_channel::bounded;
use tracing::error;

pub const SNAPSHOTS_DIR_NAME: &str = "snapshot_tests/snapshots/render_snapshots";

pub(super) fn frame_to_rgba(frame: &Frame) -> Vec<u8> {
    match &frame.data {
        FrameData::PlanarYuv420(planes) => yuv_frame_to_rgba(frame, planes),
        FrameData::PlanarYuv422(planes) => yuv_frame_to_rgba(frame, planes),
        FrameData::PlanarYuv444(planes) => yuv_frame_to_rgba(frame, planes),
        FrameData::PlanarYuvJ420(_) => panic!("unsupported"),
        FrameData::InterleavedYuv422(_) => panic!("unsupported"),
        FrameData::Rgba8UnormWgpuTexture(texture) => read_rgba_texture(texture).to_vec(),
        FrameData::Nv12WgpuTexture(_) => panic!("unsupported"),
    }
}

pub(super) fn yuv_frame_to_rgba(frame: &Frame, planes: &YuvPlanes) -> Vec<u8> {
    let YuvPlanes {
        y_plane,
        u_plane,
        v_plane,
    } = planes;

    // Renderer can sometimes produce resolution that is not dividable by 2
    let corrected_width = frame.resolution.width - (frame.resolution.width % 2);
    let corrected_height = frame.resolution.height - (frame.resolution.height % 2);

    let mut rgba_data = Vec::with_capacity(y_plane.len() * 4);
    for (i, y_plane) in y_plane
        .chunks(frame.resolution.width)
        .enumerate()
        .take(corrected_height)
    {
        for (j, y) in y_plane.iter().enumerate().take(corrected_width) {
            let mut y = (*y) as f32;
            let mut u = u_plane[(i / 2) * (frame.resolution.width / 2) + (j / 2)] as f32;
            let mut v = v_plane[(i / 2) * (frame.resolution.width / 2) + (j / 2)] as f32;

            y = ((y - 16.0) / 0.858_823_54).clamp(0.0, 255.0);
            u = ((u - 16.0) / 0.878_431_4).clamp(0.0, 255.0);
            v = ((v - 16.0) / 0.878_431_4).clamp(0.0, 255.0);

            let r = (y + 1.5748 * (v - 128.0)).clamp(0.0, 255.0);
            let g = (y - 0.1873 * (u - 128.0) - 0.4681 * (v - 128.0)).clamp(0.0, 255.0);
            let b = (y + 1.8556 * (u - 128.0)).clamp(0.0, 255.0);
            rgba_data.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
        }
    }

    rgba_data
}

fn get_wgpu_ctx() -> WgpuComponents {
    static CTX: OnceLock<WgpuComponents> = OnceLock::new();
    CTX.get_or_init(|| {
        create_wgpu_ctx(false, Default::default(), Default::default(), None).unwrap()
    })
    .clone()
}

pub(super) fn create_renderer() -> Renderer {
    let wgpu_ctx = get_wgpu_ctx();
    let (renderer, _event_loop) = Renderer::new(RendererOptions {
        web_renderer: WebRendererInitOptions::Disable,
        framerate: Framerate { num: 30, den: 1 },
        stream_fallback_timeout: Duration::from_secs(3),
        load_system_fonts: false,
        rendering_mode: RenderingMode::GpuOptimized,
        device: wgpu_ctx.device,
        queue: wgpu_ctx.queue,
    })
    .unwrap();
    renderer
}

fn read_rgba_texture(texture: &wgpu::Texture) -> bytes::Bytes {
    let WgpuComponents { device, queue, .. } = get_wgpu_ctx();
    let buffer = new_download_buffer(&device, texture);

    let mut encoder = device.create_command_encoder(&Default::default());
    copy_to_buffer(&mut encoder, texture, &buffer);
    queue.submit(Some(encoder.finish()));

    download_buffer(&device, texture.size(), &buffer)
}

fn new_download_buffer(device: &wgpu::Device, texture: &wgpu::Texture) -> wgpu::Buffer {
    let size = texture.size();
    let block_size = texture.format().block_copy_size(None).unwrap();

    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("texture buffer"),
        mapped_at_creation: false,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        size: (pad_to_256(block_size * size.width) * size.height) as u64,
    })
}

fn copy_to_buffer(
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    buffer: &wgpu::Buffer,
) {
    let size = texture.size();
    let block_size = texture.format().block_copy_size(None).unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            texture,
        },
        wgpu::TexelCopyBufferInfo {
            buffer,
            layout: wgpu::TexelCopyBufferLayout {
                bytes_per_row: Some(pad_to_256(size.width * block_size)),
                rows_per_image: Some(size.height),
                offset: 0,
            },
        },
        size,
    );
}

fn download_buffer(
    device: &wgpu::Device,
    size: wgpu::Extent3d,
    source: &wgpu::Buffer,
) -> bytes::Bytes {
    let buffer = bytes::BytesMut::with_capacity((size.width * size.height * 4) as usize);
    let (s, r) = bounded(1);
    source
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            if let Err(err) = s.send(result) {
                error!("channel send error: {err}")
            }
        });

    device.poll(wgpu::MaintainBase::Wait).unwrap();

    r.recv().unwrap().unwrap();
    let mut buffer = buffer.writer();
    {
        let range = source.slice(..).get_mapped_range();
        let chunks = range.chunks(pad_to_256(size.width * 4) as usize);
        for chunk in chunks {
            buffer
                .write_all(&chunk[..(size.width * 4) as usize])
                .unwrap();
        }
    };
    source.unmap();
    buffer.into_inner().into()
}

fn pad_to_256(value: u32) -> u32 {
    if value % 256 == 0 {
        value
    } else {
        value + (256 - (value % 256))
    }
}
