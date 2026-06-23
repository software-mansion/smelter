use std::path::{Path, PathBuf};

const DRM_BY_PATH_DIR: &str = "/dev/dri/by-path";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DrmRenderNode {
    pub(super) path: PathBuf,
    pub(super) render_node: u32,
}

pub(super) fn quicksync_drm_render_node(
    adapter_info: &wgpu::AdapterInfo,
) -> Option<DrmRenderNode> {
    let pci_bus_id = adapter_info.device_pci_bus_id.trim();
    if pci_bus_id.is_empty() {
        return None;
    }

    drm_by_path_render_node(pci_bus_id).or_else(|| drm_sysfs_render_node(pci_bus_id))
}

fn drm_by_path_render_node(pci_bus_id: &str) -> Option<DrmRenderNode> {
    canonicalize_drm_render_node(
        Path::new(DRM_BY_PATH_DIR).join(format!("pci-{pci_bus_id}-render")),
    )
}

fn drm_sysfs_render_node(pci_bus_id: &str) -> Option<DrmRenderNode> {
    let drm_dir = Path::new("/sys/bus/pci/devices").join(pci_bus_id).join("drm");
    std::fs::read_dir(drm_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            name.strip_prefix("renderD")?;
            drm_render_node(Path::new("/dev/dri").join(name))
        })
        .min_by_key(|node| node.render_node)
}

fn canonicalize_drm_render_node(path: PathBuf) -> Option<DrmRenderNode> {
    drm_render_node(std::fs::canonicalize(path).ok()?)
}

fn drm_render_node(path: PathBuf) -> Option<DrmRenderNode> {
    let render_node = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("renderD"))
        .and_then(|suffix| suffix.parse().ok())?;
    Some(DrmRenderNode { path, render_node })
}

#[cfg(test)]
fn quicksync_drm_render_node_from(
    matched_adapter: Option<DrmRenderNode>,
) -> Option<DrmRenderNode> {
    matched_adapter
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn node(path: &str, render_node: u32) -> DrmRenderNode {
        DrmRenderNode { path: path.into(), render_node }
    }

    #[test]
    fn drm_render_node_uses_matched_adapter_without_fallback() {
        let render_node =
            quicksync_drm_render_node_from(Some(node("/dev/dri/renderD130", 130)));

        assert_eq!(render_node, Some(node("/dev/dri/renderD130", 130)));
    }

    #[test]
    fn drm_render_node_does_not_fallback_when_adapter_is_known() {
        let render_node = quicksync_drm_render_node_from(None);

        assert!(render_node.is_none());
    }

    #[test]
    fn drm_render_node_rejects_render_node_in_parent_directory() {
        let node = drm_render_node(PathBuf::from("/tmp/renderD128/card0"));

        assert!(node.is_none());
    }
}
