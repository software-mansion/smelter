//! POC(dmabuf-import): proves the copy-elimination chain end to end on the
//! `miroir-bench` device (Intel Arc / Meteor Lake, Linux):
//!
//!   external NV12 dma-buf  ->  VA surface (vaCreateSurfaces, DRM_PRIME_2)
//!                          ->  mfxFrameSurface1 (ImportFrameSurface, SHARED)
//!                          ->  oneVPL H.264 encode  ->  Annex-B bitstream
//!
//! Phase 1 (`external_nv12_dmabuf_imports_into_va_and_encodes_h264`) allocates the
//! external dma-buf with `memfd` + `/dev/udmabuf` (LINEAR), the simplest allocator.
//!
//! Phase 2 (`rendered_nv12_dmabuf_imports_into_va_and_encodes_h264`) proves the
//! production-shaped link: a GPU-allocated, RENDERABLE NV12 dma-buf (tiled
//! modifier) that the compositor renders into via wgpu, then VA imports + oneVPL
//! encodes, with a decoded-content integrity check and a SHARED-vs-COPY report.
//!
//! This file is test-only and is the throwaway spike for the planned encoder
//! rewire; it does not touch the production encode path.

use std::{io::Write, mem::size_of, os::fd::AsRawFd, ptr};

use crate::{
    VideoResolution,
    dmabuf::{
        DmaBufInterop, RenderableNv12DmaBuf, export_renderable_nv12,
        probe_ccs_renderable_nv12,
    },
    quicksync::{
        create_wgpu_device,
        sys as vpl,
        supported_wgpu_features,
        va::{ExternalNv12DmaBuf, ExternalNv12DmaBufPlanes, VaDisplay},
        vpl::{Codec, Component, Session},
    },
};

const RENDER_NODE_PATH: &str = "/dev/dri/renderD128";
const RENDER_NODE_NUM: u32 = 128;
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const FRAMES: u64 = 8;
const DRM_FORMAT_MOD_LINEAR: u64 = 0;
const BITSTREAM_CAPACITY: usize = 1_500_000;

// --- udmabuf / memfd FFI (UDMABUF_CREATE = _IOW('u', 0x42, udmabuf_create)) ---

const IOC_NRSHIFT: u64 = 0;
const IOC_TYPESHIFT: u64 = 8;
const IOC_SIZESHIFT: u64 = 16;
const IOC_DIRSHIFT: u64 = 30;
const IOC_WRITE: u64 = 1;

const fn ioc(dir: u64, kind: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | ((kind as u64) << IOC_TYPESHIFT)
        | ((nr as u64) << IOC_NRSHIFT)
        | ((size as u64) << IOC_SIZESHIFT)) as libc::c_ulong
}

#[repr(C)]
struct UdmabufCreate {
    memfd: u32,
    flags: u32,
    offset: u64,
    size: u64,
}

const UDMABUF_FLAGS_CLOEXEC: u32 = 0x01;

/// A LINEAR NV12 dma-buf backed by a sealed `memfd` exposed through `/dev/udmabuf`.
struct UdmabufNv12 {
    dma_buf_fd: i32,
    size: u32,
    y_pitch: u32,
    uv_offset: u32,
    uv_pitch: u32,
}

impl UdmabufNv12 {
    fn allocate(width: u32, height: u32, fill_frame: u64) -> Result<Self, String> {
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
        let y_pitch = width;
        let uv_pitch = width;
        let uv_offset = y_pitch * height;
        let raw_size = u64::from(uv_offset + uv_pitch * (height / 2));
        let size = raw_size.div_ceil(page) * page;

        let memfd = unsafe {
            libc::memfd_create(
                c"poc-nv12".as_ptr(),
                libc::MFD_ALLOW_SEALING | libc::MFD_CLOEXEC,
            )
        };
        if memfd < 0 {
            return Err(errno("memfd_create"));
        }
        let memfd = OwnedRawFd(memfd);

        if unsafe { libc::ftruncate(memfd.0, size as libc::off_t) } != 0 {
            return Err(errno("ftruncate"));
        }
        if unsafe { libc::fcntl(memfd.0, libc::F_ADD_SEALS, libc::F_SEAL_SHRINK) } != 0 {
            return Err(errno("fcntl(F_SEAL_SHRINK)"));
        }

        fill_nv12(&memfd, size as usize, width, height, uv_offset, fill_frame)?;

        let mut create = UdmabufCreate {
            memfd: memfd.0 as u32,
            flags: UDMABUF_FLAGS_CLOEXEC,
            offset: 0,
            size,
        };
        let udmabuf = unsafe { libc::open(c"/dev/udmabuf".as_ptr(), libc::O_RDWR) };
        if udmabuf < 0 {
            return Err(errno("open(/dev/udmabuf)"));
        }
        let udmabuf = OwnedRawFd(udmabuf);
        let request = ioc(IOC_WRITE, b'u', 0x42, size_of::<UdmabufCreate>());
        let dma_buf_fd = unsafe { libc::ioctl(udmabuf.0, request, &mut create) };
        if dma_buf_fd < 0 {
            return Err(errno("ioctl(UDMABUF_CREATE)"));
        }

        Ok(Self {
            dma_buf_fd: dma_buf_fd as i32,
            size: size as u32,
            y_pitch,
            uv_offset,
            uv_pitch,
        })
    }

