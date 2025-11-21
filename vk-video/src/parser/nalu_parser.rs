use std::{collections::VecDeque, io::Read, sync::Arc};

use h264_reader::{
    Context,
    nal::{Nal, RefNal, pps::PicParameterSet, slice::SliceHeader, sps::SeqParameterSet},
    push::{AccumulatedNalHandler, NalInterest},
};

use super::h264::H264ParserError;

pub(crate) struct NalReceiver {
    parser_ctx: h264_reader::Context,
    results: VecDeque<Result<ParsedNalu, H264ParserError>>,
}

impl AccumulatedNalHandler for NalReceiver {
    fn nal(&mut self, nal: RefNal<'_>) -> NalInterest {
        if !nal.is_complete() {
            return NalInterest::Buffer;
        }

        let result = self.handle_nal(nal);
        self.results.push_back(result);

        NalInterest::Ignore
    }
}

impl NalReceiver {
    pub(crate) fn new() -> Self {
        Self {
            results: VecDeque::new(),
            parser_ctx: Context::default(),
        }
    }

    pub(crate) fn pop_parsed(&mut self) -> VecDeque<Result<ParsedNalu, H264ParserError>> {
        std::mem::take(&mut self.results)
    }

    fn handle_nal(&mut self, nal: RefNal<'_>) -> Result<ParsedNalu, H264ParserError> {
        let nal_unit_type = nal
            .header()
            .map_err(H264ParserError::NalHeaderParseError)?
            .nal_unit_type();

        match nal_unit_type {
            h264_reader::nal::UnitType::SeqParameterSet => {
                let parsed = h264_reader::nal::sps::SeqParameterSet::from_bits(nal.rbsp_bits())
                    .map_err(H264ParserError::SpsParseError)?;

                // Perhaps this shouldn't be here, but this is the only place we process sps
                // before sending them to the decoder. It also seems that this is the only thing we
                // need to check about the sps.
                if parsed.gaps_in_frame_num_value_allowed_flag {
                    // TODO: what else to do here? sure we'll throw an error, but shouldn't we also
                    // terminate the parser somehow?
                    // perhaps this should be considered in other places we throw errors too
                    Err(H264ParserError::GapsInFrameNumNotSupported)
                } else {
                    self.parser_ctx.put_seq_param_set(parsed.clone());
                    Ok(ParsedNalu::Sps(parsed.clone()))
                }
            }

            h264_reader::nal::UnitType::PicParameterSet => {
                let parsed = h264_reader::nal::pps::PicParameterSet::from_bits(
                    &self.parser_ctx,
                    nal.rbsp_bits(),
                )
                .map_err(H264ParserError::PpsParseError)?;

                self.parser_ctx.put_pic_param_set(parsed.clone());

                Ok(ParsedNalu::Pps(parsed.clone()))
            }

            h264_reader::nal::UnitType::SliceLayerWithoutPartitioningNonIdr
            | h264_reader::nal::UnitType::SliceLayerWithoutPartitioningIdr => {
                let (header, sps, pps) = h264_reader::nal::slice::SliceHeader::from_bits(
                    &self.parser_ctx,
                    &mut nal.rbsp_bits(),
                    nal.header().unwrap(),
                )
                .map_err(H264ParserError::SliceParseError)?;

                let header = Arc::new(header);

                let mut rbsp_bytes = vec![0, 0, 0, 1];
                nal.reader().read_to_end(&mut rbsp_bytes).unwrap();
                let slice = Slice {
                    nal_header: nal.header().unwrap(),
                    header,
                    pps_id: pps.pic_parameter_set_id,
                    rbsp_bytes,
                    sps: sps.clone(),
                    pps: pps.clone(),
                };

                Ok(ParsedNalu::Slice(slice))
            }

            h264_reader::nal::UnitType::Unspecified(_)
            | h264_reader::nal::UnitType::SliceDataPartitionALayer
            | h264_reader::nal::UnitType::SliceDataPartitionBLayer
            | h264_reader::nal::UnitType::SliceDataPartitionCLayer
            | h264_reader::nal::UnitType::SEI
            | h264_reader::nal::UnitType::AccessUnitDelimiter
            | h264_reader::nal::UnitType::EndOfSeq
            | h264_reader::nal::UnitType::EndOfStream
            | h264_reader::nal::UnitType::FillerData
            | h264_reader::nal::UnitType::SeqParameterSetExtension
            | h264_reader::nal::UnitType::PrefixNALUnit
            | h264_reader::nal::UnitType::SubsetSeqParameterSet
            | h264_reader::nal::UnitType::DepthParameterSet
            | h264_reader::nal::UnitType::SliceLayerWithoutPartitioningAux
            | h264_reader::nal::UnitType::SliceExtension
            | h264_reader::nal::UnitType::SliceExtensionViewComponent
            | h264_reader::nal::UnitType::Reserved(_) => Ok(ParsedNalu::Other(format!(
                "{:?}",
                nal.header().unwrap().nal_unit_type()
            ))),
        }
    }
}

// It's not used if compiled on macOS, so it's reported as a dead code
#[allow(dead_code)]
pub(crate) trait SpsExt {
    fn max_frame_num(&self) -> i64;
}

impl SpsExt for SeqParameterSet {
    fn max_frame_num(&self) -> i64 {
        1 << self.log2_max_frame_num()
    }
}

#[derive(Debug)]
// one variant of this enum is only ever printed out in debug mode, but clippy detects this as it not being
// used.
#[allow(dead_code)]
pub enum ParsedNalu {
    Sps(SeqParameterSet),
    Pps(PicParameterSet),
    Slice(Slice),
    Other(String),
}

/// H264 Network Abstraction Layer Unit
pub struct Nalu {
    /// Parsed nalu from [`Nalu::raw_bytes`]
    pub parsed: ParsedNalu,
    // Only used if parsers are exposed
    #[allow(dead_code)]
    pub raw_bytes: Box<[u8]>,
    pub pts: Option<u64>,
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct Slice {
    pub nal_header: h264_reader::nal::NalHeader,
    pub pps_id: h264_reader::nal::pps::PicParamSetId,
    pub header: Arc<SliceHeader>,
    #[derivative(Debug = "ignore")]
    pub rbsp_bytes: Vec<u8>,
    #[derivative(Debug = "ignore")]
    pub sps: h264_reader::nal::sps::SeqParameterSet,
    #[derivative(Debug = "ignore")]
    pub pps: h264_reader::nal::pps::PicParameterSet,
}
