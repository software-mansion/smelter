use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::quicksync::sys as vpl;
use tracing::info;

use crate::{
    EncodedInputChunk, FrameMetadata, H264DecoderEvent, OutputFrame, VideoResolution,
    device::{ColorRange, ColorSpace},
    dmabuf::QuickSyncDmaBufSync,
    parser::h264::{H264Parser, ParsedNalu},
    quicksync::{
        h264::{
            H264Session, H264SessionError, ImportedRgbaSurface, QUICKSYNC_ASYNC_DEPTH,
            VplSyncQueue, init_dmabuf_interop, retry_device_busy,
        },
        vpl::{Component, FrameSurface, SyncWait, check_status_allow_warnings},
    },
};

const NO_TIMESTAMP: u64 = u64::MAX;

pub struct WgpuTexturesDecoderH264 {
    decoder: QuickSyncH264Decoder,
    sync: QuickSyncDmaBufSync,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    completed_copy: Arc<AtomicU64>,
    next_copy: u64,
    pending_copies: VecDeque<PendingCopy>,
}

#[derive(Debug, thiserror::Error)]
pub enum QuickSyncH264DecoderError {
    #[error("Intel Quick Sync H264 decoder is unavailable: {0}")]
    Unavailable(#[from] H264SessionError),

    #[error("Intel Quick Sync H264 decode error: {0}")]
    Decode(String),
}

impl WgpuTexturesDecoderH264 {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        adapter_info: &wgpu::AdapterInfo,
    ) -> Result<Self, QuickSyncH264DecoderError> {
        info!("Initializing Intel Quick Sync H264 decoder");
        let (_interop, sync) = init_dmabuf_interop(&device, &queue)?;
        let decoder = QuickSyncH264Decoder::new(adapter_info)?;
        Ok(Self {
            decoder,
            sync,
            device,
            queue,
            completed_copy: Arc::new(AtomicU64::new(0)),
            next_copy: 1,
            pending_copies: VecDeque::new(),
        })
    }

    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, QuickSyncH264DecoderError> {
        self.process_event(H264DecoderEvent::DecodeChunk(frame))
    }

    pub fn flush(
        &mut self,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, QuickSyncH264DecoderError> {
        self.process_event(H264DecoderEvent::Flush)
    }

    pub fn process_event(
        &mut self,
        event: H264DecoderEvent<'_>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, QuickSyncH264DecoderError> {
        let frames = match event {
            H264DecoderEvent::DecodeChunk(chunk) => self.decoder.decode(chunk),
            H264DecoderEvent::SignalFrameEnd => self.decoder.drain_ready(),
            H264DecoderEvent::Flush => self.decoder.flush(),
            H264DecoderEvent::SignalDataLoss => {
                self.decoder.reset();
                Ok(Vec::new())
            }
            H264DecoderEvent::DecodeParsedFrame(_) => {
                Err("Intel Quick Sync H264 decoder accepts encoded bytestream chunks"
                    .into())
            }
        }
        .map_err(QuickSyncH264DecoderError::Decode)?;
        frames.into_iter().map(|frame| self.copy_frame(frame)).collect()
    }

    fn copy_frame(
        &mut self,
        frame: OutputFrame<DecodedSurface>,
    ) -> Result<OutputFrame<wgpu::Texture>, QuickSyncH264DecoderError> {
        self.retire_completed_copies();
        let OutputFrame { data, metadata } = frame;
        let size = data.resolution.extent_2d();
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Intel Quick Sync H264 decoder output texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let vpp_output = self
            .decoder
            .quicksync
            .session
            .get_surface_for_vpp_output()
            .map_err(|err| QuickSyncH264DecoderError::Decode(err.to_string()))?;
        let syncp = self
            .decoder
            .quicksync
            .session
            .run_vpp(&data.surface, &vpp_output)
            .map_err(|err| QuickSyncH264DecoderError::Decode(err.to_string()))?;
        self.decoder
            .quicksync
            .session
            .sync_status(syncp, SyncWait::Block)
            .map_err(|err| QuickSyncH264DecoderError::Decode(err.to_string()))?;
        let imported = self
            .decoder
            .quicksync
            .import_bgr4_surface(
                &self.device,
                &vpp_output,
                wgpu::TextureUsages::COPY_SRC,
                wgpu::TextureUses::COPY_SRC,
            )
            .map_err(|err| QuickSyncH264DecoderError::Decode(err.to_string()))?;
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Intel Quick Sync H264 decoder output copy"),
            });
        encoder.copy_texture_to_texture(
            imported.frame.texture().as_image_copy(),
            texture.as_image_copy(),
            size,
        );
        self.sync
            .submit_target_read(
                imported.frame.as_ref(),
                encoder,
                "Intel Quick Sync H264 decoder output copy",
            )
            .map_err(|err| QuickSyncH264DecoderError::Decode(err.to_string()))?;