    fn external(&self, width: u32, height: u32) -> ExternalNv12DmaBuf {
        ExternalNv12DmaBuf {
            fd: self.dma_buf_fd,
            size: self.size,
            modifier: DRM_FORMAT_MOD_LINEAR,
            width,
            height,
            y_offset: 0,
            y_pitch: self.y_pitch,
            uv_offset: self.uv_offset,
            uv_pitch: self.uv_pitch,
        }
    }
}

impl Drop for UdmabufNv12 {
    fn drop(&mut self) {
        unsafe { libc::close(self.dma_buf_fd) };
    }
}

struct OwnedRawFd(i32);

impl Drop for OwnedRawFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

fn fill_nv12(
    memfd: &OwnedRawFd,
    size: usize,
    width: u32,
    height: u32,
    uv_offset: u32,
    frame: u64,
) -> Result<(), String> {
    let map = unsafe {
        libc::mmap(
            ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            memfd.0,
            0,
        )
    };
    if map == libc::MAP_FAILED {
        return Err(errno("mmap"));
    }
    let bytes = unsafe { std::slice::from_raw_parts_mut(map as *mut u8, size) };
    // Luma: a diagonal ramp that shifts each frame so the content is non-trivial.
    for y in 0..height as usize {
        for x in 0..width as usize {
            bytes[y * width as usize + x] = ((x + y + frame as usize * 16) & 0xff) as u8;
        }
    }
    // Chroma: neutral gray.
    bytes[uv_offset as usize..].iter_mut().for_each(|b| *b = 128);
    unsafe { libc::munmap(map, size) };
    Ok(())
}

fn errno(context: &str) -> String {
    format!("{context} failed: {}", std::io::Error::last_os_error())
}

// --- minimal oneVPL H.264 encoder setup ---

fn init_encoder(session: &Session, width: u32, height: u32) -> Result<(), String> {
    let mut frame_info = unsafe { std::mem::zeroed::<vpl::mfxFrameInfo>() };
    frame_info.FourCC = vpl::MFX_FOURCC_NV12;
    frame_info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
    frame_info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    frame_info.BitDepthLuma = 8;
    frame_info.BitDepthChroma = 8;
    frame_info.FrameRateExtN = 30;
    frame_info.FrameRateExtD = 1;
    unsafe {
        let dims = &mut frame_info.__bindgen_anon_1.__bindgen_anon_1;
        dims.Width = align16(width);
        dims.Height = align16(height);
        dims.CropW = width as u16;
        dims.CropH = height as u16;
    }

    let mut param = unsafe { std::mem::zeroed::<vpl::mfxVideoParam>() };
    param.IOPattern = vpl::MFX_IOPATTERN_IN_VIDEO_MEMORY as u16;
    param.AsyncDepth = 1;
    unsafe {
        let mfx = &mut param.__bindgen_anon_1.mfx;
        mfx.FrameInfo = frame_info;
        mfx.CodecId = vpl::MFX_CODEC_AVC;
        mfx.CodecProfile = vpl::MFX_PROFILE_AVC_MAIN as u16;
        mfx.LowPower = vpl::MFX_CODINGOPTION_ON as u16;

        let enc = &mut mfx.__bindgen_anon_1.__bindgen_anon_1;
        enc.TargetUsage = vpl::MFX_TARGETUSAGE_BEST_SPEED as u16;
        enc.GopPicSize = 30;
        enc.GopRefDist = 1;
        enc.IdrInterval = 0;
        enc.NumRefFrame = 1;
        enc.RateControlMethod = vpl::MFX_RATECONTROL_CBR as u16;
        enc.__bindgen_anon_2.TargetKbps = 4000;
    }

    let status = unsafe { vpl::MFXVideoENCODE_Init(session.raw(), &mut param) };
    if status < vpl::mfxStatus_MFX_ERR_NONE {
        return Err(format!("MFXVideoENCODE_Init failed with status {status}"));
    }
    Ok(())
}

