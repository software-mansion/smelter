use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use crate::{
    VideoResolution,
    dmabuf::{
        DmaBufFrame, DmaBufLayer, DmaBufObject, DmaBufPlane, import_nv12_dmabuf_texture,
    },
};
use libva::{
    Display, DrmPrimeSurfaceDescriptor, PictureH264, Surface, UsageHint, VA_INVALID_ID,
    VA_PICTURE_H264_INVALID,
};

const DEFAULT_DRM_RENDER_NODE: &str = "/dev/dri/renderD128";
const DRM_BY_PATH_DIR: &str = "/dev/dri/by-path";

pub(crate) fn open_display(
    adapter_info: Option<&wgpu::AdapterInfo>,
) -> Result<Rc<Display>, String> {
    let paths = vaapi_drm_paths(adapter_info);
    for path in &paths {
        match Display::open_drm_display(path) {
            Ok(display) => return Ok(display),
            Err(err) => {
                tracing::error!("Failed to open VA-API DRM display {path}: {err}")
            }
        }
    }
    Err(no_usable_drm_display_error(&paths))
}

pub(crate) fn export_surface_as_frame(
    device: &wgpu::Device,
    surface: &Surface<()>,
) -> Result<Arc<DmaBufFrame>, String> {
    let descriptor = surface
        .export_prime()
        .map_err(|err| format!("failed to export VA surface: {err}"))?;
    import_drm_prime_surface(device, descriptor)
}

pub(crate) fn take_nv12_surface(
    display: &Rc<Display>,
    free_surfaces: &mut Vec<Surface<()>>,
    resolution: VideoResolution,
    usage_hint: UsageHint,
    batch_size: usize,
    label: &str,
) -> Result<Surface<()>, String> {
    if let Some(surface) = free_surfaces.pop() {
        return Ok(surface);
    }

    let mut surfaces = display
        .create_surfaces(
            libva::VA_RT_FORMAT_YUV420,
            Some(libva::VA_FOURCC_NV12),
            resolution.width,
            resolution.height,
            Some(usage_hint),
            vec![(); batch_size],
        )
        .map_err(|err| format!("failed to create VA-API {label} surfaces: {err}"))?;
    let surface =
        surfaces.pop().ok_or_else(|| format!("VA-API returned no {label} surface"))?;
    free_surfaces.extend(surfaces);
    Ok(surface)
}

pub(crate) fn invalid_h264_pictures<const N: usize>() -> [PictureH264; N] {
    std::array::from_fn(|_| {
        PictureH264::new(VA_INVALID_ID, 0, VA_PICTURE_H264_INVALID, 0, 0)
    })
}