        let serial = self.next_copy;
        self.next_copy += 1;
        let completed_copy = Arc::clone(&self.completed_copy);
        self.queue.on_submitted_work_done(move || {
            completed_copy.store(serial, Ordering::Release);
        });
        self.pending_copies.push_back(PendingCopy {
            serial,
            _imported: imported,
            _vpp_output: vpp_output,
            _decoded: data,
        });

        Ok(OutputFrame { data: texture, metadata })
    }

    fn retire_completed_copies(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        let completed = self.completed_copy.load(Ordering::Acquire);
        while self.pending_copies.front().is_some_and(|copy| copy.serial <= completed) {
            self.pending_copies.pop_front();
        }
    }
}

impl Drop for WgpuTexturesDecoderH264 {
    fn drop(&mut self) {
        if !self.pending_copies.is_empty() {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            self.retire_completed_copies();
        }
    }
}

struct PendingCopy {
    serial: u64,
    _imported: ImportedRgbaSurface,
    _vpp_output: FrameSurface,
    _decoded: DecodedSurface,
}

struct QuickSyncH264Decoder {
    quicksync: H264Session,
    resolution: Option<VideoResolution>,
    parser: H264Parser,
    color_space: ColorSpace,
    color_range: ColorRange,
    pending_pts: PendingPts,
    pending: VplSyncQueue<PendingDecode>,
}

impl QuickSyncH264Decoder {
    fn new(adapter_info: &wgpu::AdapterInfo) -> Result<Self, H264SessionError> {
        let quicksync = H264Session::new(adapter_info, Component::Decode)?;
        Ok(Self {
            quicksync,
            resolution: None,
            parser: H264Parser::default(),
            color_space: ColorSpace::Unspecified,
            color_range: ColorRange::Limited,
            pending_pts: PendingPts::default(),
            pending: VplSyncQueue::new(usize::from(QUICKSYNC_ASYNC_DEPTH)),
        })
    }

    fn decode(
        &mut self,
        chunk: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        self.update_color_metadata(chunk.data)?;
        let resolution = self.ensure_initialized(chunk.data, chunk.pts)?;
        let mut bitstream = input_bitstream(chunk.data, chunk.pts)?;
        self.pending_pts.queue(chunk.pts);
        let mut frames = self.drain_completed(SyncWait::Poll)?;
        frames.extend(self.decode_bitstream(&mut bitstream, resolution)?);
        Ok(frames)
    }

