use std::{
    collections::HashMap,
    fmt::{self, Display},
    sync::Arc,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::os::fd::OwnedFd;

#[cfg(target_os = "linux")]
pub const DRM_FORMAT_NV12: u32 = u32::from_le_bytes(*b"NV12");

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenderingMode {
    // - Leverage multiple views per texture
    // - Color blending in linear space
    GpuOptimized,
    // - Color blending in sRGB space
    CpuOptimized,
    // - Single view per texture
    // - Color blending in linear space (but requires additional processing)
    WebGl,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub data: FrameData,
    pub resolution: Resolution,
    pub pts: Duration,
}

#[derive(Debug, Clone)]
pub enum FrameData {
    PlanarYuv420(YuvPlanes),
    PlanarYuv422(YuvPlanes),
    PlanarYuv444(YuvPlanes),
    PlanarYuvJ420(YuvPlanes),
    InterleavedUyvy422(bytes::Bytes),
    InterleavedYuyv422(bytes::Bytes),
    Rgba8UnormWgpuTexture(Arc<wgpu::Texture>),
    Nv12WgpuTexture(Arc<wgpu::Texture>),
    #[cfg(target_os = "linux")]
    Nv12DmaBuf(Arc<DmaBufFrame>),
    Nv12(NvPlanes),
    Bgra(bytes::Bytes),
    Argb(bytes::Bytes),
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct DmaBufFrame {
    fourcc: u32,
    width: u32,
    height: u32,
    objects: Vec<DmaBufObject>,
    layers: Vec<DmaBufLayer>,
    texture: Arc<wgpu::Texture>,
    _owner: Option<Arc<dyn Send + Sync>>,
}

#[cfg(target_os = "linux")]
impl DmaBufFrame {
    pub(crate) fn new_with_owner(
        texture: Arc<wgpu::Texture>,
        fourcc: u32,
        width: u32,
        height: u32,
        objects: Vec<DmaBufObject>,
        layers: Vec<DmaBufLayer>,
        owner: Option<Arc<dyn Send + Sync>>,
    ) -> Self {
        assert!(
            !objects.is_empty() && objects.len() <= 4,
            "DMA-BUF frame must have 1..=4 objects"
        );
        assert!(
            !layers.is_empty() && layers.len() <= 4,
            "DMA-BUF frame must have 1..=4 layers"
        );
        for layer in &layers {
            assert!(
                !layer.planes.is_empty() && layer.planes.len() <= 4,
                "DMA-BUF layer must have 1..=4 planes"
            );
            for plane in &layer.planes {
                assert!(
                    plane.object_index < objects.len(),
                    "DMA-BUF plane references a missing object"
                );
                assert!(
                    plane.offset <= objects[plane.object_index].size,
                    "DMA-BUF plane offset exceeds object size"
                );
            }
        }
        Self { fourcc, width, height, objects, layers, texture, _owner: owner }
    }

    pub(crate) fn texture_arc(&self) -> Arc<wgpu::Texture> {
        Arc::clone(&self.texture)
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn fourcc(&self) -> u32 {
        self.fourcc
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn resolution(&self) -> Resolution {
        Resolution { width: self.width as usize, height: self.height as usize }
    }

    pub fn objects(&self) -> &[DmaBufObject] {
        &self.objects
    }

    pub fn layers(&self) -> &[DmaBufLayer] {
        &self.layers
    }
}

#[cfg(target_os = "linux")]
pub fn validate_nv12_dmabuf_frame(
    frame: &DmaBufFrame,
    expected_resolution: Resolution,
) -> Result<(), String> {
    if frame.resolution() != expected_resolution {
        return Err(format!(
            "expected NV12 DMA-BUF resolution {:?}, got {:?}",
            expected_resolution,
            frame.resolution()
        ));
    }

    validate_nv12_dmabuf_layout(
        frame.fourcc(),
        frame.width(),
        frame.height(),
        frame.objects(),
        frame.layers(),
    )
}

#[cfg(target_os = "linux")]
pub fn validate_nv12_dmabuf_layout(
    fourcc: u32,
    width: u32,
    height: u32,
    objects: &[DmaBufObject],
    layers: &[DmaBufLayer],
) -> Result<(), String> {
    if fourcc != DRM_FORMAT_NV12 {
        return Err(format!(
            "expected NV12 DMA-BUF fourcc {DRM_FORMAT_NV12}, got {fourcc}"
        ));
    }
    if width == 0 || height == 0 {
        return Err(format!("NV12 DMA-BUF has invalid size {width}x{height}"));
    }
    if objects.is_empty() || objects.len() > 4 {
        return Err(format!(
            "NV12 DMA-BUF object count {} is outside supported limit 1..=4",
            objects.len()
        ));
    }
    if layers.len() != 1 {
        return Err(format!("NV12 DMA-BUF requires one layer, got {}", layers.len()));
    }

    let layer = &layers[0];
    if layer.drm_format != DRM_FORMAT_NV12 {
        return Err(format!(
            "expected NV12 DMA-BUF layer drm format {DRM_FORMAT_NV12}, got {}",
            layer.drm_format
        ));
    }
    if layer.planes.len() != 2 {
        return Err(format!(
            "NV12 DMA-BUF requires two planes, got {}",
            layer.planes.len()
        ));
    }

    validate_nv12_plane("Y", &layer.planes[0], objects, width, height)?;
    validate_nv12_plane("UV", &layer.planes[1], objects, width, height.div_ceil(2))
}

#[cfg(target_os = "linux")]
fn validate_nv12_plane(
    name: &str,
    plane: &DmaBufPlane,
    objects: &[DmaBufObject],
    min_pitch: u32,
    rows: u32,
) -> Result<(), String> {
    let object = objects.get(plane.object_index).ok_or_else(|| {
        format!(
            "NV12 DMA-BUF {name} plane references object {}, but only {} objects exist",
            plane.object_index,
            objects.len()
        )
    })?;
    if plane.pitch < min_pitch {
        return Err(format!(
            "NV12 DMA-BUF {name} plane pitch {} is smaller than required width {min_pitch}",
            plane.pitch
        ));
    }
    let plane_end = u64::from(plane.offset)
        .checked_add(u64::from(plane.pitch) * u64::from(rows))
        .ok_or_else(|| format!("NV12 DMA-BUF {name} plane byte range overflows"))?;
    if plane_end > u64::from(object.size) {
        return Err(format!(
            "NV12 DMA-BUF {name} plane range {plane_end} exceeds object {} size {}",
            plane.object_index, object.size
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
impl fmt::Debug for DmaBufFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF frame")
            .field("fourcc", &self.fourcc)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("objects", &self.objects)
            .field("layers", &self.layers)
            .finish()
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct DmaBufObject {
    pub fd: Arc<OwnedFd>,
    pub size: u32,
    pub modifier: u64,
}

#[cfg(target_os = "linux")]
impl fmt::Debug for DmaBufObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF object")
            .field("size", &self.size)
            .field("modifier", &self.modifier)
            .finish()
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufLayer {
    pub drm_format: u32,
    pub planes: Vec<DmaBufPlane>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufPlane {
    pub object_index: usize,
    pub offset: u32,
    pub pitch: u32,
}

#[derive(Clone)]
pub struct YuvPlanes {
    pub y_plane: bytes::Bytes,
    pub u_plane: bytes::Bytes,
    pub v_plane: bytes::Bytes,
}

impl fmt::Debug for YuvPlanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planar YUV data")
            .field("y_plane", &format!("len={}", self.y_plane.len()))
            .field("u_plane", &format!("len={}", self.u_plane.len()))
            .field("v_plane", &format!("len={}", self.v_plane.len()))
            .finish()
    }
}

#[derive(Clone)]
pub struct NvPlanes {
    pub y_plane: bytes::Bytes,
    pub uv_planes: bytes::Bytes,
}

impl fmt::Debug for NvPlanes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Planar NV data")
            .field("y_plane", &format!("len={}", self.y_plane.len()))
            .field("uv_planes", &format!("len={}", self.uv_planes.len()))
            .finish()
    }
}

#[derive(Debug)]
pub struct FrameSet<Id>
where
    Id: From<Arc<str>>,
{
    pub frames: HashMap<Id, Frame>,
    pub pts: Duration,
}

impl<Id> FrameSet<Id>
where
    Id: From<Arc<str>>,
{
    pub fn new(pts: Duration) -> Self {
        FrameSet {
            frames: HashMap::new(),
            pts,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framerate {
    pub num: u32,
    pub den: u32,
}

impl Framerate {
    pub fn get_interval_duration(self) -> Duration {
        Duration::from_nanos(1_000_000_000u64 * self.den as u64 / self.num as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RendererId(pub Arc<str>);

impl Display for RendererId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InputId(pub Arc<str>);

impl Display for InputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<str>> for InputId {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct OutputId(pub Arc<str>);

impl Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<str>> for OutputId {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

pub const MAX_NODE_RESOLUTION: Resolution = Resolution {
    width: 7682,
    height: 4320,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub width: usize,
    pub height: usize,
}

impl Resolution {
    pub const ZERO: Self = Resolution {
        width: 0,
        height: 0,
    };

    pub const ONE_PIXEL: Self = Resolution {
        width: 1,
        height: 1,
    };

    pub const MIN_2X2: Self = Resolution {
        width: 2,
        height: 2,
    };

    pub fn ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

impl From<wgpu::Extent3d> for Resolution {
    fn from(value: wgpu::Extent3d) -> Self {
        Self {
            width: value.width as usize,
            height: value.height as usize,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFrameFormat {
    PlanarYuv420Bytes,
    PlanarYuv422Bytes,
    PlanarYuv444Bytes,
    RgbaWgpuTexture,
    Nv12WgpuTexture,
    #[cfg(target_os = "linux")]
    Nv12DmaBuf,
}
