use std::{num::NonZeroU32, time::Duration};

use crate::{
    EncodedInputChunk, EncodedOutputChunk, VideoBackendError, VideoDecoderError, VideoEncoderError,
    device::{EncoderOutputParameters, Rational},
    parameters::{H264Profile, H265Profile, ScalingAlgorithm},
};

pub struct VideoTranscoder {
    pub(crate) transcoder: Box<dyn VideoTranscoderBackend>,
}

impl VideoTranscoder {
    pub fn transcode(&mut self, input: EncodedInputChunk<'_>) -> Result<(), VideoTranscoderError> {
        self.transcode_timeout(input, Duration::MAX)
    }

    pub fn transcode_timeout(
        &mut self,
        input: EncodedInputChunk<'_>,
        timeout: Duration,
    ) -> Result<(), VideoTranscoderError> {
        self.transcoder.transcode(input, timeout)
    }

    pub fn flush(&mut self) -> Result<(), VideoTranscoderError> {
        self.flush_timeout(Duration::MAX)
    }

    pub fn flush_timeout(&mut self, timeout: Duration) -> Result<(), VideoTranscoderError> {
        self.transcoder.flush(timeout)
    }
}

pub struct TranscodedChunk {
    /// Index into [`TranscoderParameters::output_parameters`].
    pub output_index: usize,
    pub chunk: EncodedOutputChunk<Vec<u8>>,
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
    /// Maximum number of decode submissions and, for each output, encode submissions that can be
    /// in flight. When a limit is reached, transcoding blocks until the oldest submission finishes.
    /// If set to 0, the transcoder will work synchronously.
    ///
    /// This overrides `max_in_flight_submissions` in each output's encoder parameters.
    ///
    /// **Defaults to 3**
    pub max_in_flight_submissions: Option<u32>,
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

    #[error("Transcoder error: {0}")]
    BackendError(VideoBackendError),
}

pub(crate) trait VideoTranscoderBackend: Send {
    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
        timeout: Duration,
    ) -> Result<(), VideoTranscoderError>;

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoTranscoderError>;
}
