use std::{
    io::Read,
    sync::{mpsc, Arc},
};

use h264_reader::{
    nal::{slice::SliceHeader, sps::SeqParameterSet, Nal, RefNal},
    push::{AccumulatedNalHandler, NalInterest},
    Context,
};

use super::{ParsedNalu, ParserError};

pub(crate) struct NalReceiver {
    parser_ctx: h264_reader::Context,
    sender: mpsc::Sender<Result<ParsedNalu, ParserError>>,
}

impl AccumulatedNalHandler for NalReceiver {
    fn nal(&mut self, nal: RefNal<'_>) -> NalInterest {
        if !nal.is_complete() {
            return NalInterest::Buffer;
        }

        let result = self.handle_nal(nal);
        self.sender.send(result).unwrap();

        NalInterest::Ignore
    }
}

impl NalReceiver {
    pub(crate) fn new(sender: mpsc::Sender<Result<ParsedNalu, ParserError>>) -> Self {
        Self {
            sender,
            parser_ctx: Context::default(),
        }
    }

    fn handle_nal(&mut self, nal: RefNal<'_>) -> Result<ParsedNalu, ParserError> {
        let nal_unit_type = nal
            .header()
            .map_err(ParserError::NalHeaderParseError)?
            .nal_unit_type();

        match nal_unit_type {
            h264_reader::nal::UnitType::SeqParameterSet => {
                let parsed = h264_reader::nal::sps::SeqParameterSet::from_bits(nal.rbsp_bits())
                    .map_err(ParserError::SpsParseError)?;

                // Perhaps this shouldn't be here, but this is the only place we process sps
                // before sending them to the decoder. It also seems that this is the only thing we
                // need to check about the sps.
                if parsed.gaps_in_frame_num_value_allowed_flag {
                    // TODO: what else to do here? sure we'll throw an error, but shouldn't we also
                    // terminate the parser somehow?
                    // perhaps this should be considered in other places we throw errors too
                    Err(ParserError::GapsInFrameNumNotSupported)
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
                .map_err(ParserError::PpsParseError)?;

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
                .map_err(ParserError::SliceParseError)?;

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

pub(crate) trait SpsExt {
    fn max_frame_num(&self) -> i64;
}

impl SpsExt for SeqParameterSet {
    fn max_frame_num(&self) -> i64 {
        1 << self.log2_max_frame_num()
    }
}

#[derive(derivative::Derivative)]
#[derivative(Debug, Clone)]
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
