use std::time::Duration;

use anyhow::{Context, Result};
use bytes::BytesMut;
use compositor_render::{Frame, FrameData, Resolution, YuvPlanes};
use ffmpeg_next::{
    codec::{Context as FfmpegContext, Id},
    decoder,
    ffi::AV_CODEC_FLAG2_CHUNKS,
    frame,
    media::Type,
    Rational,
};
use rtp::{codecs::h264::H264Packet, packetizer::Depacketizer};

pub struct VideoDecoder {
    depayloader: H264Packet,
    decoder: decoder::Opened,
    decoded_frames: Vec<Frame>,
    first_rtp_timestamp: Option<u32>,
}

impl VideoDecoder {
    pub fn new() -> Result<Self> {
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();

            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::H264.into();
        };

        let mut decoder = FfmpegContext::from_parameters(parameters.clone())?;
        unsafe {
            (*decoder.as_mut_ptr()).flags2 |= AV_CODEC_FLAG2_CHUNKS;
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, 1_000_000).into();
        }

        let decoder = decoder
            .decoder()
            .open_as(Into::<Id>::into(parameters.id()))?;

        Ok(Self {
            decoder,
            depayloader: H264Packet::default(),
            decoded_frames: Vec::new(),
            first_rtp_timestamp: None,
        })
    }

    pub fn decode(&mut self, packet: rtp::packet::Packet) -> Result<()> {
        let first_rtp_timestamp = *self
            .first_rtp_timestamp
            .get_or_insert(packet.header.timestamp);
        let pts = (packet.header.timestamp - first_rtp_timestamp) as f64 / 90000.0 * 1_000_000.0;
        let chunk_data = self.depayloader.depacketize(&packet.payload)?;
        if chunk_data.is_empty() {
            return Ok(());
        }

        let mut packet = ffmpeg_next::Packet::new(chunk_data.len());
        packet.data_mut().unwrap().copy_from_slice(&chunk_data);
        packet.set_pts(Some(pts as i64));
        packet.set_dts(None);

        self.decoder.send_packet(&packet)?;
        self.receive_decoded_frames()
    }

    pub fn take_frames(mut self) -> Result<Vec<Frame>> {
        self.receive_decoded_frames()?;
        Ok(self.decoded_frames)
    }

    fn receive_decoded_frames(&mut self) -> Result<()> {
        let mut decoded_frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut decoded_frame).is_ok() {
            let data = FrameData::PlanarYuv420(YuvPlanes {
                y_plane: copy_plane_from_av(&decoded_frame, 0),
                u_plane: copy_plane_from_av(&decoded_frame, 1),
                v_plane: copy_plane_from_av(&decoded_frame, 2),
            });
            let resolution = Resolution {
                width: decoded_frame.width().try_into()?,
                height: decoded_frame.height().try_into()?,
            };
            let pts = decoded_frame.pts().context("missing pts")?;
            if pts < 0 {
                return Err(anyhow::anyhow!("negative pts"));
            }

            self.decoded_frames.push(Frame {
                data,
                resolution,
                pts: Duration::from_micros(pts as u64),
            });
        }

        Ok(())
    }
}

fn copy_plane_from_av(decoded: &frame::Video, plane: usize) -> bytes::Bytes {
    let mut output_buffer = BytesMut::with_capacity(
        decoded.plane_width(plane) as usize * decoded.plane_height(plane) as usize,
    );

    decoded
        .data(plane)
        .chunks(decoded.stride(plane))
        .map(|chunk| &chunk[..decoded.plane_width(plane) as usize])
        .for_each(|chunk| output_buffer.extend_from_slice(chunk));

    output_buffer.freeze()
}