    fn drain_ready(&mut self) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        self.drain_completed(SyncWait::Poll)
    }

    fn flush(&mut self) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        let mut frames = self.drain_completed(SyncWait::Poll)?;
        let Some(resolution) = self.resolution else {
            return Ok(frames);
        };
        frames.extend(self.submit_until_more_data(None, resolution)?);
        frames.extend(self.drain_all_completed()?);
        Ok(frames)
    }

    fn reset(&mut self) {
        self.close_decoder(false);
        self.parser = H264Parser::default();
        self.color_space = ColorSpace::Unspecified;
        self.color_range = ColorRange::Limited;
        self.pending_pts.clear();
        self.pending.clear();
    }

    fn update_color_metadata(&mut self, data: &[u8]) -> Result<(), String> {
        let mut access_units = self
            .parser
            .parse(data, None)
            .map_err(|err| format!("Intel Quick Sync H264 parser error: {err}"))?;
        access_units.extend(
            self.parser
                .flush()
                .map_err(|err| format!("Intel Quick Sync H264 parser error: {err}"))?,
        );
        for access_unit in access_units {
            for nalu in access_unit.0 {
                if let ParsedNalu::Sps(sps) = nalu.parsed {
                    self.color_space = ColorSpace::from(&sps);
                    self.color_range = ColorRange::from(&sps);
                }
            }
        }
        Ok(())
    }

    fn ensure_initialized(
        &mut self,
        data: &[u8],
        pts: Option<u64>,
    ) -> Result<VideoResolution, String> {
        if let Some(resolution) = self.resolution {
            return Ok(resolution);
        }

        let mut bitstream = input_bitstream(data, pts)?;
        let mut video_param = decoder_video_param();
        check_status_allow_warnings("MFXVideoDECODE_DecodeHeader", unsafe {
            vpl::MFXVideoDECODE_DecodeHeader(
                self.quicksync.session.raw(),
                &mut bitstream,
                &mut video_param,
            )
        })
        .map_err(|err| err.to_string())?;
        set_decoder_video_param_defaults(&mut video_param);
        check_status_allow_warnings("MFXVideoDECODE_Init", unsafe {
            vpl::MFXVideoDECODE_Init(self.quicksync.session.raw(), &mut video_param)
        })
        .map_err(|err| err.to_string())?;
        let layout = decoder_layout(&video_param)?;
        self.quicksync
            .session
            .init_vpp_nv12_to_bgr4(
                vpl_u16_dimension("VPP coded width", layout.coded.width)?,
                vpl_u16_dimension("VPP coded height", layout.coded.height)?,
                vpl_u16_dimension("VPP crop width", layout.visible.width)?,
                vpl_u16_dimension("VPP crop height", layout.visible.height)?,
            )
            .map_err(|err| err.to_string())?;
        let resolution = layout.visible;
        self.resolution = Some(resolution);
        Ok(resolution)
    }

    fn decode_bitstream(
        &mut self,
        bitstream: &mut vpl::mfxBitstream,
        resolution: VideoResolution,
    ) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        self.submit_until_more_data(Some(bitstream), resolution)
    }

    fn submit_until_more_data(
        &mut self,
        mut bitstream: Option<&mut vpl::mfxBitstream>,
        resolution: VideoResolution,
    ) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        let mut frames = Vec::new();
        loop {
            if self.is_full() {
                frames.extend(self.drain_completed(SyncWait::Block)?);
            }
            match self.submit_decode(bitstream.as_deref_mut(), resolution)? {
                DecodeSubmit::Submitted => {
                    frames.extend(self.drain_completed(SyncWait::Poll)?)
                }
                DecodeSubmit::NeedMoreData => break,
            }
        }
        Ok(frames)
    }

    fn submit_decode(
        &mut self,
        mut bitstream: Option<&mut vpl::mfxBitstream>,
        resolution: VideoResolution,
    ) -> Result<DecodeSubmit, String> {
        loop {
            let bitstream =
                bitstream.as_deref_mut().map_or(std::ptr::null_mut(), |bitstream| {
                    bitstream as *mut vpl::mfxBitstream
                });
            let mut output = std::ptr::null_mut();
            let mut syncp = std::ptr::null_mut();
            let status =
                retry_device_busy("MFXVideoDECODE_DecodeFrameAsync", || unsafe {
                    vpl::MFXVideoDECODE_DecodeFrameAsync(
                        self.quicksync.session.raw(),
                        bitstream,
                        std::ptr::null_mut(),
                        &mut output,
                        &mut syncp,
                    )
                })?;
            match status {
                vpl::mfxStatus_MFX_ERR_NONE => {
                    self.queue_output(syncp, output, resolution)?;
                    return Ok(DecodeSubmit::Submitted);
                }
                vpl::mfxStatus_MFX_ERR_MORE_DATA => {
                    return Ok(DecodeSubmit::NeedMoreData);
                }
                vpl::mfxStatus_MFX_WRN_VIDEO_PARAM_CHANGED => {
                    self.refresh_video_param(resolution)?;
                    if !output.is_null() {
                        self.queue_output(syncp, output, resolution)?;
                        return Ok(DecodeSubmit::Submitted);
                    }
                }
                vpl::mfxStatus_MFX_ERR_REALLOC_SURFACE => {
                    return Err("Intel Quick Sync H264 stream parameters changed".into());
                }
                vpl::mfxStatus_MFX_ERR_MORE_SURFACE => {
                    return Err(
                        "Intel Quick Sync requested an external decode surface unexpectedly".into(),
                    );
                }
                status if status > 0 && !output.is_null() => {
                    self.queue_output(syncp, output, resolution)?;
                    return Ok(DecodeSubmit::Submitted);
                }
                status if status > 0 => return Ok(DecodeSubmit::NeedMoreData),
                status => {
                    return Err(format!(
                        "MFXVideoDECODE_DecodeFrameAsync failed with oneVPL status {status}"
                    ));
                }
            }
        }
    }

    fn queue_output(
        &mut self,
        syncp: vpl::mfxSyncPoint,
        output: *mut vpl::mfxFrameSurface1,
        resolution: VideoResolution,
    ) -> Result<(), String> {
        let surface = FrameSurface::new(output).map_err(|err| err.to_string())?;
        self.pending.push(
            syncp,
            PendingDecode {
                surface,
                resolution,
                color_space: self.color_space,
                color_range: self.color_range,
                fallback_pts: self.pending_pts.pop(),
            },
        );
        Ok(())
    }

    fn refresh_video_param(
        &self,
        current_resolution: VideoResolution,
    ) -> Result<(), String> {
        let mut video_param = decoder_video_param();
        check_status_allow_warnings("MFXVideoDECODE_GetVideoParam", unsafe {
            vpl::MFXVideoDECODE_GetVideoParam(
                self.quicksync.session.raw(),
                &mut video_param,
            )
        })
        .map_err(|err| err.to_string())?;
        let resolution = decoder_layout(&video_param)?.visible;
        if current_resolution != resolution {
            return Err("Intel Quick Sync H264 stream parameters changed".into());
        }
        Ok(())
    }

    fn output_frame(pending: PendingDecode) -> OutputFrame<DecodedSurface> {
        let PendingDecode { surface, resolution, color_space, color_range, fallback_pts } =
            pending;
        let timestamp = surface.timestamp();
        OutputFrame {
            data: DecodedSurface { surface, resolution },
            metadata: FrameMetadata {
                pts: output_pts(timestamp, fallback_pts),
                color_space,
                color_range,
            },
        }
    }

    fn is_full(&self) -> bool {
        self.pending.is_full()
    }

    fn drain_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        self.pending.drain_completed(&self.quicksync, wait, |pending| {
            Ok(Self::output_frame(pending))
        })
    }

    fn drain_all_completed(
        &mut self,
    ) -> Result<Vec<OutputFrame<DecodedSurface>>, String> {
        self.pending.drain_all_completed(&self.quicksync, |pending| {
            Ok(Self::output_frame(pending))
        })
    }

    fn close_decoder(&mut self, drain: bool) {
        if self.resolution.is_some() {
            if drain {
                let _ = self.flush();
            } else {
                let _ = self.drain_all_completed();
            }
            unsafe {
                let _ = vpl::MFXVideoDECODE_Close(self.quicksync.session.raw());
            }
        }
        self.resolution = None;
    }
}

