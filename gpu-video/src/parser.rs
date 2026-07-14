mod au_splitter;
mod nalu_parser;
mod nalu_splitter;

#[cfg(supported)]
pub(crate) mod decoder_instructions;
#[cfg(supported)]
pub(crate) mod reference_manager;

pub mod h264 {
    use super::au_splitter::AUSplitter;
    use super::nalu_parser::NalParser;
    use super::nalu_splitter::NALUSplitter;

    pub use super::au_splitter::AccessUnit;
    pub use super::nalu_parser::{Nalu, ParsedNalu};
    #[cfg(feature = "expose-parsers")]
    pub use h264_reader::nal as nal_types;

    #[derive(Debug, thiserror::Error)]
    pub enum H264ParserError {
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
    #[derive(Default)]
    pub struct H264Parser {
        nal_parser: NalParser,
        nalu_splitter: NALUSplitter,
        convert_to_avcc: bool,
        au_splitter: AUSplitter,
    }

    impl H264Parser {
        pub fn new_avcc_output() -> Self {
            Self {
                convert_to_avcc: true,
                ..Default::default()
            }
        }

        /// Parses nalus in Annex B format.
        /// Returns [`AccessUnit`]s representing whole frame
        pub fn parse(
            &mut self,
            bytes: &[u8],
            pts: Option<u64>,
        ) -> Result<Vec<AccessUnit>, H264ParserError> {
            let nalus = self.nalu_splitter.push(bytes, pts);
            let nalus = nalus.into_iter().map(|(nalu_bytes, pts)| {
                self.nal_parser.parse_nalu(&nalu_bytes).map(|parsed_nalu| {
                    parsed_nalu_to_struct(parsed_nalu, nalu_bytes, self.convert_to_avcc, pts)
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

        pub fn flush(&mut self) -> Result<Vec<AccessUnit>, H264ParserError> {
            let nalus = self.nalu_splitter.flush();
            let nalus = nalus.into_iter().map(|(nalu_bytes, pts)| {
                self.nal_parser.parse_nalu(&nalu_bytes).map(|parsed_nalu| {
                    parsed_nalu_to_struct(parsed_nalu, nalu_bytes, self.convert_to_avcc, pts)
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

            if let Some(au) = self.au_splitter.flush() {
                access_units.push(au);
            }

            Ok(access_units)
        }
    }

    fn to_avcc(nalu: &mut [u8]) {
        let len = nalu.len();
        nalu[..4].copy_from_slice(&(len as u32 - 4).to_be_bytes());
    }

    fn parsed_nalu_to_struct(
        mut parsed_nalu: ParsedNalu,
        mut nalu_bytes: Vec<u8>,
        convert_to_avcc: bool,
        pts: Option<u64>,
    ) -> Nalu {
        if convert_to_avcc {
            to_avcc(&mut nalu_bytes);
            if let ParsedNalu::Slice(ref mut slice) = parsed_nalu {
                to_avcc(&mut slice.rbsp_bytes);
            }
        }

        Nalu {
            parsed: parsed_nalu,
            raw_bytes: nalu_bytes.into_boxed_slice(),
            pts,
        }
    }
}
