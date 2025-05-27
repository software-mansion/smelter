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

use super::OutPixelFormat;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Options {
    pub resolution: Resolution,
    pub pixel_format: OutPixelFormat,
    pub raw_options: Vec<(String, String)>,
}

pub struct LibavVP8Encoder {
    resolution: Resolution,
    pixel_format: OutputFrameFormat,
    frame_sender: Sender<PipelineEvent<Frame>>,
    keyframe_req_sender: Sender<()>,
}

impl LibavVP8Encoder {
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
                    "vp8 ffmpeg encoder",
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

        result_receiver.recv().unwrap()?;

        Ok(Self {
            frame_sender,
            resolution: options.resolution,
            pixel_format: options.pixel_format.into(),
            keyframe_req_sender,
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
}

fn run_encoder_thread(
    options: Options,
    framerate: Framerate,
    frame_receiver: Receiver<PipelineEvent<Frame>>,
    keyframe_req_receiver: Receiver<()>,
    packet_sender: Sender<EncoderOutputEvent>,
    result_sender: &Sender<Result<(), EncoderInitError>>,
) -> Result<(), EncoderInitError> {
    let codec = ffmpeg_next::codec::encoder::find(Id::VP8).ok_or(EncoderInitError::NoCodec)?;

    let mut encoder = Context::new().encoder().video()?;

    // We set this to 1 / 1_000_000, bc we use `as_micros` to convert frames to AV packets.
    let pts_unit_secs = Rational::new(1, 1_000_000);
    encoder.set_time_base(pts_unit_secs);
    encoder.set_format(Pixel::YUV420P);
    encoder.set_width(options.resolution.width as u32);
    encoder.set_height(options.resolution.height as u32);
    encoder.set_frame_rate(Some((framerate.num as i32, framerate.den as i32)));

    let defaults = [
        // Quality/Speed ratio modifier
        ("cpu-used", "0"),
        // Time to spend encoding.
        ("deadline", "realtime"),
        // Auto threads number used.
        ("threads", "0"),
        // Zero-latency. Disables frame reordering.
        ("lag-in-frames", "0"),
    ];

    let encoder_opts_iter = merge_options_with_defaults(&defaults, &options.raw_options);
    let mut encoder = encoder.open_as_with(codec, Dictionary::from_iter(encoder_opts_iter))?;

    result_sender.send(Ok(())).unwrap();

    let mut packet = Packet::empty();

    loop {
        let frame = match frame_receiver.recv() {
            Ok(PipelineEvent::Data(f)) => f,
            Ok(PipelineEvent::EOS) => break,
            Err(_) => break,
        };

        let mut av_frame = frame::Video::new(
            Pixel::YUV420P,
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
                warn!("Failed to send encoded video from VP8 encoder. Channel closed.");
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
            warn!("Failed to send encoded video from VP8 encoder. Channel closed.");
            return Ok(());
        }
    }

    if let Err(_err) = packet_sender.send(EncoderOutputEvent::VideoEOS) {
        warn!("Failed to send EOS from VP8 encoder. Channel closed.")
    }
    Ok(())
}

fn receive_chunk(encoder: &mut Video, packet: &mut Packet) -> Option<EncodedChunk> {
    match encoder.receive_packet(packet) {
        Ok(_) => {
            match encoded_chunk_from_av_packet(
                packet,
                EncodedChunkKind::Video(VideoCodec::VP8),
                1_000_000,
            ) {
                Ok(chunk) => {
                    trace!(pts=?packet.pts(), "VP8 encoder produced an encoded packet.");
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
    let FrameData::PlanarYuv420(data) = frame.data else {
        return Err(FrameConversionError(format!(
            "Unsupported pixel format {:?}",
            frame.data
        )));
    };
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
