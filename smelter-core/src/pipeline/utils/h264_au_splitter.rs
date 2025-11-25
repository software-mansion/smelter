use std::time::Duration;

use bytes::BytesMut;
use tracing::debug;
use vk_video::parser::h264::{AccessUnit, H264Parser, ParsedNalu, nal_types::slice::SliceFamily};

use crate::prelude::*;

#[derive(Default)]
pub struct H264AuSplitter {
    parser: H264Parser,
    prev_ref_frame_num: u16,
    detected_missed_frames: bool,
}

impl H264AuSplitter {
    pub fn put_chunk(
        &mut self,
        chunk: EncodedInputChunk,
    ) -> Result<Vec<EncodedInputChunk>, AuSplitterError> {
        if MediaKind::Video(VideoCodec::H264) != chunk.kind {
            return Err(AuSplitterError::UnsupportedMediaKind(chunk.kind));
        }

        let access_units = self
            .parser
            .parse(&chunk.data, Some(chunk.pts.as_micros() as u64))?;

        self.process_au(access_units)
    }

    pub fn flush(&mut self) -> Result<Vec<EncodedInputChunk>, AuSplitterError> {
        let access_units = self.parser.flush()?;
        self.process_au(access_units)
    }

    fn process_au(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<EncodedInputChunk>, AuSplitterError> {
        let mut chunks = Vec::new();
        for au in access_units {
            self.verify_access_unit(&au)?;

            let mut data = BytesMut::new();
            let pts = match au.0.first().and_then(|nalu| nalu.pts) {
                Some(pts) => pts,
                None => {
                    debug!("Expected access unit with pts. Skipping the access unit");
                    continue;
                }
            };

            // Parser returns nalus which may not start with a start code
            // but each nalu always ends with the start code of the next nalu,
            // so we have to make sure that there is a start code in the beginning
            const START_CODES: [&[u8]; 2] = [&[0, 0, 0, 1], &[0, 0, 1]];
            if let Some(first_nalu) = au.0.first() {
                let has_start_code = START_CODES
                    .iter()
                    .any(|code| first_nalu.raw_bytes.starts_with(code));
                if !has_start_code {
                    data.extend_from_slice(&[0, 0, 1]);
                }
            }

            for nalu in au.0.iter() {
                data.extend_from_slice(&nalu.raw_bytes);
            }

            chunks.push(EncodedInputChunk {
                data: data.freeze(),
                pts: Duration::from_micros(pts),
                dts: None,
                kind: MediaKind::Video(VideoCodec::H264),
            });
        }

        Ok(chunks)
    }

    pub fn mark_missing_data(&mut self) {
        self.detected_missed_frames = true;
    }

    fn verify_access_unit(&mut self, au: &AccessUnit) -> Result<(), AuSplitterError> {
        let Some(ParsedNalu::Slice(slice)) =
            au.0.iter()
                .map(|nalu| &nalu.parsed)
                .find(|nalu| matches!(nalu, ParsedNalu::Slice(_)))
        else {
            return Err(AuSplitterError::InvalidAccessUnit);
        };

        match slice.header.slice_type.family {
            SliceFamily::P | SliceFamily::B => {
                if self.detected_missed_frames {
                    return Err(AuSplitterError::MissingReferenceFrame);
                }
                let sps = &slice.sps;
                let frame_num = slice.header.frame_num;
                let max_frame_num = 1i64 << sps.log2_max_frame_num();

                let is_expected_frame_num = !sps.gaps_in_frame_num_value_allowed_flag
                    && frame_num != self.prev_ref_frame_num
                    && frame_num != ((self.prev_ref_frame_num as i64 + 1) % max_frame_num) as u16;
                if is_expected_frame_num {
                    debug!("AUSplitter detected missing frame");
                    self.detected_missed_frames = true;
                    return Err(AuSplitterError::MissingReferenceFrame);
                }

                self.prev_ref_frame_num = frame_num;
            }
            SliceFamily::I => {
                self.prev_ref_frame_num = slice.header.frame_num;
                self.detected_missed_frames = false;
            }
            SliceFamily::SP | SliceFamily::SI => {} // Not supported
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuSplitterError {
    #[error("Missing reference frame")]
    MissingReferenceFrame,

    #[error("Could not parse H264 chunk: {0}")]
    ParserError(#[from] vk_video::parser::h264::H264ParserError),

    #[error("Invalid access unit")]
    InvalidAccessUnit,

    #[error("Unsupported media kind {0:?}")]
    UnsupportedMediaKind(MediaKind),
}
