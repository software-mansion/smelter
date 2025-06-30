use fdk_aac_sys as fdk;
use std::sync::Arc;
use tracing::error;

use crate::{
    error::InputInitError,
    pipeline::{
        decoder::AacDecoderOptions,
        types::{EncodedChunk, EncodedChunkKind, Samples}, AudioCodec,
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
    instance: *mut fdk::AAC_DECODER_INSTANCE,
    sample_rate: u32,
}

impl AacDecoder {
    /// The encoded chunk used for initialization here still needs to be fed into `Decoder::decode_chunk` later
    pub fn new(
        options: AacDecoderOptions,
        first_chunk: &EncodedChunk,
    ) -> Result<Self, InputInitError> {
        let transport = if first_chunk.data[..4] == [b'A', b'D', b'I', b'F'] {
            fdk::TRANSPORT_TYPE_TT_MP4_ADIF
        } else if first_chunk.data[0] == 0xff && first_chunk.data[1] & 0xf0 == 0xf0 {
            fdk::TRANSPORT_TYPE_TT_MP4_ADTS
        } else {
            fdk::TRANSPORT_TYPE_TT_MP4_RAW
        };

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

        let info = unsafe { *fdk::aacDecoder_GetStreamInfo(instance) };
        let aac_sample_rate = info.aacSampleRate;
        let sample_rate = if aac_sample_rate > 0 {
            aac_sample_rate as u32
        } else {
            return Err(AacDecoderError::UnsupportedSampleRate(aac_sample_rate).into());
        };
        if info.channelConfig != 1 && info.channelConfig != 2 {
            return Err(AacDecoderError::UnsupportedChannelConfig.into());
        }

        Ok(AacDecoder {
            instance,
            sample_rate,
        })
    }
}

impl Drop for AacDecoder {
    fn drop(&mut self) {
        unsafe {
            fdk::aacDecoder_Close(self.instance);
        }
    }
}

impl AudioDecoderExt for AacDecoder {
    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, DecodingError> {
        if chunk.kind != EncodedChunkKind::Audio(crate::pipeline::AudioCodec::Aac) {
            return Err(AacDecoderError::UnsupportedChunkKind(chunk.kind).into());
        }

        let buffer_size = chunk.data.len() as u32;
        let mut bytes_valid = buffer_size;
        let mut buffer = chunk.data.to_vec();
        let mut output_buffer = Vec::new();

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

            let info = unsafe { *fdk::aacDecoder_GetStreamInfo(self.instance) };

            // The decoder should output `info.aacSamplesPerFrame` for each channel
            let mut decoded_samples: Vec<fdk::INT_PCM> =
                vec![0; (info.aacSamplesPerFrame * info.channelConfig) as usize];

            let result = unsafe {
                fdk::aacDecoder_DecodeFrame(
                    self.instance,
                    decoded_samples.as_mut_ptr(),
                    decoded_samples.len() as i32,
                    0,
                )
            };

            if result == fdk::AAC_DECODER_ERROR_AAC_DEC_NOT_ENOUGH_BITS {
                // Need to put more data in
                continue;
            }

            if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                return Err(AacDecoderError::FdkDecoderError(result).into());
            }

            let samples = match info.channelConfig {
                1 => Arc::new(Samples::Mono16Bit(decoded_samples)),
                2 => Arc::new(Samples::Stereo16Bit(
                    decoded_samples
                        .chunks_exact(2)
                        .map(|c| (c[0], c[1]))
                        .collect(),
                )),
                _ => return Err(AacDecoderError::UnsupportedChannelConfig.into()),
            };

            // Sample rate can change after decoding
            let info = unsafe { *fdk::aacDecoder_GetStreamInfo(self.instance) };
            let sample_rate = if info.sampleRate > 0 {
                info.sampleRate as u32
            } else {
                error!(
                    "Unexpected sample rate of decoded AAC audio: {}",
                    info.sampleRate
                );
                0
            };

            output_buffer.push(DecodedSamples {
                samples,
                start_pts: chunk.pts,
                sample_rate,
            })
        }

        Ok(output_buffer)
    }

    fn decoded_sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

struct Decoder {
    instance: *mut fdk::AAC_DECODER_INSTANCE,
    decoded_samples_buffer: Vec<fdk::INT_PCM>,
    decoded_samples: Vec<DecodedSamples>,
}

impl Decoder {
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
