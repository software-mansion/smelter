use fdk_aac_sys as fdk;
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    audio_mixer::AudioSamples,
    error::DecoderInitError,
    pipeline::{
        decoder::{AudioDecoder, DecodingError},
        types::{EncodedChunk, EncodedChunkKind},
        AudioCodec, PipelineCtx,
    },
};

use super::DecodedSamples;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub asc: Option<bytes::Bytes>,
}

pub(crate) struct FdkAacDecoder {
    decoder: Option<Decoder>,
    asc: Option<bytes::Bytes>,
}

impl AudioDecoder for FdkAacDecoder {
    const LABEL: &'static str = "FDK AAC decoder";

    type Options = Options;

    fn new(_ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError> {
        info!("Initializing FDK AAC decoder");
        Ok(Self {
            decoder: None,
            asc: options.asc,
        })
    }

    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, DecodingError> {
        match &mut self.decoder {
            Some(decoder) => Ok(decoder.decode(chunk)?),
            None => {
                let mut decoder = Decoder::new(&self.asc, &chunk)?;
                let result = decoder.decode(chunk)?;
                self.decoder = Some(decoder);
                Ok(result)
            }
        }
    }

    fn flush(&mut self) -> Vec<DecodedSamples> {
        Vec::new()
    }
}

struct Decoder {
    instance: *mut fdk::AAC_DECODER_INSTANCE,
    decoded_samples_buffer: Vec<fdk::INT_PCM>,
}

impl Decoder {
    fn new(
        asc: &Option<bytes::Bytes>,
        first_chunk: &EncodedChunk,
    ) -> Result<Self, FdkAacDecoderError> {
        let transport = if first_chunk.data[..4] == [b'A', b'D', b'I', b'F'] {
            fdk::TRANSPORT_TYPE_TT_MP4_ADIF
        } else if first_chunk.data[0] == 0xff && first_chunk.data[1] & 0xf0 == 0xf0 {
            fdk::TRANSPORT_TYPE_TT_MP4_ADTS
        } else {
            fdk::TRANSPORT_TYPE_TT_MP4_RAW
        };

        let instance = unsafe { fdk::aacDecoder_Open(transport, 1) };

        if let Some(config) = asc {
            let result = unsafe {
                fdk::aacDecoder_ConfigRaw(
                    instance,
                    &mut config.to_vec().as_mut_ptr(),
                    &(config.len() as u32),
                )
            };

            if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                return Err(FdkAacDecoderError::FdkDecoderError(result));
            }
        }

        Ok(Self {
            instance,
            decoded_samples_buffer: vec![0; 100_000],
        })
    }

    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, FdkAacDecoderError> {
        if chunk.kind != EncodedChunkKind::Audio(AudioCodec::Aac) {
            return Err(FdkAacDecoderError::UnsupportedChunkKind(chunk.kind));
        }

        let buffer_size = chunk.data.len() as u32;
        // bytes read from buffer
        let mut bytes_valid = buffer_size;
        let mut buffer = chunk.data.to_vec();

        let mut decoded_samples = Vec::new();

        while bytes_valid > 0 {
            // This fills the decoder with data.
            // It will adjust `bytes_valid` on its own based on how many bytes are left in the
            // buffer.
            let result = unsafe {
                fdk::aacDecoder_Fill(
                    self.instance,
                    &mut buffer.as_mut_ptr(),
                    &buffer_size,
                    &mut bytes_valid,
                )
            };

            if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                return Err(FdkAacDecoderError::FdkDecoderError(result));
            }

            loop {
                let result = unsafe {
                    fdk::aacDecoder_DecodeFrame(
                        self.instance,
                        self.decoded_samples_buffer.as_mut_ptr(),
                        self.decoded_samples_buffer.len() as i32,
                        0,
                    )
                };

                if result == fdk::AAC_DECODER_ERROR_AAC_DEC_NOT_ENOUGH_BITS {
                    // Need to put more data in
                    break;
                }

                if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                    return Err(FdkAacDecoderError::FdkDecoderError(result));
                }

                let info = unsafe { *fdk::aacDecoder_GetStreamInfo(self.instance) };
                let raw_frame_size = (info.aacSamplesPerFrame * info.channelConfig) as usize;

                let samples = match info.channelConfig {
                    1 => AudioSamples::Mono(
                        self.decoded_samples_buffer[..raw_frame_size]
                            .iter()
                            .map(|value| *value as f64 / i16::MAX as f64)
                            .collect(),
                    ),
                    2 => AudioSamples::Stereo(
                        self.decoded_samples_buffer[..raw_frame_size]
                            .chunks_exact(2)
                            .map(|c| (c[0] as f64 / i16::MAX as f64, c[1] as f64 / i16::MAX as f64))
                            .collect(),
                    ),
                    _ => return Err(FdkAacDecoderError::UnsupportedChannelConfig),
                };

                let sample_rate = if info.sampleRate > 0 {
                    info.sampleRate as u32
                } else {
                    error!(
                        "Unexpected sample rate of decoded AAC audio: {}",
                        info.sampleRate
                    );
                    0
                };

                decoded_samples.push(DecodedSamples {
                    samples,
                    start_pts: chunk.pts,
                    sample_rate,
                })
            }
        }
        Ok(decoded_samples)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            fdk::aacDecoder_Close(self.instance);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FdkAacDecoderError {
    #[error("The internal fdk decoder returned an error: {0:?}.")]
    FdkDecoderError(fdk::AAC_DECODER_ERROR),

    #[error("The channel config in the aac audio is unsupported.")]
    UnsupportedChannelConfig,

    #[error("The aac decoder cannot decode chunks with kind {0:?}.")]
    UnsupportedChunkKind(EncodedChunkKind),

    #[error("The aac decoder cannot decode chunks with sample rate {0}.")]
    UnsupportedSampleRate(i32),
}