fn align16(value: u32) -> u16 {
    (value.div_ceil(16) * 16) as u16
}

struct Bitstream {
    buffer: Vec<u8>,
    raw: vpl::mfxBitstream,
}

impl Bitstream {
    fn new() -> Self {
        let mut buffer = vec![0u8; BITSTREAM_CAPACITY];
        let mut raw = unsafe { std::mem::zeroed::<vpl::mfxBitstream>() };
        raw.Data = buffer.as_mut_ptr();
        raw.MaxLength = BITSTREAM_CAPACITY as u32;
        Self { buffer, raw }
    }

    fn reset(&mut self) {
        self.raw.DataOffset = 0;
        self.raw.DataLength = 0;
    }

    fn encoded(&self) -> &[u8] {
        let start = self.raw.DataOffset as usize;
        let end = start + self.raw.DataLength as usize;
        &self.buffer[start..end]
    }
}

fn encode_async(
    session: &Session,
    ctrl: *mut vpl::mfxEncodeCtrl,
    surface: *mut vpl::mfxFrameSurface1,
    bitstream: &mut vpl::mfxBitstream,
) -> Result<Option<vpl::mfxSyncPoint>, String> {
    let mut syncp = ptr::null_mut();
    let mut status;
    loop {
        status = unsafe {
            vpl::MFXVideoENCODE_EncodeFrameAsync(
                session.raw(),
                ctrl,
                surface,
                bitstream,
                &mut syncp,
            )
        };
        if status != vpl::mfxStatus_MFX_WRN_DEVICE_BUSY {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    match status {
        vpl::mfxStatus_MFX_ERR_NONE => Ok(Some(syncp)),
        vpl::mfxStatus_MFX_ERR_MORE_DATA => Ok(None),
        status if status > 0 => Ok(Some(syncp)),
        status => Err(format!("EncodeFrameAsync failed with status {status}")),
    }
}

fn sync(session: &Session, syncp: vpl::mfxSyncPoint) -> Result<(), String> {
    let status = unsafe {
        vpl::MFXVideoCORE_SyncOperation(session.raw(), syncp, vpl::MFX_INFINITE)
    };
    if status < vpl::mfxStatus_MFX_ERR_NONE {
        return Err(format!("SyncOperation failed with status {status}"));
    }
    Ok(())
}

fn nal_units(annexb: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut i = 0;
    while i + 3 < annexb.len() {
        let (code, len) = if annexb[i..].starts_with(&[0, 0, 0, 1]) {
            (true, 4)
        } else if annexb[i..].starts_with(&[0, 0, 1]) {
            (true, 3)
        } else {
            (false, 0)
        };
        if code {
            if let Some(&header) = annexb.get(i + len) {
                types.push(header & 0x1f);
            }
            i += len;
        } else {
            i += 1;
        }
    }
    types
}

#[test]
fn external_nv12_dmabuf_imports_into_va_and_encodes_h264() {
    // Step 0: open VA + oneVPL encode session.
    let display = VaDisplay::open(RENDER_NODE_PATH)
        .unwrap_or_else(|err| panic!("step0 VaDisplay::open: {err}"));
    let session =
        Session::new(RENDER_NODE_NUM, Codec::H264, Component::Encode, display.handle())
            .unwrap_or_else(|err| panic!("step0 Session::new: {err}"));
    init_encoder(&session, WIDTH, HEIGHT)
        .unwrap_or_else(|err| panic!("step0 init_encoder: {err}"));
    eprintln!("step0 OK: VA display + oneVPL H.264 encode session ready");

    let mut bitstream = Bitstream::new();
    let mut outputs: Vec<Vec<u8>> = Vec::new();

    for frame in 0..FRAMES {
        // Step 1: allocate + fill an external LINEAR NV12 dma-buf.
        let dma_buf = UdmabufNv12::allocate(WIDTH, HEIGHT, frame)
            .unwrap_or_else(|err| panic!("step1 udmabuf alloc (frame {frame}): {err}"));
        if frame == 0 {
            eprintln!(
                "step1 OK: udmabuf NV12 {WIDTH}x{HEIGHT} LINEAR fd={} size={}",
                dma_buf.dma_buf_fd, dma_buf.size
            );
        }

        // Step 2: import the dma-buf as a VA surface (vaCreateSurfaces, DRM_PRIME_2).
        let va_surface = display
            .import_nv12_surface(dma_buf.external(WIDTH, HEIGHT))
            .unwrap_or_else(|err| panic!("step2 vaCreateSurfaces (frame {frame}): {err}"));
        if frame == 0 {
            eprintln!("step2 OK: VA surface id={} imported from dma-buf", va_surface.id());
        }

        // Step 3: wrap the VA surface as an mfxFrameSurface1 (ImportFrameSurface).
        let (mut surface, _import_flags) = session
            .import_va_surface(display.handle(), va_surface.id(), Component::Encode)
            .unwrap_or_else(|err| {
                panic!("step3 ImportFrameSurface (frame {frame}): {err}")
            });
        unsafe {
            let info = &mut (*surface.raw()).Info;
            info.FourCC = vpl::MFX_FOURCC_NV12;
            info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
            info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
            let dims = &mut info.__bindgen_anon_1.__bindgen_anon_1;
            dims.Width = align16(WIDTH);
            dims.Height = align16(HEIGHT);
            dims.CropW = WIDTH as u16;
            dims.CropH = HEIGHT as u16;
        }
        surface.set_timestamp(frame);
        if frame == 0 {
            eprintln!("step3 OK: mfxFrameSurface1 imported from VA surface");
        }

        // Step 4: encode this frame (force IDR on the first).
        let mut ctrl = unsafe { std::mem::zeroed::<vpl::mfxEncodeCtrl>() };
        let ctrl_ptr = if frame == 0 {
            ctrl.FrameType = (vpl::MFX_FRAMETYPE_IDR
                | vpl::MFX_FRAMETYPE_I
                | vpl::MFX_FRAMETYPE_REF) as u16;
            &mut ctrl as *mut _
        } else {
            ptr::null_mut()
        };
        bitstream.reset();
        let syncp = encode_async(&session, ctrl_ptr, surface.raw(), &mut bitstream.raw)
            .unwrap_or_else(|err| panic!("step4 encode (frame {frame}): {err}"));
        if let Some(syncp) = syncp {
            sync(&session, syncp)
                .unwrap_or_else(|err| panic!("step4 sync (frame {frame}): {err}"));
            outputs.push(bitstream.encoded().to_vec());
        }
        // Surface + VA surface + dma-buf released here in reverse order.
    }

    // Drain any buffered frames.
    loop {
        bitstream.reset();
        let syncp =
            encode_async(&session, ptr::null_mut(), ptr::null_mut(), &mut bitstream.raw)
                .unwrap_or_else(|err| panic!("drain encode: {err}"));
        match syncp {
            Some(syncp) => {
                sync(&session, syncp).unwrap_or_else(|err| panic!("drain sync: {err}"));
                outputs.push(bitstream.encoded().to_vec());
            }
            None => break,
        }
    }

    let total: usize = outputs.iter().map(Vec::len).sum();
    let first_nals = outputs.first().map(|chunk| nal_units(chunk)).unwrap_or_default();
    eprintln!(
        "step4 OK: {} encoded frames, {total} bytes, first-frame NAL types {:?}",
        outputs.len(),
        first_nals
    );

    assert!(!outputs.is_empty(), "encoder produced no frames");
    assert!(total > 0, "encoder produced empty bitstream");
    // First frame must be an IDR keyframe: NAL type 5 (IDR slice), and SPS/PPS (7/8).
    assert!(first_nals.contains(&5), "first frame is not an IDR keyframe: {first_nals:?}");
    assert!(first_nals.contains(&7), "first frame missing SPS NAL: {first_nals:?}");
    assert!(first_nals.contains(&8), "first frame missing PPS NAL: {first_nals:?}");

    unsafe { vpl::MFXVideoENCODE_Close(session.raw()) };
}

// =========================== Phase 2 ===========================

const Y_TOP: f64 = 0.85; // luma of the top half of the rendered pattern
const Y_BOTTOM: f64 = 0.20; // luma of the bottom half

/// Set up a wgpu Vulkan device with the Quick Sync dma-buf interop features.
fn create_dmabuf_wgpu_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::empty(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .expect("no Vulkan adapter");
    let features = supported_wgpu_features(&adapter);
    assert!(
        !features.is_empty(),
        "adapter does not support Quick Sync dma-buf interop features"
    );
    create_wgpu_device(
        &adapter,
        &wgpu::DeviceDescriptor {
            label: Some("poc dmabuf device"),
            required_features: features,
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        },
    )
    .expect("create_wgpu_device")
}

/// Render a top-bright / bottom-dark NV12 pattern into the dma-buf texture's Y and
/// UV plane views (two color-attachment passes, mirroring `WgpuRgbaToNv12Converter`).
fn render_pattern(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    let shader_src = format!(
        r#"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {{
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4<f32>(p[idx], 0.0, 1.0);
}}

@fragment
fn fs_y(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {{
    let luma = select({bottom}, {top}, pos.y < {half}.0);
    return vec4<f32>(luma, 0.0, 0.0, 1.0);
}}

@fragment
fn fs_uv(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {{
    return vec4<f32>(0.5, 0.5, 0.0, 1.0);
}}
"#,
        top = Y_TOP,
        bottom = Y_BOTTOM,
        half = HEIGHT / 2,
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("poc nv12 pattern"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("poc nv12 pattern layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let make_pipeline = |entry: &str, format: wgpu::TextureFormat| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("poc nv12 plane pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let y_pipeline = make_pipeline("fs_y", wgpu::TextureFormat::R8Unorm);
    let uv_pipeline = make_pipeline("fs_uv", wgpu::TextureFormat::Rg8Unorm);

    let y_view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::R8Unorm),
        aspect: wgpu::TextureAspect::Plane0,
        ..Default::default()
    });
    let uv_view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rg8Unorm),
        aspect: wgpu::TextureAspect::Plane1,
        ..Default::default()
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    for (view, pipeline) in [(&y_view, &y_pipeline), (&uv_view, &uv_pipeline)] {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll");
}

/// Read the Y plane back from the GPU and return (top-half mean, bottom-half mean).
fn read_y_plane_means(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> (f64, f64) {
    let bytes_per_row = (WIDTH + 255) / 256 * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("poc y readback"),
        size: u64::from(bytes_per_row * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane0,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll readback");
    let data = buffer.slice(..).get_mapped_range().expect("map range");
    let means = plane_means(&data, bytes_per_row as usize);
    drop(data);
    buffer.unmap();
    means
}

/// Mean luma of the top and bottom halves of a width=WIDTH, height=HEIGHT Y plane
/// with the given row stride.
fn plane_means(data: &[u8], stride: usize) -> (f64, f64) {
    let mut sums = [0u64; 2];
    let mut counts = [0u64; 2];
    for y in 0..HEIGHT as usize {
        let half = if y < HEIGHT as usize / 2 { 0 } else { 1 };
        let row = &data[y * stride..y * stride + WIDTH as usize];
        sums[half] += row.iter().map(|&b| u64::from(b)).sum::<u64>();
        counts[half] += WIDTH as u64;
    }
    (sums[0] as f64 / counts[0] as f64, sums[1] as f64 / counts[1] as f64)
}

/// Decode the first frame of an Annex-B H.264 stream with ffmpeg and return the
/// top/bottom-half mean luma of the decoded picture.
fn ffmpeg_decode_first_frame_means(annexb: &[u8]) -> (f64, f64) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let in_path = dir.join(format!("poc_phase2_{pid}.h264"));
    let out_path = dir.join(format!("poc_phase2_{pid}.gray"));
    std::fs::File::create(&in_path).unwrap().write_all(annexb).unwrap();

    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "h264", "-i"])
        .arg(&in_path)
        .args(["-frames:v", "1", "-pix_fmt", "gray", "-f", "rawvideo"])
        .arg(&out_path)
        .status()
        .expect("run ffmpeg");
    assert!(status.success(), "ffmpeg decode failed");

    let decoded = std::fs::read(&out_path).expect("read decoded frame");
    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    assert_eq!(
        decoded.len(),
        (WIDTH * HEIGHT) as usize,
        "decoded frame size mismatch"
    );
    plane_means(&decoded, WIDTH as usize)
}

// =========================== Phase 0: CCS feasibility ===========================

/// Phase 0: does the device advertise an Intel render-compression (CCS) NV12
/// modifier, can we allocate a RENDERABLE+exportable NV12 surface on it, and does
/// VA + oneVPL import that SAME compressed surface zero-copy (IMPORT_SHARED)?
/// This is purely a feasibility check: it allocates the CCS surface and walks it
/// through `vaCreateSurfaces` (with the aux plane(s) described) and oneVPL
/// `ImportFrameSurface`, then reports SHARED vs COPY vs failure. No rendering.
#[test]
fn ccs_renderable_nv12_imports_into_va_shared_feasibility() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::level_filters::LevelFilter::TRACE)
        .with_writer(std::io::stderr)
        .try_init();

    const CCS_WIDTH: u32 = 1920;
    const CCS_HEIGHT: u32 = 1080;
    let resolution = VideoResolution { width: CCS_WIDTH, height: CCS_HEIGHT };

    let (device, _queue) = create_dmabuf_wgpu_device();
    let interop = DmaBufInterop::new(&device)
        .unwrap_or_else(|err| panic!("phase0 DmaBufInterop: {err}"));

    // On the Mesa ANV Vulkan driver (Meteor Lake) the NV12 modifier list is
    // LINEAR / X_TILED / 4_TILED only — all plane_count=2, no render-compression
    // (CCS) modifier. The compositor renders through wgpu→Vulkan→ANV, so if ANV
    // never exposes a CCS NV12 modifier we cannot even allocate a surface to render
    // into, let alone hand it to VA. That is a terminal, documented NO: the probe
    // logs the full list above for the record and the test stops here (green).
    let probe = match probe_ccs_renderable_nv12(&interop, resolution) {
        Ok(probe) => probe,
        Err(err) => {
            eprintln!(
                "PHASE0 VERDICT: NOT FEASIBLE — the Vulkan driver advertises no \
                 render-compression (CCS) NV12 modifier to render into: {err}"
            );
            return;
        }
    };
    let planes: Vec<(u32, u32)> =
        probe.planes.iter().map(|p| (p.offset, p.pitch)).collect();
    eprintln!(
        "phase0 OK: allocated CCS renderable NV12, modifier={:#018x} plane_count={} \
         size={} planes(offset,pitch)={:?}",
        probe.modifier, probe.plane_count, probe.size, planes
    );

    // Open VA + a oneVPL H.264 encode session (encode is the zero-copy consumer).
    let display = VaDisplay::open(RENDER_NODE_PATH)
        .unwrap_or_else(|err| panic!("phase0 VaDisplay::open: {err}"));
    let session =
        Session::new(RENDER_NODE_NUM, Codec::H264, Component::Encode, display.handle())
            .unwrap_or_else(|err| panic!("phase0 Session::new: {err}"));
    init_encoder(&session, CCS_WIDTH, CCS_HEIGHT)
        .unwrap_or_else(|err| panic!("phase0 init_encoder: {err}"));

    // Step A: VA must accept the compressed surface (describing all N planes).
    let va_surface = match display.import_nv12_surface_planes(ExternalNv12DmaBufPlanes {
        fd: probe.fd.as_raw_fd(),
        size: probe.size,
        modifier: probe.modifier,
        width: CCS_WIDTH,
        height: CCS_HEIGHT,
        planes: planes.clone(),
    }) {
        Ok(surface) => surface,
        Err(err) => {
            eprintln!(
                "PHASE0 VERDICT: NOT FEASIBLE — iHD vaCreateSurfaces rejected the CCS \
                 NV12 surface (modifier={:#018x}, {} planes): {err}",
                probe.modifier,
                planes.len(),
            );
            panic!("phase0 vaCreateSurfaces (CCS) failed: {err}");
        }
    };
    eprintln!("phase0 OK: VA imported CCS surface id={}", va_surface.id());

    // Step B: oneVPL must import the VA surface, and we need IMPORT_SHARED for the
    // copy to actually be eliminated.
    let (surface, import_flags) = match session.import_va_surface(
        display.handle(),
        va_surface.id(),
        Component::Encode,
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!(
                "PHASE0 VERDICT: NOT FEASIBLE — oneVPL ImportFrameSurface rejected the \
                 CCS VA surface: {err}"
            );
            panic!("phase0 ImportFrameSurface (CCS) failed: {err}");
        }
    };
    let shared = import_flags & vpl::MFX_SURFACE_FLAG_IMPORT_SHARED != 0;
    let copy = import_flags & vpl::MFX_SURFACE_FLAG_IMPORT_COPY != 0;
    let mode = if shared {
        "IMPORT_SHARED (zero-copy)"
    } else if copy {
        "IMPORT_COPY (fallback)"
    } else {
        "unknown"
    };
    eprintln!(
        "phase0 OK: oneVPL imported CCS surface, SurfaceFlags={import_flags:#x} ({mode})"
    );
    eprintln!(
        "PHASE0 VERDICT: {} — CCS modifier {:#018x} with {} planes imported {}",
        if shared { "FEASIBLE" } else { "NOT FEASIBLE" },
        probe.modifier,
        planes.len(),
        mode,
    );

    // Tear down consumers before producers.
    drop(surface);
    unsafe { vpl::MFXVideoENCODE_Close(session.raw()) };
    drop(session);
    drop(va_surface);
    drop(display);
    drop(probe);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    drop(interop);
    drop(device);

    assert!(
        shared,
        "CCS NV12 surface did not import zero-copy (IMPORT_SHARED); flags={import_flags:#x}"
    );
}

