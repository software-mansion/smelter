use std::sync::{Arc, mpsc};

use au_splitter::AUSplitter;
use h264_reader::{
    annexb::AnnexBReader,
    nal::{pps::PicParameterSet, slice::SliceHeader, sps::SeqParameterSet},
    push::NalAccumulator,
};
use nalu_parser::NalReceiver;
use nalu_splitter::NALUSplitter;

pub use reference_manager::ReferenceManagementError;
pub(crate) use reference_manager::{ReferenceContext, ReferenceId};

pub use nalu_parser::{ParsedNalu, Slice};

use crate::parameters::MissedFrameHandling;

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
    au_splitter: AUSplitter,
    receiver: mpsc::Receiver<Result<ParsedNalu, ParserError>>,
    nalu_splitter: NALUSplitter,
}

impl Parser {
    // TODO: Make it default
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        Parser {
            reader: AnnexBReader::accumulate(NalReceiver::new(tx)),
            au_splitter: AUSplitter::default(),
            receiver: rx,
            nalu_splitter: NALUSplitter::default(),
        }
    }

    pub fn parse(
        &mut self,
        bytes: &[u8],
        pts: Option<u64>,
    ) -> Result<Vec<Vec<(ParsedNalu, Option<u64>)>>, ParserError> {
        let nalus = self.nalu_splitter.push(bytes, pts);
        let nalus = nalus
            .into_iter()
            .map(|(nalu, pts)| {
                self.reader.push(&nalu);
                (self.receiver.try_recv().unwrap(), pts)
            })
            .collect::<Vec<_>>();

        let mut parsed_nalus = Vec::new();
        for (nalu, pts) in nalus {
            let nalu = nalu?;

            let Some(nalus) = self.au_splitter.put_nalu(nalu, pts) else {
                continue;
            };

            parsed_nalus.push(nalus);
        }

        Ok(parsed_nalus)
    }
}
