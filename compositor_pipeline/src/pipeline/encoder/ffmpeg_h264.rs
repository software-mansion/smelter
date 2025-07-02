use core::slice;
use std::time::Duration;

use compositor_render::{Frame, FrameData, Framerate, OutputFrameFormat, OutputId, Resolution};
use crossbeam_channel::{Receiver, Sender};
use ffmpeg_next::{
    codec::{Context, Id},
    encoder::Video,
    format::Pixel,
    frame, Dictionary, Packet, Rational,
};
use tracing::{debug, error, span, trace, warn, Level};

use crate::{
    error::EncoderInitError,
    pipeline::types::{
        ChunkFromFfmpegError, EncodedChunk, EncodedChunkKind, EncoderOutputEvent, IsKeyframe,
        VideoCodec,
    },
    queue::PipelineEvent,
};

use super::OutputPixelFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

impl EncoderPreset {
    fn to_str(self) -> &'static str {
        match self {
            EncoderPreset::Ultrafast => "ultrafast",
            EncoderPreset::Superfast => "superfast",
            EncoderPreset::Veryfast => "veryfast",
            EncoderPreset::Faster => "faster",
            EncoderPreset::Fast => "fast",
            EncoderPreset::Medium => "medium",
            EncoderPreset::Slow => "slow",
            EncoderPreset::Slower => "slower",
            EncoderPreset::Veryslow => "veryslow",
            EncoderPreset::Placebo => "placebo",
        }
    }

    fn default_partitions(&self) -> &'static str {
        match self {
            EncoderPreset::Ultrafast => "none",
            EncoderPreset::Superfast => "i8x8,i4x4",
            EncoderPreset::Veryfast => "p8x8,b8x8,i8x8,i4x4",
            EncoderPreset::Faster => "p8x8,b8x8,i8x8,i4x4",
            EncoderPreset::Fast => "p8x8,b8x8,i8x8,i4x4",
            EncoderPreset::Medium => "p8x8,b8x8,i8x8,i4x4",
            EncoderPreset::Slow => "all",
            EncoderPreset::Slower => "all",
            EncoderPreset::Veryslow => "all",
            EncoderPreset::Placebo => "all",
        }
    }

    fn default_subq_mode(&self) -> &'static str {
        match self {
            EncoderPreset::Ultrafast => "0",
            EncoderPreset::Superfast => "1",
            EncoderPreset::Veryfast => "2",
            EncoderPreset::Faster => "4",
            EncoderPreset::Fast => "6",
            EncoderPreset::Medium => "7",
            EncoderPreset::Slow => "8",
            EncoderPreset::Slower => "9",
            EncoderPreset::Veryslow => "10",
            EncoderPreset::Placebo => "11",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Options {
    pub preset: EncoderPreset,
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(String, String)>,
}

pub struct LibavH264Encoder {
    resolution: Resolution,
    pixel_format: OutputFrameFormat,
    frame_sender: Sender<PipelineEvent<Frame>>,
    keyframe_req_sender: Sender<()>,
    extradata: Option<bytes::Bytes>,
}

impl LibavH264Encoder {
    pub fn new(
        output_id: &OutputId,
        options: Options,
        framerate: Framerate,
        chunks_sender: Sender<EncoderOutputEvent>,
    ) -> Result<Self, EncoderInitError> {
        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
        let (result_sender, result_receiver) = crossbeam_channel::bounded(0);
        let (keyframe_req_sender, keyframe_req_receiver) = crossbeam_channel::unbounded();

        let options_clone = options.clone();
        let output_id = output_id.clone();

        std::thread::Builder::new()
            .name(format!("Encoder thread for output {}", output_id))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "h264 ffmpeg encoder",
                    output_id = output_id.to_string()
                )
                .entered();
                let encoder_result = run_encoder_thread(
                    options_clone,
                    framerate,
                    frame_receiver,
                    keyframe_req_receiver,
                    chunks_sender,
                    &result_sender,
                );

                if let Err(err) = encoder_result {
                    warn!(%err, "Encoder thread finished with an error.");
                    if let Err(err) = result_sender.send(Err(err)) {
                        warn!(%err, "Failed to send error info. Result channel already closed.");
                    }
                }
                debug!("Encoder thread finished.");
            })
            .unwrap();

        let encoder_config = result_receiver.recv().unwrap()?;

        Ok(Self {
            frame_sender,
            resolution: options.resolution,
            pixel_format: options.pixel_format.into(),
            keyframe_req_sender,
            extradata: encoder_config,
        })
    }

    pub fn frame_sender(&self) -> &Sender<PipelineEvent<Frame>> {
        &self.frame_sender
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn pixel_format(&self) -> OutputFrameFormat {
        self.pixel_format
    }

    pub fn keyframe_request_sender(&self) -> Sender<()> {
        self.keyframe_req_sender.clone()
    }

    pub fn context(&self) -> Option<bytes::Bytes> {
        self.extradata.clone()
    }
}

