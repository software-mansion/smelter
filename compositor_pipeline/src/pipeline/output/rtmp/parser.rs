use std::sync::mpsc;

use h264_reader::{
    annexb::AnnexBReader,
    nal::{pps::PicParameterSet, sps::SeqParameterSet},
    push::NalAccumulator,
};
use nalu_parser::{NalReceiver, Slice};
use nalu_splitter::NALUSplitter;

mod nalu_parser;
mod nalu_splitter;

#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    #[error("Bitstreams that allow gaps in frame_num are not supported")]
    GapsInFrameNumNotSupported,


    #[error("Error while parsing a NAL header: {0:?}")]
    NalHeaderParseError(h264_reader::nal::NalHeaderError),

    #[error("Error while parsing SPS: {0:?}")]
    SpsParseError(h264_reader::nal::sps::SpsError),

    #[error("Error while parsing PPS: {0:?}")]
    PpsParseError(h264_reader::nal::pps::PpsError),

    #[error("Error while parsing a slice: {0:?}")]
    SliceParseError(h264_reader::nal::slice::SliceHeaderError),
}

#[derive(Debug, Clone)]
// one variant of this enum is only ever printed out in debug mode, but clippy detects this as it not being
// used.
#[allow(dead_code)]
pub enum ParsedNalu {
    Sps(SeqParameterSet),
    Pps(PicParameterSet),
    Slice(Slice),
    Other(String),
}

pub struct Parser {
    reader: AnnexBReader<NalAccumulator<NalReceiver>>,
    receiver: mpsc::Receiver<Result<ParsedNalu, ParserError>>,
    nalu_splitter: NALUSplitter,
}

impl Default for Parser {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();

        Parser {
            reader: AnnexBReader::accumulate(NalReceiver::new(tx)),
            receiver: rx,
            nalu_splitter: NALUSplitter::default(),
        }
    }
}

impl Parser {
    pub fn parse(
        &mut self,
        bytes: &[u8],
        pts: u64,
    ) -> Vec<(Result<ParsedNalu, ParserError>, Option<u64>)> {
        let nalus = self.nalu_splitter.push(bytes, Some(pts));
        let nalus = nalus
            .into_iter()
            .map(|(nalu, pts)| {
                self.reader.push(&nalu);
                (self.receiver.try_recv().unwrap(), pts)
            })
            .collect::<Vec<_>>();

        nalus
    }
}
