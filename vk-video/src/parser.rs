use std::sync::{Arc, mpsc};

use au_splitter::AUSplitter;
use h264_reader::{
    annexb::AnnexBReader,
    nal::{pps::PicParameterSet, slice::SliceHeader, sps::SeqParameterSet},
    push::NalAccumulator,
};
use nalu_parser::NalReceiver;
use nalu_splitter::NALUSplitter;
use reference_manager::ReferenceContext;

pub(crate) use reference_manager::ReferenceId;
pub use reference_manager::ReferenceManagementError;

use crate::parameters::MissedFrameHandling;

pub use au_splitter::AccessUnit;
pub use nalu_parser::{Nalu, ParsedNalu};

mod au_splitter;
mod nalu_parser;
mod nalu_splitter;
mod reference_manager;

#[derive(Clone, derivative::Derivative)]
#[derivative(Debug)]
#[allow(non_snake_case)]
pub struct DecodeInformation {
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
pub enum DecoderInstruction {
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

#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    #[error(transparent)]
    ReferenceManagementError(#[from] ReferenceManagementError),

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

// TODO: This is a public API now, document it
pub struct H264Parser {
    reader: AnnexBReader<NalAccumulator<NalReceiver>>,
    receiver: mpsc::Receiver<Result<ParsedNalu, ParserError>>,
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
    // TODO: This is a public API now, document it
    pub fn parse(
        &mut self,
        bytes: &[u8],
        pts: Option<u64>,
    ) -> Result<Vec<AccessUnit>, ParserError> {
        let nalus = self.nalu_splitter.push(bytes, pts);
        let nalus = nalus
            .into_iter()
            .map(|(nalu_bytes, pts)| {
                self.reader.push(&nalu_bytes);

                let parsed_nalu = self.receiver.try_recv().unwrap();
                parsed_nalu.map(|parsed_nalu| Nalu {
                    parsed: parsed_nalu,
                    raw: nalu_bytes,
                    pts,
                })
            })
            .collect::<Vec<_>>();

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

// TODO: Nalu is only h264 abstraction? Maybe drop h264
pub struct H264NaluProcessor {
    reference_ctx: ReferenceContext,
}

impl H264NaluProcessor {
    pub fn new(missed_frame_handling: MissedFrameHandling) -> Self {
        Self {
            reference_ctx: ReferenceContext::new(missed_frame_handling),
        }
    }

    // TODO: Don't use ParserError?
    pub fn process(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<DecoderInstruction>, ParserError> {
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
            let mut inst = self.reference_ctx.put_picture(slices)?;
            instructions.append(&mut inst);
        }

        Ok(instructions)
    }
}
