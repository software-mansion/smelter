use bytes::BytesMut;
use gpu_video::parser::h264::{
    AccessUnit, H264Parser, ParsedNalu,
    nal_types::slice::{
        DecRefPicMarking, MemoryManagementControlOperation, SliceFamily, SliceHeader,
    },
};
use tracing::debug;

use crate::prelude::*;

pub struct H264AuSplitter {
    parser: H264Parser,
    prev_ref_frame_num: u16,
    waiting_for_recovery_point: bool,
    last_recovery_point_pts: Option<Timestamp>,
}

impl Default for H264AuSplitter {
    fn default() -> Self {
        Self {
            parser: H264Parser::default(),
            prev_ref_frame_num: 0,
            // Decoding starts at a recovery point.
            waiting_for_recovery_point: true,
            last_recovery_point_pts: None,
        }
    }
}

impl H264AuSplitter {
    /// Returns one result per access unit, so a rejected one does not drop the others.
    pub fn put_chunk(
        &mut self,
        chunk: EncodedInputChunk,
    ) -> Vec<Result<EncodedInputChunk, AuSplitterError>> {
        if MediaKind::Video(VideoCodec::H264) != chunk.kind {
            return vec![Err(AuSplitterError::UnsupportedMediaKind(chunk.kind))];
        }

        match self
            .parser
            .parse(&chunk.data, Some(chunk.pts.as_nanos() as u64))
        {
            Ok(access_units) => self.process_au(access_units),
            Err(err) => vec![Err(err.into())],
        }
    }

    pub fn flush(&mut self) -> Vec<Result<EncodedInputChunk, AuSplitterError>> {
        match self.parser.flush() {
            Ok(access_units) => self.process_au(access_units),
            Err(err) => vec![Err(err.into())],
        }
    }

