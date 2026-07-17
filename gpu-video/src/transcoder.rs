use std::num::NonZeroU32;

use crate::{
    EncodedInputChunk, EncodedOutputChunk, VideoBackendError, VideoDecoderError, VideoEncoderError,
    device::{EncoderOutputParameters, Rational},
    parameters::{H264Profile, H265Profile, ScalingAlgorithm},
};

pub struct VideoTranscoder {
    pub(crate) transcoder: Box<dyn VideoTranscoderBackend>,
}

impl VideoTranscoder {
    pub fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError> {
        self.transcoder.transcode(input)
    }

    pub fn flush(&mut self) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError> {
        self.transcoder.flush()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnyEncoderParameters {
    H264(EncoderOutputParameters<H264Profile>),
    H265(EncoderOutputParameters<H265Profile>),
}

/// Configuration for a transcoder
#[derive(Debug, Clone)]
pub struct TranscoderParameters {
    pub input_framerate: Rational,
    pub output_parameters: Vec<TranscoderOutputParameters>,
}

/// Configuration for a single transcoder output.
#[derive(Debug, Clone, Copy)]
pub struct TranscoderOutputParameters {
    pub encoder_parameters: AnyEncoderParameters,
    pub output_width: NonZeroU32,
    pub output_height: NonZeroU32,
    pub scaling_algorithm: ScalingAlgorithm,
}

#[derive(Debug, thiserror::Error)]
pub enum VideoTranscoderError {
    #[error(transparent)]
    Decoder(#[from] VideoDecoderError),

    #[error(transparent)]
    Encoder(#[from] VideoEncoderError),

    #[error("Wrong output number: expected a value between 0 and {expected_max}, found {actual}")]
    WrongOutputNumber { expected_max: usize, actual: usize },

    #[error("Transcoding is not supported on this backend")]
    TranscoderUnsupported,

    #[error("Transcoder error: {0}")]
    BackendError(VideoBackendError),
}

pub(crate) trait VideoTranscoderBackend: Send {
    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError>;

    fn flush(&mut self) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, VideoTranscoderError>;
}
