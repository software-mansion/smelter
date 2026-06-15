use std::sync::mpsc;

use crate::{
    VideoResolution,
    dmabuf::{DmaBufInterop, QuickSyncDmaBufSync},
};

use super::Nv12Plane;

pub(super) fn probe_nv12_dmabuf_wgpu_roundtrip(
    device: &wgpu::Device,
    interop: &DmaBufInterop,
    sync: &QuickSyncDmaBufSync,
    queue: &wgpu::Queue,
) -> Result<(), String> {
    let resolution = VideoResolution {
        width: 64,
        height: 64,
    };
    let source_size = resolution.extent_2d();
    let source = nv12_probe_texture(device, "NV12 DMA-BUF probe source", source_size);
    write_solid_nv12_texture(queue, &source, 63, 91, 177);
    queue.submit([]);
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("failed to wait for NV12 DMA-BUF probe source upload: {err}"))?;

    let source_pixels =
        read_nv12_texture(device, queue, &source, "NV12 DMA-BUF probe source readback")?;
    let exported = interop
        .export_nv12_texture(resolution)
        .map_err(|err| format!("failed to export NV12 DMA-BUF texture: {err}"))?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("NV12 DMA-BUF probe upload"),
    });
    encoder.copy_texture_to_texture(
        source.as_image_copy(),
        exported.texture().as_image_copy(),
        source_size,
    );
    sync.submit_dma_buf_write(&exported, encoder, "NV12 DMA-BUF probe export copy")
        .map_err(|err| err.to_string())?;

    let imported = interop
        .import_nv12_texture(exported.descriptor())
        .map_err(|err| format!("failed to import exported NV12 DMA-BUF texture: {err}"))?;
    let scratch = nv12_probe_texture(device, "NV12 DMA-BUF probe scratch", source_size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("NV12 DMA-BUF probe download"),
    });
    encoder.copy_texture_to_texture(
        imported.texture().as_image_copy(),
        scratch.as_image_copy(),
        source_size,
    );
    sync.submit_dma_buf_read(&imported, encoder, "NV12 DMA-BUF probe import copy")
        .map_err(|err| err.to_string())?;

    let roundtrip_pixels = read_nv12_texture(
        device,
        queue,
        &scratch,
        "NV12 DMA-BUF probe scratch readback",
    )?;
    if source_pixels != roundtrip_pixels {
        return Err("NV12 DMA-BUF probe roundtrip changed texture contents".into());
    }
    Ok(())
}

fn nv12_probe_texture(
    device: &wgpu::Device,
    label: &'static str,
    size: wgpu::Extent3d,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_solid_nv12_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, y: u8, u: u8, v: u8) {
    let resolution = texture_resolution(texture);

    let y_plane = vec![y; plane_byte_len(Nv12Plane::Y, resolution)];
    write_nv12_plane(queue, texture, Nv12Plane::Y, &y_plane);

    let mut uv_plane = vec![0; plane_byte_len(Nv12Plane::Uv, resolution)];
    for pixel in uv_plane.chunks_exact_mut(2) {
        pixel[0] = u;
        pixel[1] = v;
    }
    write_nv12_plane(queue, texture, Nv12Plane::Uv, &uv_plane);
}

fn write_nv12_plane(queue: &wgpu::Queue, texture: &wgpu::Texture, plane: Nv12Plane, data: &[u8]) {
    let extent = plane.extent(texture_resolution(texture));
    let row_bytes = extent.width * plane.bytes_per_texel();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: plane.aspect(),
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(row_bytes),
            rows_per_image: Some(extent.height),
        },
        extent,
    );
}

fn read_nv12_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    label: &str,
) -> Result<Vec<u8>, String> {
    let resolution = texture_resolution(texture);
    let mut output = Vec::with_capacity(
        Nv12Plane::ALL
            .into_iter()
            .map(|plane| plane_byte_len(plane, resolution))
            .sum(),
    );
    for plane in Nv12Plane::ALL {
        output.extend(read_nv12_plane(device, queue, texture, plane, label)?);
    }
    Ok(output)
}

fn read_nv12_plane(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    plane: Nv12Plane,
    label: &str,
) -> Result<Vec<u8>, String> {
    let extent = plane.extent(texture_resolution(texture));
    let row_bytes = extent.width * plane.bytes_per_texel();
    let padded_row_bytes = pad_to_256(row_bytes);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: u64::from(padded_row_bytes) * u64::from(extent.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: plane.aspect(),
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(extent.height),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("failed to wait for {label}: {err}"))?;

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).ok();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("failed to map {label}: {err}"))?;
    receiver
        .recv()
        .map_err(|_| format!("failed to receive {label} readback result"))?
        .map_err(|err| format!("failed to map {label}: {err}"))?;

    let mapped = slice
        .get_mapped_range()
        .map_err(|err| format!("failed to read {label}: {err}"))?;
    let mut output = Vec::with_capacity((row_bytes * extent.height) as usize);
    for row in mapped
        .chunks(padded_row_bytes as usize)
        .take(extent.height as usize)
    {
        output.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(output)
}

fn pad_to_256(value: u32) -> u32 {
    (value + 255) & !255
}

fn texture_resolution(texture: &wgpu::Texture) -> VideoResolution {
    let size = texture.size();
    VideoResolution {
        width: size.width,
        height: size.height,
    }
}

fn plane_byte_len(plane: Nv12Plane, resolution: VideoResolution) -> usize {
    let extent = plane.extent(resolution);
    (extent.width * extent.height * plane.bytes_per_texel()) as usize
}