    fn process_au(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Vec<Result<EncodedInputChunk, AuSplitterError>> {
        let mut chunks = Vec::new();
        for au in access_units {
            // The parser leaves NALUs that follow the last slice (e.g. filler data) at the front
            // of the next access unit, with the previous pts. Once the parser keeps them in their
            // own access unit, the first NALU can be used again.
            let first_slice =
                au.0.iter()
                    .find(|nalu| matches!(nalu.parsed, ParsedNalu::Slice(_)));
            let pts = match first_slice.and_then(|nalu| nalu.pts) {
                Some(pts) => Timestamp::from_nanos(pts as i64),
                None => {
                    debug!("Expected access unit with pts. Skipping the access unit");
                    continue;
                }
            };

            if let Err(err) = self.verify_access_unit(&au, pts) {
                chunks.push(Err(err));
                continue;
            }

            let mut data = BytesMut::new();
            for nalu in au.0.iter() {
                data.extend_from_slice(&nalu.raw_bytes);
            }

            chunks.push(Ok(EncodedInputChunk {
                data: data.freeze(),
                pts,
                dts: None,
                kind: MediaKind::Video(VideoCodec::H264),
                // Decode-only frames are dropped by `DecodeOnlyFilter` in the decoder stream, based
                // on the pts of the input chunks. Chunks from here only go to the decoder, which
                // ignores this flag.
                decode_only: false,
            }));
        }

        chunks
    }

    pub fn mark_missing_data(&mut self) {
        // Access unit in progress is missing data. If the previous access unit was not followed by
        // `AuDelimiter`, the parser may still hold it, and it is dropped too, even if complete.
        let _ = self.parser.flush();
        self.waiting_for_recovery_point = true;
    }

    /// Stream continues on a new timeline. Decoding has to start again at a recovery point.
    pub fn reset(&mut self) {
        self.prev_ref_frame_num = 0;
        self.waiting_for_recovery_point = true;
        self.last_recovery_point_pts = None;
    }

    /// Drops access units until the next recovery point, keeping the one in progress.
    pub fn wait_for_recovery_point(&mut self, frame_pts: Option<Timestamp>) {
        // Frame decoded before the last recovery point, but output with a reorder delay. Frames
        // after that recovery point can't reference it.
        if let (Some(frame_pts), Some(recovery_point_pts)) =
            (frame_pts, self.last_recovery_point_pts)
            && frame_pts < recovery_point_pts
        {
            return;
        }
        self.waiting_for_recovery_point = true;
    }

    fn verify_access_unit(
        &mut self,
        au: &AccessUnit,
        pts: Timestamp,
    ) -> Result<(), AuSplitterError> {
        let Some(ParsedNalu::Slice(slice)) =
            au.0.iter()
                .map(|nalu| &nalu.parsed)
                .find(|nalu| matches!(nalu, ParsedNalu::Slice(_)))
        else {
            return Err(AuSplitterError::InvalidAccessUnit);
        };

        // `frame_num` the next reference frame continues from. IDR and MMCO 5 restart the
        // numbering.
        let ref_frame_num = match slice.header.is_idr() || slice.header.has_mmco_reset() {
            true => 0,
            false => slice.header.frame_num,
        };

        if Self::is_recovery_point(au) {
            self.prev_ref_frame_num = ref_frame_num;
            self.waiting_for_recovery_point = false;
            self.last_recovery_point_pts = Some(pts);
        } else if slice.header.is_reference_frame() {
            if self.waiting_for_recovery_point {
                return Err(AuSplitterError::MissingReferenceFrame);
            }
            let sps = &slice.sps;
            let frame_num = slice.header.frame_num;
            let max_frame_num = 1i64 << sps.log2_max_frame_num();

            let has_frame_num_gap = !sps.gaps_in_frame_num_value_allowed_flag
                && frame_num != self.prev_ref_frame_num
                && frame_num != ((self.prev_ref_frame_num as i64 + 1) % max_frame_num) as u16;
            if has_frame_num_gap {
                debug!("AUSplitter detected missing frame");
                self.waiting_for_recovery_point = true;
                return Err(AuSplitterError::MissingReferenceFrame);
            }

            self.prev_ref_frame_num = ref_frame_num;
        } else if self.waiting_for_recovery_point {
            // Non-reference frame can still reference the missing one.
            return Err(AuSplitterError::MissingReferenceFrame);
        }

        Ok(())
    }

    /// Whether decoding can start, or resume after a loss, at this access unit. The beginning of
    /// the picture has to be present, and the picture is one of:
    /// - IDR
    /// - Intra reference picture with MMCO 5
    /// - Intra reference picture with SPS `max_num_ref_frames <= 1`
    /// - TODO: Picture with a recovery point SEI, including an intra refresh start
    /// - Any intra reference picture, as a fallback until recovery point SEI is supported
    fn is_recovery_point(au: &AccessUnit) -> bool {
        // Slices of the primary picture. Redundant copies of it don't count.
        let slices = || {
            au.0.iter().filter_map(|nalu| match &nalu.parsed {
                ParsedNalu::Slice(slice) if slice.header.is_primary() => Some(slice),
                _ => None,
            })
        };
        // After a loss, `mark_missing_data` drops the beginning, the rest does not start at the
        // first macroblock.
        let has_beginning = slices().any(|slice| slice.header.first_mb_in_slice == 0);
        if !has_beginning {
            return false;
        }

        let is_idr = slices().all(|slice| slice.header.is_idr());
        if is_idr {
            return true;
        }

        // MMCO 5 drops all references after decoding this picture, so frames after it can't
        // reference frames before it, and the intra picture itself doesn't use any.
        let is_intra_clearing_references =
            slices().all(|slice| slice.header.is_intra() && slice.header.has_mmco_reset());
        if is_intra_clearing_references {
            return true;
        }

        // Only one reference frame can be stored, so storing this picture replaces the previous
        // one.
        let is_intra_single_reference = slices().all(|slice| {
            slice.header.is_reference_frame()
                && slice.header.is_intra()
                && slice.sps.max_num_ref_frames <= 1
        });
        if is_intra_single_reference {
            return true;
        }

        // TODO: Recovery point SEI. Pseudocode:
        //
        //   // gpu-video parser (`NalReceiver::handle_nal`), SEI is `ParsedNalu::Other` now.
        //   // `h264_reader::nal::sei::SeiReader` splits SEI into messages, but does not parse
        //   // the `HeaderType::RecoveryPoint` payload:
        //   //   recovery_frame_cnt: ue(v), exact_match_flag: u(1), broken_link_flag: u(1),
        //   //   changing_slice_group_idc: u(2)
        //   UnitType::SEI => Ok(ParsedNalu::Sei(messages)), // e.g. Vec<Sei::RecoveryPoint { .. }>
        //
        //   // Here, with this method returning `Option<RecoveryPoint>` instead of `bool`:
        //   let recovery_frame_cnt = au.0.iter().find_map(|nalu| match &nalu.parsed {
        //       ParsedNalu::Sei(messages) => messages.iter().find_map(|message| match message {
        //           Sei::RecoveryPoint { recovery_frame_cnt, .. } => Some(*recovery_frame_cnt),
        //           _ => None,
        //       }),
        //       _ => None,
        //   });
        //   if let Some(recovery_frame_cnt) = recovery_frame_cnt {
        //       // 0: e.g. an open GOP keyframe. > 0: intra refresh start, usually not intra.
        //       // Output is correct, in output order, from the reference picture with this
        //       // frame_num on.
        //       let slice = slices().next()?;
        //       let max_frame = 1 << slice.sps.log2_max_frame_num();
        //       let frame_num = (slice.header.frame_num + recovery_frame_cnt) % max_frame;
        //       return Some(RecoveryPoint::Sei { frame_num });
        //   }
        //
        //   // In `verify_access_unit`, until the reference picture with that frame_num:
        //   //   - a loss restarts the wait,
        //   //   - every access unit is sent to the decoder, and its output is hidden,
        //   //   - that picture sets `recovery_pts` to its pts.
        //   // In the decoder, output with pts lower than `recovery_pts` is hidden too. That covers
        //   // leading pictures of an open GOP and partially refreshed pictures of an intra
        //   // refresh. `DecodeOnlyFilter` can't do it, it runs before the splitter.

        // Fallback until recovery point SEI is supported, e.g. an open GOP keyframe or the first
        // frame after a discontinuity. Frames after it can still reference lost frames before it.
        slices().all(|slice| slice.header.is_reference_frame() && slice.header.is_intra())
    }
}

trait SliceHeaderExt {
    /// Other pictures may reference this one (`nal_ref_idc != 0`).
    fn is_reference_frame(&self) -> bool;
    fn is_idr(&self) -> bool;
    /// I or SI slice, no prediction from other pictures.
    fn is_intra(&self) -> bool;
    /// Slice of the primary picture, not of its redundant copy.
    fn is_primary(&self) -> bool;
    /// Picture marks all references as unused after it is decoded (MMCO 5).
    fn has_mmco_reset(&self) -> bool;
}

impl SliceHeaderExt for SliceHeader {
    fn is_reference_frame(&self) -> bool {
        // Present only when `nal_ref_idc != 0`.
        self.dec_ref_pic_marking.is_some()
    }

    fn is_idr(&self) -> bool {
        matches!(self.dec_ref_pic_marking, Some(DecRefPicMarking::Idr { .. }))
    }

    fn is_intra(&self) -> bool {
        matches!(self.slice_type.family, SliceFamily::I | SliceFamily::SI)
    }

    fn is_primary(&self) -> bool {
        self.redundant_pic_cnt.unwrap_or(0) == 0
    }

    fn has_mmco_reset(&self) -> bool {
        match &self.dec_ref_pic_marking {
            Some(DecRefPicMarking::Adaptive(operations)) => operations.iter().any(|operation| {
                matches!(
                    operation,
                    MemoryManagementControlOperation::AllRefPicturesUnused
                )
            }),
            _ => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuSplitterError {
    #[error("Missing reference frame")]
    MissingReferenceFrame,

    #[error("Could not parse H264 chunk: {0}")]
    ParserError(#[from] gpu_video::parser::h264::H264ParserError),

    #[error("Invalid access unit")]
    InvalidAccessUnit,

    #[error("Unsupported media kind {0:?}")]
    UnsupportedMediaKind(MediaKind),
}
