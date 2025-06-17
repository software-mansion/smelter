use fdk_aac_sys as fdk;
use std::sync::Arc;
use tracing::error;

use crate::{
    error::InputInitError,
    pipeline::{
        decoder::AacDecoderOptions,
        types::{EncodedChunk, EncodedChunkKind, Samples},
        AudioCodec,
    },
};

use super::{AudioDecoderExt, DecodedSamples, DecodingError};

#[derive(Debug, thiserror::Error)]
pub enum AacDecoderError {
    #[error("The internal fdk decoder returned an error: {0:?}.")]
    FdkDecoderError(fdk::AAC_DECODER_ERROR),

    #[error("The channel config in the aac audio is unsupported.")]
    UnsupportedChannelConfig,

    #[error("The aac decoder cannot decode chunks with kind {0:?}.")]
    UnsupportedChunkKind(EncodedChunkKind),

    #[error("The aac decoder cannot decode chunks with sample rate {0}.")]
    UnsupportedSampleRate(i32),
}

pub(super) struct AacDecoder {
    decoder: Decoder,
    #[allow(dead_code)]
    sample_rate: u32,
}

impl AacDecoder {
    /// The encoded chunk used for initialization here still needs to be fed into `Decoder::decode_chunk` later
    pub fn new(
        options: AacDecoderOptions,
        first_chunk: EncodedChunk,
    ) -> Result<Self, InputInitError> {
        let transport = if first_chunk.data[..4] == [b'A', b'D', b'I', b'F'] {
            fdk::TRANSPORT_TYPE_TT_MP4_ADIF
        } else if first_chunk.data[0] == 0xff && first_chunk.data[1] & 0xf0 == 0xf0 {
            fdk::TRANSPORT_TYPE_TT_MP4_ADTS
        } else {
            fdk::TRANSPORT_TYPE_TT_MP4_RAW
        };

        let mut decoder = Decoder::new(options, transport)?;
        decoder.decode(first_chunk)?;

        let info = unsafe { *fdk::aacDecoder_GetStreamInfo(decoder.instance) };

        let sample_rate = match info.aacSampleRate > 0 {
            true => info.aacSampleRate as u32,
            false => {
                return Err(AacDecoderError::UnsupportedSampleRate(info.aacSampleRate).into());
            }
        };
        if info.channelConfig != 1 && info.channelConfig != 2 {
            return Err(AacDecoderError::UnsupportedChannelConfig.into());
        }

        Ok(AacDecoder {
            decoder,
            sample_rate,
        })
    }
}

impl AudioDecoderExt for AacDecoder {
    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, DecodingError> {
        self.decoder.decode(chunk)?;
        Ok(self.decoder.decoded_samples())
    }
}

struct Decoder {
    instance: *mut fdk::AAC_DECODER_INSTANCE,
    decoded_samples_buffer: Vec<fdk::INT_PCM>,
    decoded_samples: Vec<DecodedSamples>,
}

impl Decoder {
    fn new(options: AacDecoderOptions, transport: i32) -> Result<Self, AacDecoderError> {
        let instance = unsafe { fdk::aacDecoder_Open(transport, 1) };

        if let Some(config) = options.asc {
            let result = unsafe {
                fdk::aacDecoder_ConfigRaw(
                    instance,
                    &mut config.to_vec().as_mut_ptr(),
                    &(config.len() as u32),
                )
            };

            if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                return Err(AacDecoderError::FdkDecoderError(result).into());
            }
        }

        Ok(Self {
            instance,
            decoded_samples_buffer: vec![0; 100_000],
            decoded_samples: Vec::new(),
        })
    }

    fn decode(&mut self, chunk: EncodedChunk) -> Result<(), AacDecoderError> {
        if chunk.kind != EncodedChunkKind::Audio(AudioCodec::Aac) {
            return Err(AacDecoderError::UnsupportedChunkKind(chunk.kind));
        }

        let buffer_size = chunk.data.len() as u32;
        // bytes read from buffer
        let mut bytes_valid = buffer_size;
        let mut buffer = chunk.data.to_vec();

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
                return Err(AacDecoderError::FdkDecoderError(result).into());
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
                    return Err(AacDecoderError::FdkDecoderError(result).into());
                }

                let info = unsafe { *fdk::aacDecoder_GetStreamInfo(self.instance) };
                let raw_frame_size = (info.aacSamplesPerFrame * info.channelConfig) as usize;

                let samples = match info.channelConfig {
                    1 => Arc::new(Samples::Mono16Bit(
                        self.decoded_samples_buffer[..raw_frame_size].to_vec(),
                    )),
                    2 => Arc::new(Samples::Stereo16Bit(
                        self.decoded_samples_buffer[..raw_frame_size]
                            .chunks_exact(2)
                            .map(|c| (c[0], c[1]))
                            .collect(),
                    )),
                    _ => return Err(AacDecoderError::UnsupportedChannelConfig.into()),
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

                self.decoded_samples.push(DecodedSamples {
                    samples,
                    start_pts: chunk.pts,
                    sample_rate,
                })
            }
        }
        Ok(())
    }

    fn decoded_samples(&mut self) -> Vec<DecodedSamples> {
        std::mem::take(&mut self.decoded_samples)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            fdk::aacDecoder_Close(self.instance);
        }
    }
}
