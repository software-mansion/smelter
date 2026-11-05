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

    /// Format of the H.264 bytestream fed to the parser
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum InputBytestreamFormatH264 {
        /// NAL units are separated by start codes (`0x000001` or `0x00000001`)
        #[default]
        AnnexB,

        /// Each NAL unit is prefixed with its length as a 1, 2 or 4-byte big-endian integer.
        ///
        /// The prefix length is stored in the `lengthSizeMinusOne` field of the `AVCDecoderConfigurationRecord`
        /// (the `avcC` box in MP4, `CodecPrivate` in Matroska, the AVC sequence header in RTMP).
        /// In practice the prefix is almost always 4 bytes long.
        Avcc { prefix_length: AvccPrefixLength },
    }

    /// Format of the H.264 bytestream produced by the parser
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OutputBytestreamFormatH264 {
        /// Each NAL unit is prefixed with a `0x00000001` start code
        #[default]
        AnnexB,

        /// Each NAL unit is prefixed with its length as a 4-byte big-endian integer.
        Avcc,
    }

    /// Byte length of the NAL unit length prefix in AVCC streams
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AvccPrefixLength {
        Bytes1,
        Bytes2,
        #[default]
        Bytes4,
    }

    impl AvccPrefixLength {
        pub(crate) fn prefix_byte_length(self) -> usize {
            match self {
                Self::Bytes1 => 1,
                Self::Bytes2 => 2,
                Self::Bytes4 => 4,
            }
        }
    }

    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ParserOptionsH264 {
        pub input_format: InputBytestreamFormatH264,
        pub output_format: OutputBytestreamFormatH264,
    }

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

    #[derive(Default)]
    pub struct H264Parser {
        nal_parser: NalParser,
        nalu_splitter: NALUSplitter,
        output_format: OutputBytestreamFormatH264,
        au_splitter: AUSplitter,
    }

    impl H264Parser {
        pub fn new(options: ParserOptionsH264) -> Self {
            Self {
                nalu_splitter: NALUSplitter::new(options.input_format),
                output_format: options.output_format,
                ..Default::default()
            }
        }

        /// Parses nalus in the format selected with [`ParserOptionsH264::input_format`].
        /// Returns [`AccessUnit`]s representing whole frame
        pub fn parse(
            &mut self,
            bytes: &[u8],
            pts: Option<u64>,
        ) -> Result<Vec<AccessUnit>, H264ParserError> {
            let nalus = self.nalu_splitter.push(bytes, pts);
            let nalus = nalus.into_iter().map(|(nalu_bytes, pts)| {
                self.nal_parser.parse_nalu(&nalu_bytes).map(|parsed_nalu| {
                    parsed_nalu_to_struct(parsed_nalu, nalu_bytes, self.output_format, pts)
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
                    parsed_nalu_to_struct(parsed_nalu, nalu_bytes, self.output_format, pts)
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
        output_format: OutputBytestreamFormatH264,
        pts: Option<u64>,
    ) -> Nalu {
        if output_format == OutputBytestreamFormatH264::Avcc {
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
