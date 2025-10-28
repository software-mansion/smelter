use std::sync::{Arc, mpsc};

use au_splitter::AUSplitter;
use h264_reader::{
    annexb::AnnexBReader,
    nal::{pps::PicParameterSet, slice::SliceHeader, sps::SeqParameterSet},
    push::NalAccumulator,
};
use nalu_parser::{NalReceiver, ParsedNalu};
use nalu_splitter::NALUSplitter;
use reference_manager::ReferenceContext;

pub(crate) use reference_manager::ReferenceId;
pub use reference_manager::ReferenceManagementError;

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

pub struct Parser {
    reader: AnnexBReader<NalAccumulator<NalReceiver>>,
    reference_ctx: ReferenceContext,
    au_splitter: AUSplitter,
    receiver: mpsc::Receiver<Result<ParsedNalu, ParserError>>,
    nalu_splitter: NALUSplitter,
}

impl Parser {
    pub fn new(allow_gaps_in_frames: bool) -> Self {
        let (tx, rx) = mpsc::channel();

        Parser {
            reader: AnnexBReader::accumulate(NalReceiver::new(tx)),
            reference_ctx: ReferenceContext::new(allow_gaps_in_frames),
            au_splitter: AUSplitter::default(),
            receiver: rx,
            nalu_splitter: NALUSplitter::default(),
        }
    }

    pub fn parse(
        &mut self,
        bytes: &[u8],
        pts: Option<u64>,
    ) -> Result<Vec<DecoderInstruction>, ParserError> {
        let nalus = self.nalu_splitter.push(bytes, pts);
        let nalus = nalus
            .into_iter()
            .map(|(nalu, pts)| {
                self.reader.push(&nalu);
                (self.receiver.try_recv().unwrap(), pts)
            })
            .collect::<Vec<_>>();

        let mut instructions = Vec::new();
        for (nalu, pts) in nalus {
            let nalu = nalu?;

            let Some(nalus) = self.au_splitter.put_nalu(nalu, pts) else {
                continue;
            };

            let mut slices = Vec::new();
            for (nalu, pts) in nalus {
                match nalu {
                    ParsedNalu::Sps(seq_parameter_set) => {
                        instructions.push(DecoderInstruction::Sps(seq_parameter_set))
                    }
                    ParsedNalu::Pps(pic_parameter_set) => {
                        instructions.push(DecoderInstruction::Pps(pic_parameter_set))
                    }
                    ParsedNalu::Slice(slice) => {
                        slices.push((slice, pts));
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