#[test]
fn rendered_nv12_dmabuf_imports_into_va_and_encodes_h264() {
    let resolution = VideoResolution { width: WIDTH, height: HEIGHT };

    // Step 1: GPU-allocate a renderable, exportable NV12 dma-buf.
    let (device, queue) = create_dmabuf_wgpu_device();
    let interop = DmaBufInterop::new(&device)
        .unwrap_or_else(|err| panic!("step1 DmaBufInterop: {err}"));
    let RenderableNv12DmaBuf {
        texture,
        fd,
        modifier,
        size,
        y_offset,
        y_pitch,
        uv_offset,
        uv_pitch,
    } = export_renderable_nv12(&interop, resolution)
        .unwrap_or_else(|err| panic!("step1 export_renderable_nv12: {err}"));
    let tiled = modifier != DRM_FORMAT_MOD_LINEAR;
    eprintln!(
        "step1 OK: renderable NV12 dma-buf, modifier={modifier:#x} ({}), size={size}, \
         y(off={y_offset},pitch={y_pitch}) uv(off={uv_offset},pitch={uv_pitch})",
        if tiled { "TILED" } else { "LINEAR" }
    );

    // Step 2: render the test pattern into the dma-buf via wgpu, then verify the GPU
    // actually wrote it by reading the Y plane back.
    render_pattern(&device, &queue, &texture);
    let (gpu_top, gpu_bottom) = read_y_plane_means(&device, &queue, &texture);
    eprintln!("step2 OK: GPU-rendered Y means top={gpu_top:.1} bottom={gpu_bottom:.1}");
    assert!(
        gpu_top > 170.0 && gpu_bottom < 100.0 && gpu_top - gpu_bottom > 80.0,
        "GPU render did not produce the expected pattern: top={gpu_top:.1} bottom={gpu_bottom:.1}"
    );

    // Step 3: import the rendered dma-buf into VA, then oneVPL.
    let display = VaDisplay::open(RENDER_NODE_PATH)
        .unwrap_or_else(|err| panic!("step3 VaDisplay::open: {err}"));
    let session =
        Session::new(RENDER_NODE_NUM, Codec::H264, Component::Encode, display.handle())
            .unwrap_or_else(|err| panic!("step3 Session::new: {err}"));
    init_encoder(&session, WIDTH, HEIGHT)
        .unwrap_or_else(|err| panic!("step3 init_encoder: {err}"));

    let va_surface = display
        .import_nv12_surface(ExternalNv12DmaBuf {
            fd: fd.as_raw_fd(),
            size,
            modifier,
            width: WIDTH,
            height: HEIGHT,
            y_offset,
            y_pitch,
            uv_offset,
            uv_pitch,
        })
        .unwrap_or_else(|err| panic!("step3 vaCreateSurfaces: {err}"));
    let (mut surface, import_flags) = session
        .import_va_surface(display.handle(), va_surface.id(), Component::Encode)
        .unwrap_or_else(|err| panic!("step3 ImportFrameSurface: {err}"));
    let shared = import_flags & vpl::MFX_SURFACE_FLAG_IMPORT_SHARED != 0;
    let copy = import_flags & vpl::MFX_SURFACE_FLAG_IMPORT_COPY != 0;
    eprintln!(
        "step3 OK: VA surface id={} imported into oneVPL, SurfaceFlags={import_flags:#x} \
         ({})",
        va_surface.id(),
        if shared { "IMPORT_SHARED / zero-copy" } else if copy { "IMPORT_COPY" } else { "unknown" }
    );
    unsafe {
        let info = &mut (*surface.raw()).Info;
        info.FourCC = vpl::MFX_FOURCC_NV12;
        info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
        info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
        let dims = &mut info.__bindgen_anon_1.__bindgen_anon_1;
        dims.Width = align16(WIDTH);
        dims.Height = align16(HEIGHT);
        dims.CropW = WIDTH as u16;
        dims.CropH = HEIGHT as u16;
    }

    // Step 4: encode a few frames from the rendered surface.
    let mut bitstream = Bitstream::new();
    let mut outputs: Vec<Vec<u8>> = Vec::new();
    for frame in 0..FRAMES {
        surface.set_timestamp(frame);
        let mut ctrl = unsafe { std::mem::zeroed::<vpl::mfxEncodeCtrl>() };
        let ctrl_ptr = if frame == 0 {
            ctrl.FrameType = (vpl::MFX_FRAMETYPE_IDR
                | vpl::MFX_FRAMETYPE_I
                | vpl::MFX_FRAMETYPE_REF) as u16;
            &mut ctrl as *mut _
        } else {
            ptr::null_mut()
        };
        bitstream.reset();
        let syncp = encode_async(&session, ctrl_ptr, surface.raw(), &mut bitstream.raw)
            .unwrap_or_else(|err| panic!("step4 encode (frame {frame}): {err}"));
        if let Some(syncp) = syncp {
            sync(&session, syncp)
                .unwrap_or_else(|err| panic!("step4 sync (frame {frame}): {err}"));
            outputs.push(bitstream.encoded().to_vec());
        }
    }
    loop {
        bitstream.reset();
        let syncp =
            encode_async(&session, ptr::null_mut(), ptr::null_mut(), &mut bitstream.raw)
                .unwrap_or_else(|err| panic!("drain encode: {err}"));
        match syncp {
            Some(syncp) => {
                sync(&session, syncp).unwrap_or_else(|err| panic!("drain sync: {err}"));
                outputs.push(bitstream.encoded().to_vec());
            }
            None => break,
        }
    }

    let total: usize = outputs.iter().map(Vec::len).sum();
    let first = outputs.first().expect("no encoded frames");
    let first_nals = nal_units(first);
    assert!(first_nals.contains(&5), "first frame is not an IDR keyframe: {first_nals:?}");
    assert!(first_nals.contains(&7), "first frame missing SPS NAL: {first_nals:?}");

    // Step 4 (content integrity): decode the keyframe and confirm it matches the
    // rendered top-bright/bottom-dark pattern (not garbage).
    let annexb: Vec<u8> = outputs.concat();
    let (dec_top, dec_bottom) = ffmpeg_decode_first_frame_means(&annexb);
    eprintln!(
        "step4 OK: {} frames, {total} bytes, first NALs {first_nals:?}; decoded Y means \
         top={dec_top:.1} bottom={dec_bottom:.1}",
        outputs.len()
    );
    assert!(
        dec_top > 170.0 && dec_bottom < 100.0 && dec_top - dec_bottom > 80.0,
        "decoded content does not match rendered pattern: top={dec_top:.1} bottom={dec_bottom:.1}"
    );

    eprintln!(
        "VERDICT: render->NV12 dma-buf({})->VA->oneVPL encode works; import mode = {}",
        if tiled { "TILED" } else { "LINEAR" },
        if shared { "IMPORT_SHARED (zero-copy)" } else { "IMPORT_COPY" }
    );

    // Tear down consumers before producers: oneVPL imported surface -> encoder ->
    // session -> VA surface -> VA display -> dma-buf fd -> wgpu texture (frees the
    // Vulkan image/memory) -> wgpu device.
    drop(surface);
    unsafe { vpl::MFXVideoENCODE_Close(session.raw()) };
    drop(session);
    drop(va_surface);
    drop(display);
    drop(fd);
    drop(texture);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    drop(interop);
    drop(queue);
    drop(device);
}