pub(crate) fn import_drm_prime_surface(
    device: &wgpu::Device,
    descriptor: DrmPrimeSurfaceDescriptor,
) -> Result<Arc<DmaBufFrame>, String> {
    let objects: Vec<DmaBufObject> = descriptor
        .objects
        .into_iter()
        .map(|object| DmaBufObject {
            fd: Arc::new(object.fd),
            size: object.size,
            modifier: object.drm_format_modifier,
        })
        .collect();
    let layers: Vec<DmaBufLayer> = descriptor
        .layers
        .into_iter()
        .map(|layer| {
            dmabuf_layer_from_prime_parts(
                layer.drm_format,
                layer.num_planes,
                layer.object_index.map(|index| index as usize),
                layer.offset,
                layer.pitch,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;

    import_nv12_dmabuf_texture(
        device,
        descriptor.fourcc,
        descriptor.width,
        descriptor.height,
        objects,
        layers,
    )
    .map_err(|err| err.to_string())
}

fn dmabuf_layer_from_prime_parts(
    drm_format: u32,
    num_planes: u32,
    object_index: [usize; 4],
    offset: [u32; 4],
    pitch: [u32; 4],
) -> Result<DmaBufLayer, String> {
    let plane_count = num_planes as usize;
    if plane_count > 4 {
        return Err(format!(
            "DRM PRIME layer plane count {plane_count} exceeds VA-API descriptor limit"
        ));
    }
    Ok(DmaBufLayer {
        drm_format,
        planes: (0..plane_count)
            .map(|index| DmaBufPlane {
                object_index: object_index[index],
                offset: offset[index],
                pitch: pitch[index],
            })
            .collect(),
    })
}

fn vaapi_drm_paths(adapter_info: Option<&wgpu::AdapterInfo>) -> Vec<String> {
    vaapi_drm_paths_from(
        std::env::var("SMELTER_VAAPI_DRM_DEVICE").ok().filter(|path| !path.is_empty()),
        adapter_info.and_then(matching_adapter_render_node),
        discover_drm_render_nodes(),
    )
}

fn matching_adapter_render_node(adapter_info: &wgpu::AdapterInfo) -> Option<String> {
    let pci_bus_id = adapter_info.device_pci_bus_id.trim();
    if pci_bus_id.is_empty() {
        return None;
    }

    drm_by_path_render_node(pci_bus_id).or_else(|| drm_sysfs_render_node(pci_bus_id))
}

fn drm_by_path_render_node(pci_bus_id: &str) -> Option<String> {
    canonicalize_drm_path(
        Path::new(DRM_BY_PATH_DIR).join(format!("pci-{pci_bus_id}-render")),
    )
}

fn drm_sysfs_render_node(pci_bus_id: &str) -> Option<String> {
    let drm_dir = Path::new("/sys/bus/pci/devices").join(pci_bus_id).join("drm");
    std::fs::read_dir(drm_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            name.starts_with("renderD").then(|| format!("/dev/dri/{name}"))
        })
        .min()
}

fn canonicalize_drm_path(path: PathBuf) -> Option<String> {
    Some(std::fs::canonicalize(path).ok()?.to_string_lossy().into_owned())
}

fn discover_drm_render_nodes() -> Vec<String> {
    let mut paths = std::fs::read_dir("/dev/dri")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let index = entry
                .file_name()
                .to_str()?
                .strip_prefix("renderD")?
                .parse::<u32>()
                .ok()?;
            let path = entry.path().to_str()?.to_string();
            Some((index, path))
        })
        .collect::<Vec<_>>();

    paths.sort_by_key(|(index, _)| *index);
    paths.into_iter().map(|(_, path)| path).collect()
}

fn vaapi_drm_paths_from(
    configured: Option<String>,
    matched_adapter: Option<String>,
    discovered: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(path) = configured {
        push_unique_drm_path(&mut paths, path);
    }
    if let Some(path) = matched_adapter {
        push_unique_drm_path(&mut paths, path);
    }
    for path in discovered {
        push_unique_drm_path(&mut paths, path);
    }
    if paths.is_empty() {
        paths.push(DEFAULT_DRM_RENDER_NODE.to_string());
    }
    paths
}

fn push_unique_drm_path(paths: &mut Vec<String>, path: String) {
    if !path.is_empty() && !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn no_usable_drm_display_error(paths: &[String]) -> String {
    format!("no usable DRM display found in {}", paths.join(", "))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn drm_paths_keep_configured_device_first() {
        let paths = vaapi_drm_paths_from(
            Some("/dev/dri/renderD129".into()),
            Some("/dev/dri/renderD130".into()),
            ["/dev/dri/renderD128".into(), "/dev/dri/renderD129".into()],
        );

        assert_eq!(
            paths,
            vec!["/dev/dri/renderD129", "/dev/dri/renderD130", "/dev/dri/renderD128"]
        );
    }

    #[test]
    fn drm_paths_use_default_when_no_render_nodes_are_discovered() {
        let paths = vaapi_drm_paths_from(None, None, []);

        assert_eq!(paths, vec![DEFAULT_DRM_RENDER_NODE]);
    }
}
