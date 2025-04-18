use std::time::Duration;

use crate::{
    error::InputInitError,
    pipeline::{
        types::{EncodedChunk, EncodedChunkKind, VideoCodec},
        PipelineCtx,
    },
    queue::PipelineEvent,
};

use compositor_render::{Frame, FrameData, InputId, Resolution, YuvPlanes};
use crossbeam_channel::{Receiver, Sender};
use ffmpeg_next::{
    codec::{Context, Id},
    format::Pixel,
    frame::Video,
    media::Type,
    Rational,
};
use tracing::{debug, error, span, trace, warn, Level};

pub fn start_ffmpeg_decoder_thread(
    _pipeline_ctx: &PipelineCtx,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    input_id: InputId,
    send_eos: bool,
) -> Result<(), InputInitError> {
    let (init_result_sender, init_result_receiver) = crossbeam_channel::bounded(0);

    let mut parameters = ffmpeg_next::codec::Parameters::new();
    unsafe {
        let parameters = &mut *parameters.as_mut_ptr();

        parameters.codec_type = Type::Video.into();
        parameters.codec_id = Id::VP9.into();
    };

    std::thread::Builder::new()
        .name(format!("VP9 decoder {}", input_id.0))
        .spawn(move || {
            let _span =
                span!(Level::INFO, "VP9 decoder", input_id = input_id.to_string()).entered();
            run_decoder_thread(
                parameters,
                init_result_sender,
                chunks_receiver,
                frame_sender,
                send_eos,
            )
        })
        .unwrap();

    init_result_receiver.recv().unwrap()?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum DecoderChunkConversionError {
    #[error(
        "Cannot send a chunk of kind {0:?} to the decoder. The decoder only handles VP9-encoded video."
    )]
    BadPayloadType(EncodedChunkKind),
}

fn run_decoder_thread(
    parameters: ffmpeg_next::codec::Parameters,
    init_result_sender: Sender<Result<(), InputInitError>>,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    send_eos: bool,
) {
    let decoder = Context::from_parameters(parameters.clone())
        .map_err(InputInitError::FfmpegError)
        .and_then(|mut decoder| {
            unsafe {
                // This is because we use microseconds as pts and dts in the packets.
                // See `chunk_to_av` and `frame_from_av`.
                (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, 1_000_000).into();
            }

            let decoder = decoder.decoder();
            decoder
                .open_as(Into::<Id>::into(parameters.id()))
                .map_err(InputInitError::FfmpegError)
        });

    let mut decoder = match decoder {
        Ok(decoder) => {
            init_result_sender.send(Ok(())).unwrap();
            decoder
        }
        Err(err) => {
            init_result_sender.send(Err(err)).unwrap();
            return;
        }
    };

    let mut decoded_frame = ffmpeg_next::frame::Video::empty();
    let mut pts_offset = None;
    for chunk in chunks_receiver {
        let chunk = match chunk {
            PipelineEvent::Data(chunk) => chunk,
            PipelineEvent::EOS => {
                break;
            }
        };
        if chunk.kind != EncodedChunkKind::Video(VideoCodec::VP9) {
            error!("VP9 decoder received chunk of wrong kind: {:?}", chunk.kind);
            continue;
        }

        let av_packet: ffmpeg_next::Packet = match chunk_to_av(chunk) {
            Ok(packet) => packet,
            Err(err) => {
                warn!("Dropping frame: {}", err);
                continue;
            }
        };

        match decoder.send_packet(&av_packet) {
            Ok(()) => {}
            Err(e) => {
                warn!("Failed to send a packet to decoder: {:?}", e);
                continue;
            }
        }

        while decoder.receive_frame(&mut decoded_frame).is_ok() {
            let frame = match frame_from_av(&mut decoded_frame, &mut pts_offset) {
                Ok(frame) => frame,
                Err(err) => {
                    warn!("Dropping frame: {}", err);
                    continue;
                }
            };

            trace!(pts=?frame.pts, "VP9 decoder produced a frame.");
            if frame_sender.send(PipelineEvent::Data(frame)).is_err() {
                debug!("Failed to send frame from VP9 decoder. Channel closed.");
                return;
            }
        }
    }
    if send_eos && frame_sender.send(PipelineEvent::EOS).is_err() {
        debug!("Failed to send EOS from VP9 decoder. Channel closed.")
    }
}

fn chunk_to_av(chunk: EncodedChunk) -> Result<ffmpeg_next::Packet, DecoderChunkConversionError> {
    if chunk.kind != EncodedChunkKind::Video(VideoCodec::VP9) {
        return Err(DecoderChunkConversionError::BadPayloadType(chunk.kind));
    }

    let mut packet = ffmpeg_next::Packet::new(chunk.data.len());

    packet.data_mut().unwrap().copy_from_slice(&chunk.data);
    packet.set_pts(Some(chunk.pts.as_micros() as i64));
    packet.set_dts(chunk.dts.map(|dts| dts.as_micros() as i64));

    Ok(packet)
}

#[derive(Debug, thiserror::Error)]
enum DecoderFrameConversionError {
    #[error("Error converting frame: {0}")]
    FrameConversionError(String),
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedPixelFormat(ffmpeg_next::format::pixel::Pixel),
}

fn frame_from_av(
    decoded: &mut Video,
    pts_offset: &mut Option<i64>,
) -> Result<Frame, DecoderFrameConversionError> {
    let Some(pts) = decoded.pts() else {
        return Err(DecoderFrameConversionError::FrameConversionError(
            "missing pts".to_owned(),
        ));
    };
    if pts < 0 {
        error!(pts, pts_offset, "Received negative PTS. PTS values of the decoder output are not monotonically increasing.")
    }
    let pts = Duration::from_micros(i64::max(pts, 0) as u64);

    // TODO add yuv422 and yuv444
    let data = match decoded.format() {
        Pixel::YUV420P => FrameData::PlanarYuv420(YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        }),
        fmt => return Err(DecoderFrameConversionError::UnsupportedPixelFormat(fmt)),
    };
    Ok(Frame {
        data,
        resolution: Resolution {
            width: decoded.width().try_into().unwrap(),
            height: decoded.height().try_into().unwrap(),
        },
        pts,
    })
}

fn copy_plane_from_av(decoded: &Video, plane: usize) -> bytes::Bytes {
    let mut output_buffer = bytes::BytesMut::with_capacity(
        decoded.plane_width(plane) as usize * decoded.plane_height(plane) as usize,
    );

    decoded
        .data(plane)
        .chunks(decoded.stride(plane))
        .map(|chunk| &chunk[..decoded.plane_width(plane) as usize])
        .for_each(|chunk| output_buffer.extend_from_slice(chunk));

    output_buffer.freeze()
}