fn run_encoder_thread(
    options: Options,
    framerate: Framerate,
    frame_receiver: Receiver<PipelineEvent<Frame>>,
    keyframe_req_receiver: Receiver<()>,
    packet_sender: Sender<EncoderOutputEvent>,
    result_sender: &Sender<Result<Option<bytes::Bytes>, EncoderInitError>>,
) -> Result<(), EncoderInitError> {
    let codec = ffmpeg_next::codec::encoder::find(Id::H264).ok_or(EncoderInitError::NoCodec)?;

    let mut encoder = Context::new().encoder().video()?;

    // We set this to 1 / 1_000_000, bc we use `as_micros` to convert frames to AV packets.
    let pts_unit_secs = Rational::new(1, 1_000_000);
    encoder.set_time_base(pts_unit_secs);
    encoder.set_format(options.pixel_format.into());
    encoder.set_width(options.resolution.width as u32);
    encoder.set_height(options.resolution.height as u32);
    encoder.set_frame_rate(Some((framerate.num as i32, framerate.den as i32)));
    encoder.set_colorspace(ffmpeg_next::color::Space::BT709);
    encoder.set_color_range(ffmpeg_next::color::Range::MPEG);
    unsafe {
        let encoder = encoder.as_mut_ptr();
        use ffmpeg_next::ffi;
        (*encoder).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
        (*encoder).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
    }

    // TODO: audit settings below
    // Those values are copied from somewhere, they have to be set because libx264
    // is throwing an error if it detects default ffmpeg settings.
    let defaults = [
        ("preset", options.preset.to_str()),
        // Quality-based VBR (0-51)
        ("crf", "23"),
        // Override ffmpeg defaults from https://github.com/mirror/x264/blob/eaa68fad9e5d201d42fde51665f2d137ae96baf0/encoder/encoder.c#L674
        // QP curve compression - libx264 defaults to 0.6 (in case of tune=grain to 0.8)
        ("qcomp", "0.6"),
        //  Maximum motion vector search range - libx264 defaults to 16 (in case of placebo
        //  or veryslow preset to 24)
        ("me_range", "16"),
        // Auto number of threads
        ("threads", "0"),
        // Max QP step - libx264 defaults to 4
        ("qdiff", "4"),
        // Min QP - libx264 defaults to 0
        ("qmin", "0"),
        // Max QP - libx264 defaults to QP_MAX = 69
        ("qmax", "69"),
        //  Maximum GOP (Group of Pictures) size - libx264 defaults to 250
        ("g", "250"),
        // QP factor between I and P frames - libx264 defaults to 1.4 (in case of tune=grain to 1.1)
        ("i_qfactor", "1.4"),
        // QP factor between P and B frames - libx264 defaults to 1.4 (in case of tune=grain to 1.1)
        ("f_pb_factor", "1.3"),
        // A comma-separated list of partitions to consider. Possible values: p8x8, p4x4, b8x8, i8x8, i4x4, none, all
        ("partitions", options.preset.default_partitions()),
        // Subpixel motion estimation and mode decision (decision quality: 1=fast, 11=best)
        ("subq", options.preset.default_subq_mode()),
    ];

    let encoder_opts_iter = merge_options_with_defaults(&defaults, &options.raw_options);
    let mut encoder = encoder.open_as_with(codec, Dictionary::from_iter(encoder_opts_iter))?;

    let extradata = unsafe {
        let encoder_ptr = encoder.0 .0 .0.as_ptr();
        let size = (*encoder_ptr).extradata_size;
        if size > 0 {
            let extradata_slice = slice::from_raw_parts((*encoder_ptr).extradata, size as usize);
            Some(bytes::Bytes::copy_from_slice(extradata_slice))
        } else {
            None
        }
    };

    result_sender.send(Ok(extradata)).unwrap();

    let mut packet = Packet::empty();

    loop {
        let frame = match frame_receiver.recv() {
            Ok(PipelineEvent::Data(f)) => f,
            Ok(PipelineEvent::EOS) => break,
            Err(_) => break,
        };

        let mut av_frame = frame::Video::new(
            options.pixel_format.into(),
            options.resolution.width as u32,
            options.resolution.height as u32,
        );

        if let Err(e) = frame_into_av(frame, &mut av_frame) {
            error!(
                "Failed to convert a frame to an ffmpeg frame: {}. Dropping",
                e.0
            );
            continue;
        }

        if keyframe_req_receiver.try_recv().is_ok() {
            av_frame.set_kind(ffmpeg_next::picture::Type::I);
        }

        if let Err(e) = encoder.send_frame(&av_frame) {
            error!("Encoder error: {e}.");
            continue;
        }

        while let Some(chunk) = receive_chunk(&mut encoder, &mut packet) {
            if packet_sender.send(EncoderOutputEvent::Data(chunk)).is_err() {
                warn!("Failed to send encoded video from H264 encoder. Channel closed.");
                return Ok(());
            }
        }
    }

    // Flush the encoder
    if let Err(e) = encoder.send_eof() {
        error!("Failed to enter draining mode on encoder: {e}.");
    }
    while let Some(chunk) = receive_chunk(&mut encoder, &mut packet) {
        if packet_sender.send(EncoderOutputEvent::Data(chunk)).is_err() {
            warn!("Failed to send encoded video from H264 encoder. Channel closed.");
            return Ok(());
        }
    }

    if let Err(_err) = packet_sender.send(EncoderOutputEvent::VideoEOS) {
        warn!("Failed to send EOS from H264 encoder. Channel closed.")
    }
    Ok(())
}

