use std::time::Duration;

use bytes::BytesMut;
use vk_video::{AccessUnit, H264SliceFamily, ParsedNalu};

use crate::prelude::*;

// TODO: H264 only
#[derive(Default)]
pub struct AUSplitter {
    // TODO: Won't compile on macos
    parser: vk_video::H264Parser,
    prev_ref_frame_num: u16,
    detected_missed_frames: bool,
}

impl AUSplitter {
    pub fn put_chunk(
        &mut self,
        chunk: EncodedInputChunk,
    ) -> Result<Vec<EncodedInputChunk>, AUSplitterError> {
        if MediaKind::Video(VideoCodec::H264) != chunk.kind {
            return Err(AUSplitterError::UnsupportedMediaKind(chunk.kind));
        }

        tracing::info!("CHUNK: {:?}", &chunk.data[0..5]);
        let access_units = self
            .parser
            .parse(&chunk.data, Some(chunk.pts.as_micros() as u64))?;

        let mut chunks = Vec::new();
        for au in access_units {
            self.verify_access_unit(&au)?;

            let mut data = BytesMut::new();
            let pts = au.0.first().and_then(|nalu| nalu.pts);
            for nalu in au.0 {
                tracing::info!("Beginning: {:?}", &nalu.raw[0..5]);
                // TODO: This shouldn't be needed
                if &nalu.raw[..5] != &[0, 0, 0, 1] && &nalu.raw[..4] != &[0, 0, 1] {
                    data.extend_from_slice(&[0, 0, 1]);
                }
                data.extend_from_slice(&nalu.raw);
            }

            // TODO: No DTS, is it ok?
            if let Some(pts) = pts {
                chunks.push(EncodedInputChunk {
                    data: data.freeze(),
                    pts: Duration::from_micros(pts),
                    dts: None,
                    kind: MediaKind::Video(VideoCodec::H264),
                });
            }
        }

        Ok(chunks)
    }

    fn verify_access_unit(&mut self, au: &AccessUnit) -> Result<(), AUSplitterError> {
        let Some(ParsedNalu::Slice(slice)) =
            au.0.iter()
                .map(|nalu| &nalu.parsed)
                .find(|nalu| matches!(nalu, ParsedNalu::Slice(_)))
        else {
            return Err(AUSplitterError::InvalidAccessUnit);
        };

        match slice.header.slice_type.family {
            H264SliceFamily::P | H264SliceFamily::B => {
                let sps = &slice.sps;
                let frame_num = slice.header.frame_num;
                let max_frame_num = 1i64 << sps.log2_max_frame_num();

                let is_expected_frame_num = !sps.gaps_in_frame_num_value_allowed_flag
                    && frame_num != self.prev_ref_frame_num
                    && frame_num != ((self.prev_ref_frame_num as i64 + 1) % max_frame_num) as u16;
                if is_expected_frame_num || self.detected_missed_frames {
                    tracing::error!("Missing frame detected");
                    self.detected_missed_frames = true;
                    return Err(AUSplitterError::MissingReferenceFrame);
                }

                self.prev_ref_frame_num = frame_num;
            }
            H264SliceFamily::I => {
                tracing::warn!("IDR frame received");
                self.prev_ref_frame_num = 0;
                self.detected_missed_frames = false;
            }
            H264SliceFamily::SP | H264SliceFamily::SI => {} // Not supported
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AUSplitterError {
    #[error("Missing reference frame")]
    MissingReferenceFrame,

    #[error("Could not parse H264 chunk: {0}")]
    ParserError(#[from] vk_video::ParserError),

    #[error("Invalid access unit")]
    InvalidAccessUnit,

    #[error("Unsupported media kind {0:?}")]
    UnsupportedMediaKind(MediaKind),
}
