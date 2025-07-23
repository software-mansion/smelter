use std::{
    mem::{self, MaybeUninit},
    os::raw::{c_int, c_void},
    ptr,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use fdk_aac_sys as fdk;
use tracing::{error, info};

use crate::{
    audio_mixer::{AudioChannels, AudioSamples, OutputSamples},
    error::EncoderInitError,
    pipeline::{types::IsKeyframe, AudioCodec, EncodedChunk, EncodedChunkKind, PipelineCtx},
};

use super::{AudioEncoder, AudioEncoderConfig, AudioEncoderOptionsExt};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AacEncoderOptions {
    pub channels: AudioChannels,
    pub sample_rate: u32,
}

impl AudioEncoderOptionsExt for AacEncoderOptions {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// FDK-AAC encoder.
/// Implementation is based on the fdk-aac encoder documentation:
/// https://github.com/mstorsjo/fdk-aac/blob/master/documentation/aacEncoder.pdf
pub struct FdkAacEncoder {
    encoder: *mut fdk::AACENCODER,
    input_buffer: Vec<i16>,
    output_buffer: Vec<u8>,
    sample_rate: u32,
    start_pts: Option<Duration>,
    sent_samples: u128,
    samples_per_frame: u32,
}

impl AudioEncoder for FdkAacEncoder {
    const LABEL: &'static str = "FDK AAC encoder";

    type Options = AacEncoderOptions;

    fn new(
        _ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError> {
        info!(?options, "Initializing FDK AAC encoder");
        // Section 2.3 of the fdk-aac Encoder documentation - encoder initialization.
        let mut encoder = ptr::null_mut();
        // For mono and stereo audio, those values are the same, but it's not the case for other channel modes.
        // Leaving as it is, to avoid potential issues when other channel options will be added.
        let (channels, channel_mode) = match options.channels {
            AudioChannels::Mono => (1, fdk::CHANNEL_MODE_MODE_1 as u32),
            AudioChannels::Stereo => (2, fdk::CHANNEL_MODE_MODE_2 as u32),
        };
        let mut maybe_info = MaybeUninit::uninit();
        let info;

        unsafe {
            check(fdk::aacEncOpen(&mut encoder as *mut _, 0, channels))?;

            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_AOT,
                fdk::AUDIO_OBJECT_TYPE_AOT_AAC_LC as u32,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_BITRATEMODE,
                5,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_SAMPLERATE,
                options.sample_rate,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_TRANSMUX,
                0,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_SBR_MODE,
                0,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_CHANNELMODE,
                channel_mode,
            ))?;
            check(fdk::aacEncoder_SetParam(
                encoder,
                fdk::AACENC_PARAM_AACENC_AFTERBURNER,
                1,
            ))?;

            // Section 2.2.3 of the fdk-aac Encoder documentation:
            // "Call aacEncEncode() with NULL parameters to initialize encoder instance with present parameter set."
            check(fdk::aacEncEncode(
                encoder,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
            ))?;

            check(fdk::aacEncInfo(encoder, maybe_info.as_mut_ptr()))?;
            info = maybe_info.assume_init();
        }

        Ok((
            Self {
                encoder,
                input_buffer: Vec::new(),
                output_buffer: vec![0; info.maxOutBufBytes as usize],
                sample_rate: options.sample_rate,
                start_pts: None,
                sent_samples: 0,
                samples_per_frame: info.frameLength,
            },
            AudioEncoderConfig {
                //channels: options.channels,
                //sample_rate: options.sample_rate,
                extradata: Some(Bytes::copy_from_slice(
                    &info.confBuf[0..(info.confSize as usize)],
                )),
            },
        ))
    }

    fn encode(&mut self, output_samples: OutputSamples) -> Vec<EncodedChunk> {
        self.enqueue_samples(output_samples);
        self.call_fdk_encode(false).unwrap_or_else(|err| {
            error!("Encoding error: {:?}", err);
            vec![]
        })
    }

    fn flush(&mut self) -> Vec<EncodedChunk> {
        self.call_fdk_encode(true).unwrap_or_else(|err| {
            error!("Encoding error: {:?}", err);
            vec![]
        })
    }

    // For AAC it does completely nothing, however it is needed for trait
    // implementation
    fn set_packet_loss(&mut self, _packet_loss: i32) {}
}

impl FdkAacEncoder {
    fn call_fdk_encode(&mut self, flush: bool) -> Result<Vec<EncodedChunk>, EncoderInitError> {
        let mut output = vec![];

        loop {
            // According to aacEncEncode docs, numInSamples should be set to -1 to flush the encoder.
            let num_in_samples = match flush {
                true => -1,
                false => self.input_buffer.len() as c_int,
            };

            let in_args = fdk::AACENC_InArgs {
                numInSamples: num_in_samples,
                numAncBytes: 0,
            };

            // FDK docs 2.5 states that "the input buffer should be handled as a modulo buffer". I couldn't find what "modulo buffer" means.
            // Code examples in docs use static array and move samples after each `encode` call (it's not a circular buffer).
            // Circular/ring buffer (sth like VecDeque) can't be used, since it doesn't guarantee contiguous memory layout.
            // So, Vec is used here instead. This approach does additional heap allocation after every `fdk::aacEncEncode` call (on drain),
            // but is safer / more readable (no buffer overflows etc.).
            //
            // Calling `drain` on the input buffer will reallocate it, so the buffer descriptions are created right before calling `aacEncEncode`.
            // It's unsafe to use pointers obtained by calling `as_ptr()` and `as_ptr_mut()` after moving / reallocating the buffer.
            let mut in_buf = self.input_buffer.as_ptr();
            let mut in_buf_ident: c_int = fdk::AACENC_BufferIdentifier_IN_AUDIO_DATA as c_int;
            let mut in_buf_size: c_int = self.input_buffer.len() as c_int;
            let mut in_buf_el_size: c_int = mem::size_of::<i16>() as c_int;

            let in_desc = fdk::AACENC_BufDesc {
                numBufs: 1,
                bufs: &mut in_buf as *mut _ as *mut *mut c_void,
                bufferIdentifiers: &mut in_buf_ident as *mut _,
                bufSizes: &mut in_buf_size as *mut _,
                bufElSizes: &mut in_buf_el_size as *mut _,
            };

            let mut out_buf = self.output_buffer.as_mut_ptr();
            let mut out_buf_ident: c_int = fdk::AACENC_BufferIdentifier_OUT_BITSTREAM_DATA as c_int;
            let mut out_buf_size: c_int = self.output_buffer.len() as c_int;
            let mut out_buf_el_size: c_int = mem::size_of::<i16>() as c_int;

            let out_desc = fdk::AACENC_BufDesc {
                numBufs: 1,
                bufs: &mut out_buf as *mut _ as *mut *mut c_void,
                bufferIdentifiers: &mut out_buf_ident as *mut _,
                bufSizes: &mut out_buf_size as *mut _,
                bufElSizes: &mut out_buf_el_size as *mut _,
            };

            let mut out_args;
            let res = unsafe {
                out_args = mem::zeroed();

                check(fdk::aacEncEncode(
                    self.encoder,
                    &in_desc,
                    &out_desc,
                    &in_args,
                    &mut out_args,
                ))
            };

            // Breaking here no matter what error was return seems wrong,
            // but calling convention in documentation specifies that it should be done this way.
            if res.is_err() {
                break;
            }

            self.input_buffer.drain(..(out_args.numInSamples as usize));

            let encoded_bytes = out_args.numOutBytes as usize;
            if encoded_bytes > 0 {
                let pts = self.start_pts.unwrap()
                    + Duration::from_secs_f64(self.sent_samples as f64 / self.sample_rate as f64);

                // assume that encoder is always producing batches representing full frame
                self.sent_samples += self.samples_per_frame as u128;

                output.push(EncodedChunk {
                    data: Bytes::copy_from_slice(
                        &self.output_buffer[..out_args.numOutBytes as usize],
                    ),
                    pts,
                    dts: None,
                    is_keyframe: IsKeyframe::NoKeyframes,
                    kind: EncodedChunkKind::Audio(AudioCodec::Aac),
                });
            } else {
                break;
            }
        }
        Ok(output)
    }

    fn enqueue_samples(&mut self, samples: OutputSamples) {
        if self.start_pts.is_none() {
            self.start_pts = Some(samples.start_pts);
        };

        match samples.samples {
            AudioSamples::Mono(mono_samples) => {
                self.input_buffer.extend(
                    mono_samples
                        .iter()
                        .map(|val| (*val * i16::MAX as f64) as i16),
                );
            }
            AudioSamples::Stereo(stereo_samples) => {
                for (l, r) in stereo_samples {
                    self.input_buffer.push((l * i16::MAX as f64) as i16);
                    self.input_buffer.push((r * i16::MAX as f64) as i16);
                }
            }
        }
    }
}

impl Drop for FdkAacEncoder {
    fn drop(&mut self) {
        unsafe {
            fdk::aacEncClose(&mut self.encoder as *mut _);
        }
    }
}

fn check(result: fdk::AACENC_ERROR) -> Result<(), EncoderInitError> {
    if result == fdk::AACENC_ERROR_AACENC_OK {
        Ok(())
    } else {
        Err(EncoderInitError::AacError(result))
    }
}
