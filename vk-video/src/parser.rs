use std::sync::Arc;

use crate::parser::h264::{AccessUnit, ParsedNalu};
use h264_reader::nal::{pps::PicParameterSet, slice::SliceHeader, sps::SeqParameterSet};

pub use reference_manager::ReferenceManagementError;
pub(crate) use reference_manager::{ReferenceContext, ReferenceId};

mod au_splitter;
mod nalu_parser;
mod nalu_splitter;
mod reference_manager;

pub mod h264 {
    use super::au_splitter::AUSplitter;
    use super::nalu_parser::NalReceiver;
    use super::nalu_splitter::NALUSplitter;
    use h264_reader::annexb::AnnexBReader;
    use h264_reader::push::NalAccumulator;
    use std::sync::mpsc;

    pub use super::au_splitter::AccessUnit;
    pub use super::nalu_parser::{Nalu, ParsedNalu};
    pub use h264_reader::nal as nal_types;

    #[derive(Debug, thiserror::Error)]
    pub enum H264ParserError {
        #[error("Bitstreams that allow gaps in frame_num are not supported")]
        GapsInFrameNumNotSupported,

        #[error("Streams containing fields instead of frames are not supported")]
        FieldsNotSupported,

        #[error("Error while parsing a NAL header: {0:?}")]
        NalHeaderParseError(h264_reader::nal::NalHeaderError),

        #[error("Error while parsing SPS: {0:?}")]
        SpsParseError(h264_reader::nal::sps::SpsError),

        #[error("Error while parsing PPS: {0:?}")]
        PpsParseError(h264_reader::nal::pps::PpsError),

        #[error("Error while parsing a slice: {0:?}")]
        SliceParseError(h264_reader::nal::slice::SliceHeaderError),
    }

    /// H264 parser for Annex B format
    pub struct H264Parser {
        reader: AnnexBReader<NalAccumulator<NalReceiver>>,
        receiver: mpsc::Receiver<Result<ParsedNalu, H264ParserError>>,
        nalu_splitter: NALUSplitter,
        au_splitter: AUSplitter,
    }

    impl Default for H264Parser {
        fn default() -> Self {
            let (tx, rx) = mpsc::channel();

            H264Parser {
                reader: AnnexBReader::accumulate(NalReceiver::new(tx)),
                receiver: rx,
                nalu_splitter: NALUSplitter::default(),
                au_splitter: AUSplitter::default(),
            }
        }
    }

    impl H264Parser {
        /// Parses nalus in Annex B format.
        /// Returns [`AccessUnit`]s representing whole frame
        pub fn parse(
            &mut self,
            bytes: &[u8],
            pts: Option<u64>,
        ) -> Result<Vec<AccessUnit>, H264ParserError> {
            let nalus = self.nalu_splitter.push(bytes, pts);
            let nalus = nalus.into_iter().map(|(nalu_bytes, pts)| {
                self.reader.push(&nalu_bytes);

                let parsed_nalu = self.receiver.try_recv().unwrap();
                parsed_nalu.map(|parsed_nalu| Nalu {
                    parsed: parsed_nalu,
                    raw_bytes: nalu_bytes.into_boxed_slice(),
                    pts,
                })
            });

            let mut access_units = Vec::new();
            for nalu in nalus {
                let nalu = nalu?;

                let Some(au) = self.au_splitter.put_nalu(nalu) else {
                    continue;
                };

                access_units.push(au);
            }

            Ok(access_units)
        }
    }
}

#[derive(Clone, derivative::Derivative)]
#[derivative(Debug)]
#[allow(non_snake_case)]
pub(crate) struct DecodeInformation {
    pub(crate) reference_list: Option<Vec<ReferencePictureInfo>>,
    #[derivative(Debug = "ignore")]
    pub(crate) rbsp_bytes: Vec<u8>,
    pub(crate) slice_indices: Vec<usize>,
    #[derivative(Debug = "ignore")]
    pub(crate) header: Arc<SliceHeader>,
    pub(crate) sps_id: u8,
    pub(crate) pps_id: u8,
    pub(crate) picture_info: PictureInfo,
    pub(crate) pts: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub(crate) struct ReferencePictureInfo {
    pub(crate) id: ReferenceId,
    pub(crate) LongTermPicNum: Option<u64>,
    pub(crate) non_existing: bool,
    pub(crate) FrameNum: u16,
    pub(crate) PicOrderCnt: [i32; 2],
}

#[derive(Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub(crate) struct PictureInfo {
    pub(crate) used_for_long_term_reference: bool,
    pub(crate) non_existing: bool,
    pub(crate) FrameNum: u16,
    pub(crate) PicOrderCnt_for_decoding: [i32; 2],
    pub(crate) PicOrderCnt_as_reference_pic: [i32; 2],
}

#[derive(Debug, Clone)]
pub(crate) enum DecoderInstruction {
    Decode {
        decode_info: DecodeInformation,
        reference_id: ReferenceId,
    },

    Idr {
        decode_info: DecodeInformation,
        reference_id: ReferenceId,
    },

    Drop {
        reference_ids: Vec<ReferenceId>,
    },

    Sps(SeqParameterSet),

    Pps(PicParameterSet),
}

pub(crate) fn compile_to_decoder_instructions(
    reference_ctx: &mut ReferenceContext,
    access_units: Vec<AccessUnit>,
) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
    let mut instructions = Vec::new();
    for AccessUnit(nalus) in access_units {
        let mut slices = Vec::new();
        for nalu in nalus {
            match nalu.parsed {
                ParsedNalu::Sps(seq_parameter_set) => {
                    instructions.push(DecoderInstruction::Sps(seq_parameter_set))
                }
                ParsedNalu::Pps(pic_parameter_set) => {
                    instructions.push(DecoderInstruction::Pps(pic_parameter_set))
                }
                ParsedNalu::Slice(slice) => {
                    slices.push((slice, nalu.pts));
                }

                ParsedNalu::Other(_) => {}
            }
        }

        // TODO: warn when not all pts are equal here
        let mut inst = reference_ctx.put_picture(slices)?;
        instructions.append(&mut inst);
    }

    Ok(instructions)
}
