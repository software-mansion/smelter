use std::sync::mpsc;

use super::{
    display::quicksync_drm_render_nodes,
    va::VaDisplay,
    vpl::{Codec, Component, Session},
};
use wgpu::hal::api::Vulkan as VkApi;

#[derive(Debug, Clone, Copy)]
pub struct Rgb4VppSurfaceSharingProbe {
    pub render_node: u32,
    pub rgb4_va_surface_id: u32,
    pub nv12_va_surface_id: u32,
    pub rgb4_fourcc: u32,
    pub rgb4_width: u32,
    pub rgb4_height: u32,
    pub rgb4_objects: usize,
    pub rgb4_layers: usize,
    pub rgb4_planes: usize,
    pub rgb4_pitch: u32,
    pub rgb4_modifier: u64,
    pub rgb4_single_plane: bool,
    pub rgb4_wgpu_import: bool,
    pub rgb4_wgpu_roundtrip: bool,
    pub nv12_fourcc: u32,
    pub nv12_objects: usize,
    pub nv12_layers: usize,
    pub nv12_planes: usize,
}

pub fn probe_rgb4_vpp_surface_sharing(
    adapter_info: &wgpu::AdapterInfo,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Rgb4VppSurfaceSharingProbe, String> {
    let mut last_error = "no Intel Quick Sync DRM render node found".to_string();
    for drm_node in quicksync_drm_render_nodes(adapter_info).iter() {
        match probe_rgb4_vpp_surface_sharing_for_node(drm_node, device, queue) {
            Ok(probe) => return Ok(probe),
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

fn probe_rgb4_vpp_surface_sharing_for_node(
    drm_node: &super::display::DrmRenderNode,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Rgb4VppSurfaceSharingProbe, String> {
    let display = VaDisplay::open(&drm_node.path).map_err(|err| err.to_string())?;
    let session = Session::new(
        drm_node.render_node,
        Codec::H264,
        Component::VppInput,
        display.handle(),
    )
    .map_err(|err| err.to_string())?;
    session.init_vpp_rgb4_to_nv12(640, 368, 640, 360).map_err(|err| err.to_string())?;

    let rgb4_input =
        session.get_surface_for_vpp_input().map_err(|err| err.to_string())?;
    let nv12_output =
        session.get_surface_for_vpp_output().map_err(|err| err.to_string())?;
    let exported_rgb4 =
        session.export_va_surface(&rgb4_input).map_err(|err| err.to_string())?;
    let exported_nv12 =
        session.export_va_surface(&nv12_output).map_err(|err| err.to_string())?;
    let rgb4_layout = display
        .export_surface_layout(exported_rgb4.va_surface_id())
        .map_err(|err| err.to_string())?;
    let nv12_layout = display
        .export_surface_layout(exported_nv12.va_surface_id())
        .map_err(|err| err.to_string())?;
    let rgb4_planes = rgb4_layout.layers.iter().map(|layer| layer.planes.len()).sum();
    let nv12_planes = nv12_layout.layers.iter().map(|layer| layer.planes.len()).sum();
    let rgb4_plane = rgb4_layout
        .layers
        .first()
        .and_then(|layer| layer.planes.first())
        .ok_or_else(|| "RGB4 VPP input exported without planes".to_string())?;
    let rgb4_object = rgb4_layout
        .objects
        .first()
        .ok_or_else(|| "RGB4 VPP input exported without objects".to_string())?;
    let rgb4_dma_buf = display
        .export_single_plane_surface(exported_rgb4.va_surface_id())
        .map_err(|err| err.to_string())?;
    if rgb4_dma_buf.fourcc != rgb4_layout.fourcc
        || rgb4_dma_buf.width != rgb4_layout.width
        || rgb4_dma_buf.height != rgb4_layout.height
        || rgb4_dma_buf.pitch != rgb4_plane.pitch
        || rgb4_dma_buf.modifier != rgb4_object.modifier
    {
        return Err("RGB4 layout changed between VA exports".into());
    }
    let imported_rgb4 = import_rgb4_dma_buf_texture(device, rgb4_dma_buf)?;
    let rgb4_wgpu_roundtrip = probe_rgb4_wgpu_roundtrip(device, queue, &imported_rgb4)?;

    Ok(Rgb4VppSurfaceSharingProbe {
        render_node: drm_node.render_node,
        rgb4_va_surface_id: exported_rgb4.va_surface_id(),
        nv12_va_surface_id: exported_nv12.va_surface_id(),
        rgb4_fourcc: rgb4_layout.fourcc,
        rgb4_width: rgb4_layout.width,
        rgb4_height: rgb4_layout.height,
        rgb4_objects: rgb4_layout.objects.len(),
        rgb4_layers: rgb4_layout.layers.len(),
        rgb4_planes,
        rgb4_pitch: rgb4_plane.pitch,
        rgb4_modifier: rgb4_object.modifier,
        rgb4_single_plane: rgb4_layout.is_single_plane(),
        rgb4_wgpu_import: true,
        rgb4_wgpu_roundtrip,
        nv12_fourcc: nv12_layout.fourcc,
        nv12_objects: nv12_layout.objects.len(),
        nv12_layers: nv12_layout.layers.len(),
        nv12_planes,
    })
}

fn import_rgb4_dma_buf_texture(
    device: &wgpu::Device,
    dma_buf: super::va::DrmPrimeSinglePlaneSurface,
) -> Result<wgpu::Texture, String> {
    if dma_buf.fourcc.to_le_bytes() != *b"ARGB" {
        return Err(format!(
            "expected VA RGB4 to export as ARGB DRM fourcc, got {:?}",
            dma_buf.fourcc.to_le_bytes()
        ));
    }
    let size = wgpu::Extent3d {
        width: dma_buf.width,
        height: dma_buf.height,
        depth_or_array_layers: 1,
    };
    let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC;
    let hal_usage = wgpu::TextureUses::COPY_DST | wgpu::TextureUses::COPY_SRC;
    let hal_texture = unsafe {
        let hal_device = device.as_hal::<VkApi>().ok_or_else(|| {
            "RGB4 DMA-BUF import requires a Vulkan wgpu device".to_string()
        })?;
        (*hal_device)
            .texture_from_dmabuf_fd(
                dma_buf.fd,
                &wgpu::hal::TextureDescriptor {
                    label: Some("RGB4 VPP input DMA-BUF import"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: hal_usage,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                dma_buf.modifier,
                u64::from(dma_buf.pitch),
                u64::from(dma_buf.offset),
            )
            .map_err(|err| {
                format!("failed to import RGB4 DMA-BUF into wgpu-hal: {err}")
            })?
    };
    Ok(unsafe {
        device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("RGB4 VPP input DMA-BUF import"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage,
                view_formats: &[],
            },
            wgpu::TextureUses::COPY_DST,
        )
    })
}

fn probe_rgb4_wgpu_roundtrip(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<bool, String> {
    let extent = wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 };
    let pixel = [17, 41, 83, 255];
    let mut data = vec![0; (extent.width * extent.height * 4) as usize];
    for chunk in data.chunks_exact_mut(4) {
        chunk.copy_from_slice(&pixel);
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(extent.width * 4),
            rows_per_image: Some(extent.height),
        },
        extent,
    );

    let padded_row_bytes = pad_to_256(extent.width * 4);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("RGB4 VPP input DMA-BUF readback"),
        size: u64::from(padded_row_bytes) * u64::from(extent.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("RGB4 VPP input DMA-BUF roundtrip"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
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
        .map_err(|err| format!("failed to wait for RGB4 DMA-BUF roundtrip: {err}"))?;
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("failed to map RGB4 DMA-BUF readback: {err}"))?;
    rx.recv()
        .map_err(|err| format!("failed to receive RGB4 DMA-BUF map result: {err}"))?
        .map_err(|err| format!("failed to map RGB4 DMA-BUF readback: {err}"))?;
    let mapped = slice
        .get_mapped_range()
        .map_err(|err| format!("failed to read RGB4 DMA-BUF mapped range: {err}"))?;
    let matches = mapped
        .chunks(padded_row_bytes as usize)
        .take(extent.height as usize)
        .all(|row| {
            row[..(extent.width * 4) as usize].chunks_exact(4).all(|px| px == pixel)
        });
    drop(mapped);
    buffer.unmap();
    Ok(matches)
}

fn pad_to_256(value: u32) -> u32 {
    (value + 255) & !255
}