fn receive_chunk(encoder: &mut Video, packet: &mut Packet) -> Option<EncodedChunk> {
    match encoder.receive_packet(packet) {
        Ok(_) => {
            match encoded_chunk_from_av_packet(
                packet,
                EncodedChunkKind::Video(VideoCodec::H264),
                1_000_000,
            ) {
                Ok(chunk) => {
                    trace!(pts=?packet.pts(), ?chunk, "H264 encoder produced an encoded packet.");
                    Some(chunk)
                }
                Err(e) => {
                    warn!("failed to parse an ffmpeg packet received from encoder: {e}",);
                    None
                }
            }
        }

        Err(ffmpeg_next::Error::Eof) => None,

        Err(ffmpeg_next::Error::Other {
            errno: ffmpeg_next::error::EAGAIN,
        }) => None, // encoder needs more frames to produce a packet

        Err(e) => {
            error!("Encoder error: {e}.");
            None
        }
    }
}

#[derive(Debug)]
struct FrameConversionError(String);

fn frame_into_av(frame: Frame, av_frame: &mut frame::Video) -> Result<(), FrameConversionError> {
    let (data, expected_pixel_format) = match frame.data {
        FrameData::PlanarYuv420(data) => (data, Pixel::YUV420P),
        FrameData::PlanarYuv422(data) => (data, Pixel::YUV422P),
        FrameData::PlanarYuv444(data) => (data, Pixel::YUV444P),
        _ => {
            return Err(FrameConversionError(format!(
                "Unsupported pixel format {:?}",
                frame.data
            )))
        }
    };

    if av_frame.format() != expected_pixel_format {
        return Err(FrameConversionError(format!(
            "Frame format mismatch: expected {:?}, got {:?}",
            expected_pixel_format,
            av_frame.format()
        )));
    }

    let expected_y_plane_size = (av_frame.plane_width(0) * av_frame.plane_height(0)) as usize;
    let expected_u_plane_size = (av_frame.plane_width(1) * av_frame.plane_height(1)) as usize;
    let expected_v_plane_size = (av_frame.plane_width(2) * av_frame.plane_height(2)) as usize;
    if expected_y_plane_size != data.y_plane.len() {
        return Err(FrameConversionError(format!(
            "Y plane is a wrong size, expected: {} received: {}",
            expected_y_plane_size,
            data.y_plane.len()
        )));
    }
    if expected_u_plane_size != data.u_plane.len() {
        return Err(FrameConversionError(format!(
            "U plane is a wrong size, expected: {} received: {}",
            expected_u_plane_size,
            data.u_plane.len()
        )));
    }
    if expected_v_plane_size != data.v_plane.len() {
        return Err(FrameConversionError(format!(
            "V plane is a wrong size, expected: {} received: {}",
            expected_v_plane_size,
            data.v_plane.len()
        )));
    }

    av_frame.set_pts(Some(frame.pts.as_micros() as i64));

    write_plane_to_av(av_frame, 0, &data.y_plane);
    write_plane_to_av(av_frame, 1, &data.u_plane);
    write_plane_to_av(av_frame, 2, &data.v_plane);

    Ok(())
}

fn write_plane_to_av(frame: &mut frame::Video, plane: usize, data: &[u8]) {
    let stride = frame.stride(plane);
    let width = frame.plane_width(plane) as usize;

    data.chunks(width)
        .zip(frame.data_mut(plane).chunks_mut(stride))
        .for_each(|(data, target)| target[..width].copy_from_slice(data));
}

fn merge_options_with_defaults<'a>(
    defaults: &'a [(&str, &str)],
    overrides: &'a [(String, String)],
) -> impl Iterator<Item = (&'a str, &'a str)> {
    defaults
        .iter()
        .copied()
        .filter(|(key, _value)| {
            // filter out any defaults that are in overrides
            !overrides
                .iter()
                .any(|(override_key, _)| key == override_key)
        })
        .chain(
            overrides
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
}

fn encoded_chunk_from_av_packet(
    value: &ffmpeg_next::Packet,
    kind: EncodedChunkKind,
    timescale: i64,
) -> Result<EncodedChunk, ChunkFromFfmpegError> {
    let data = match value.data() {
        Some(data) => bytes::Bytes::copy_from_slice(data),
        None => return Err(ChunkFromFfmpegError::NoData),
    };

    let rescale = |v: i64| Duration::from_secs_f64((v as f64) * (1.0 / timescale as f64));

    Ok(EncodedChunk {
        data,
        pts: value
            .pts()
            .map(rescale)
            .ok_or(ChunkFromFfmpegError::NoPts)?,
        dts: value.dts().map(rescale),
        is_keyframe: if value.flags().contains(ffmpeg_next::packet::Flags::KEY) {
            IsKeyframe::Yes
        } else {
            IsKeyframe::No
        },
        kind,
    })
}
