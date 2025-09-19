use std::io::Write;

use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;

pub fn read_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> bytes::Bytes {
    let buffer = new_download_buffer(device, texture);

    let mut encoder = device.create_command_encoder(&Default::default());
    copy_to_buffer(&mut encoder, texture, &buffer);
    queue.submit(Some(encoder.finish()));

    download_buffer(device, texture.size(), &buffer)
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
    if value.is_multiple_of(256) {
        value
    } else {
        value + (256 - (value % 256))
    }
}