impl Drop for QuickSyncH264Decoder {
    fn drop(&mut self) {
        self.close_decoder(true);
    }
}

struct DecodedSurface {
    surface: FrameSurface,
    resolution: VideoResolution,
}

struct PendingDecode {
    surface: FrameSurface,
    resolution: VideoResolution,
    color_space: ColorSpace,
    color_range: ColorRange,
    fallback_pts: Option<u64>,
}

#[derive(Default)]
struct PendingPts(VecDeque<u64>);

impl PendingPts {
    fn queue(&mut self, pts: Option<u64>) {
        if let Some(pts) = pts
            && self.0.back().copied() != Some(pts)
        {
            self.0.push_back(pts);
        }
    }

    fn pop(&mut self) -> Option<u64> {
        self.0.pop_front()
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

enum DecodeSubmit {
    Submitted,
    NeedMoreData,
}

fn decoder_video_param() -> vpl::mfxVideoParam {
    let mut video_param = unsafe { std::mem::zeroed::<vpl::mfxVideoParam>() };
    set_decoder_video_param_defaults(&mut video_param);
    video_param
}

fn set_decoder_video_param_defaults(video_param: &mut vpl::mfxVideoParam) {
    video_param.IOPattern = vpl::MFX_IOPATTERN_OUT_VIDEO_MEMORY as u16;
    video_param.AsyncDepth = QUICKSYNC_ASYNC_DEPTH;
    unsafe {
        let mfx = &mut video_param.__bindgen_anon_1.mfx;
        mfx.CodecId = vpl::MFX_CODEC_AVC;
        mfx.FrameInfo.FourCC = vpl::MFX_FOURCC_NV12;
        mfx.FrameInfo.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
        mfx.FrameInfo.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    }
}

fn vpl_u16_dimension(name: &str, value: u32) -> Result<u16, String> {
    value.try_into().map_err(|_| format!("H264 {name} {value} exceeds oneVPL limit"))
}

fn input_bitstream(data: &[u8], pts: Option<u64>) -> Result<vpl::mfxBitstream, String> {
    let len = u32::try_from(data.len()).map_err(|_| {
        format!("H264 bitstream length {} exceeds oneVPL limit", data.len())
    })?;
    let mut bitstream = unsafe { std::mem::zeroed::<vpl::mfxBitstream>() };
    bitstream.Data = data.as_ptr() as *mut u8;
    bitstream.DataLength = len;
    bitstream.MaxLength = len;
    bitstream.TimeStamp = pts.unwrap_or(NO_TIMESTAMP);
    Ok(bitstream)
}

fn output_pts(timestamp: u64, fallback_pts: Option<u64>) -> Option<u64> {
    if timestamp == NO_TIMESTAMP { fallback_pts } else { Some(timestamp) }
}

struct DecoderLayout {
    visible: VideoResolution,
    coded: VideoResolution,
}

fn decoder_layout(video_param: &vpl::mfxVideoParam) -> Result<DecoderLayout, String> {
    unsafe {
        let frame_info = video_param.__bindgen_anon_1.mfx.FrameInfo;
        let dims = frame_info.__bindgen_anon_1.__bindgen_anon_1;
        Ok(DecoderLayout {
            visible: VideoResolution {
                width: visible_dimension("width", dims.CropW, dims.Width)?,
                height: visible_dimension("height", dims.CropH, dims.Height)?,
            },
            coded: VideoResolution {
                width: coded_dimension("width", dims.Width)?,
                height: coded_dimension("height", dims.Height)?,
            },
        })
    }
}

fn coded_dimension(name: &str, coded: u16) -> Result<u32, String> {
    if coded == 0 {
        Err(format!("Intel Quick Sync H264 decoder reported zero coded {name}"))
    } else {
        Ok(u32::from(coded))
    }
}

fn visible_dimension(name: &str, crop: u16, coded: u16) -> Result<u32, String> {
    let dimension = if crop == 0 { coded } else { crop };
    if dimension == 0 {
        Err(format!("Intel Quick Sync H264 decoder reported zero visible {name}"))
    } else {
        Ok(u32::from(dimension))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_pts_coalesces_fragments_from_same_frame() {
        let mut pending_pts = PendingPts::default();

        pending_pts.queue(Some(10));
        pending_pts.queue(Some(10));
        pending_pts.queue(Some(11));
        pending_pts.queue(None);

        assert_eq!(pending_pts.pop(), Some(10));
        assert_eq!(pending_pts.pop(), Some(11));
        assert_eq!(pending_pts.pop(), None);
    }

    #[test]
    fn output_pts_prefers_driver_timestamp() {
        assert_eq!(output_pts(42, Some(7)), Some(42));
    }

    #[test]
    fn output_pts_uses_fallback_for_missing_driver_timestamp() {
        assert_eq!(output_pts(NO_TIMESTAMP, Some(7)), Some(7));
    }
}
