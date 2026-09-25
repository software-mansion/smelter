use fdk_aac_sys as fdk;
use std::{sync::Arc, time::Duration};
use tracing::{info, trace, warn};

use crate::pipeline::decoder::{AudioDecoder, EncodedInputEvent};

use crate::prelude::*;

/// After this many frames concealment has faded out to silence, restarting the decoder gives the
/// same result and brings audio back without a fade-in.
const MAX_CONCEALED_FRAMES: u32 = 6;

pub(crate) struct FdkAacDecoder {
    /// Dropped when an fdk call fails, the only case that can leave its stream info zeroed.
    decoder: Option<Decoder>,
    asc: Option<bytes::Bytes>,
}

impl AudioDecoder for FdkAacDecoder {
    const LABEL: &'static str = "FDK AAC decoder";

    type Options = FdkAacDecoderOptions;

    fn new(_ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError> {
        info!("Initializing FDK AAC decoder");
        Ok(Self {
            decoder: None,
            asc: options.asc,
        })
    }

    fn decode(
        &mut self,
        event: EncodedInputEvent,
    ) -> Result<Vec<InputAudioSamples>, DecodingError> {
        trace!(?event, "FDK AAC decoder received an event.");
        let chunk = match event {
            EncodedInputEvent::Chunk(chunk) => chunk,
            // Gaps are detected from chunk timestamps instead.
            EncodedInputEvent::LostData | EncodedInputEvent::AuDelimiter => return Ok(vec![]),
            EncodedInputEvent::Discontinuity => return Ok(self.flush()),
        };
        if chunk.kind != MediaKind::Audio(AudioCodec::Aac) {
            return Err(FdkAacDecoderError::UnsupportedChunkKind(chunk.kind).into());
        }

        let mut samples = self.fill_gap(chunk.pts);
        samples.extend(self.decode_chunk(&chunk)?);
        Ok(samples)
    }

    fn flush(&mut self) -> Vec<InputAudioSamples> {
        self.decoder.take().map(Decoder::flush).unwrap_or_default()
    }
}

impl FdkAacDecoder {
    /// Conceals a short gap before `pts`. A long one ends the stream, so that a new decoder
    /// starts from `pts`.
    fn fill_gap(&mut self, pts: Timestamp) -> Vec<InputAudioSamples> {
        let Some(decoder) = &mut self.decoder else {
            return Vec::new();
        };
        let lost_frames = decoder.lost_frames(pts);
        if lost_frames > MAX_CONCEALED_FRAMES {
            return self.flush();
        }
        match decoder.conceal(lost_frames) {
            Ok(concealed) => concealed,
            // Only synthetic audio is lost, the chunk goes to a new decoder.
            Err(err) => {
                warn!("FDK AAC decoder failed to conceal, restarting: {err}");
                self.decoder = None;
                Vec::new()
            }
        }
    }

    fn decode_chunk(
        &mut self,
        chunk: &EncodedInputChunk,
    ) -> Result<Vec<InputAudioSamples>, FdkAacDecoderError> {
        match &mut self.decoder {
            Some(decoder) => match decoder.decode(chunk) {
                Ok(decoded) => Ok(decoded),
                // fdk tore its state down, so nothing is drained or read from the old decoder. A
                // new one gets the same chunk, for ADTS it carries the configuration that failed.
                Err(err) => {
                    warn!("FDK AAC decoder failed, restarting: {err}");
                    self.decoder = None;
                    let mut decoder = Decoder::new(&self.asc, chunk)?;
                    let decoded = decoder.decode(chunk)?;
                    self.decoder = Some(decoder);
                    Ok(decoded)
                }
            },
            None => {
                let mut decoder = Decoder::new(&self.asc, chunk)?;
                let decoded = decoder.decode(chunk)?;
                self.decoder = Some(decoder);
                Ok(decoded)
            }
        }
    }
}

enum FrameStatus {
    Frame(AudioSamples),
    NotEnoughBits,
    /// Nothing usable came out, but the decoder can continue.
    Skipped,
}

struct Decoder {
    instance: *mut fdk::AAC_DECODER_INSTANCE,
    decoded_samples_buffer: Vec<fdk::INT_PCM>,
    /// Where concealed and flushed frames continue.
    last_frame_end_pts: Option<Timestamp>,
    /// Output still to drop from the start. A fresh decoder first outputs its empty delay line,
    /// so this starts at the delay reported with the first frame.
    skip_samples: Option<usize>,
}

impl Decoder {
    fn new(
        asc: &Option<bytes::Bytes>,
        first_chunk: &EncodedInputChunk,
    ) -> Result<Self, FdkAacDecoderError> {
        // Every demuxer that provides an ASC delivers bare access units. Without one the
        // frames have to describe themselves (HLS over MPEG-TS delivers ADTS), so sniff.
        let transport = match (asc, &first_chunk.data[..]) {
            (Some(_), _) => fdk::TRANSPORT_TYPE_TT_MP4_RAW,
            (None, [b'A', b'D', b'I', b'F', ..]) => fdk::TRANSPORT_TYPE_TT_MP4_ADIF,
            (None, [0xff, second, ..]) if second & 0xf0 == 0xf0 => fdk::TRANSPORT_TYPE_TT_MP4_ADTS,
            (None, _) => fdk::TRANSPORT_TYPE_TT_MP4_RAW,
        };

        // Constructed before configuration, so an early return closes the instance.
        let decoder = Self {
            instance: unsafe { fdk::aacDecoder_Open(transport, 1) },
            decoded_samples_buffer: vec![0; 100_000],
            last_frame_end_pts: None,
            skip_samples: None,
        };

        // Only mono and stereo are supported, fdk downmixes anything with more channels.
        let result = unsafe {
            fdk::aacDecoder_SetParam(
                decoder.instance,
                fdk::AACDEC_PARAM_AAC_PCM_MAX_OUTPUT_CHANNELS,
                2,
            )
        };
        if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
            return Err(FdkAacDecoderError::FdkDecoderError(result));
        }

        if let Some(config) = asc {
            let result = unsafe {
                fdk::aacDecoder_ConfigRaw(
                    decoder.instance,
                    &mut config.to_vec().as_mut_ptr(),
                    &(config.len() as u32),
                )
            };

            if result != fdk::AAC_DECODER_ERROR_AAC_DEC_OK {
                return Err(FdkAacDecoderError::FdkDecoderError(result));
            }
        }

        Ok(decoder)
    }

    fn stream_info(&self) -> fdk::CStreamInfo {
        unsafe { *fdk::aacDecoder_GetStreamInfo(self.instance) }
    }

    /// Whole frames missing between the last decoded access unit and `pts`.
    fn lost_frames(&self, pts: Timestamp) -> u32 {
        // Nothing decoded yet, there is no signal to substitute for.
        let Some(last_frame_end_pts) = self.last_frame_end_pts else {
            return 0;
        };
        let info = self.stream_info();
        let expected_pts = last_frame_end_pts + info.output_delay();
        let gap_frames = (pts - expected_pts).as_secs_f64() / info.frame_duration().as_secs_f64();
        if gap_frames < 0.5 {
            return 0;
        }
        // Overfilling only delays audio slightly, underfilling can leave nothing to play in time.
        // The tolerance keeps timestamp jitter on a whole-frame loss from adding an extra frame.
        (gap_frames - 0.1).ceil() as u32
    }

    /// Substitutes `count` lost frames, placed right after the last produced frame.
    fn conceal(&mut self, count: u32) -> Result<Vec<InputAudioSamples>, FdkAacDecoderError> {
        let mut concealed = Vec::new();
        for _ in 0..count {
            // Nothing decoded yet, there is no signal to substitute for.
            let Some(pts) = self.last_frame_end_pts else {
                break;
            };
            let FrameStatus::Frame(samples) = self.decode_frame(fdk::AACDEC_CONCEAL)? else {
                break;
            };
            if let Some(batch) = self.prepare_batch(samples, pts) {
                trace!(?batch, "FDK AAC decoder concealed a frame.");
                concealed.push(batch);
            }
        }
        Ok(concealed)
    }

    fn decode(
        &mut self,
        chunk: &EncodedInputChunk,
    ) -> Result<Vec<InputAudioSamples>, FdkAacDecoderError> {
        let buffer_size = chunk.data.len() as u32;
        // bytes left in the buffer
        let mut bytes_valid = buffer_size;
        let mut buffer = chunk.data.to_vec();

        let mut decoded = Vec::new();
        let mut is_first_frame = true;

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
                let samples = match self.decode_frame(0)? {
                    FrameStatus::Frame(samples) => samples,
                    FrameStatus::Skipped => continue,
                    FrameStatus::NotEnoughBits => break,
                };
                let start_pts = match is_first_frame {
                    false => self.last_frame_end_pts.unwrap_or_else(|| {
                        // should never happen, set by prepare_batch
                        chunk.pts - self.stream_info().output_delay()
                    }),
                    true => chunk.pts - self.stream_info().output_delay(),
                };
                let batch = self.prepare_batch(samples, start_pts);
                is_first_frame = false;
                if let Some(batch) = batch {
                    trace!(?batch, "FDK AAC decoder produced samples.");
                    decoded.push(batch)
                }
            }
        }
        Ok(decoded)
    }

    /// Drains the audio the decoder still holds (the delay reported with the last frame).
    /// Flushing the filterbanks keeps producing frames, so stop once the delayed audio is out
    /// and cut the last frame to the part that carries it.
    fn flush(mut self) -> Vec<InputAudioSamples> {
        let info = self.stream_info();
        let mut remaining_samples = info.outputDelay as usize;
        let mut flushed = Vec::new();
        while remaining_samples > 0 {
            // Nothing decoded yet, so nothing is held either.
            let Some(pts) = self.last_frame_end_pts else {
                break;
            };
            let mut samples = match self.decode_frame(fdk::AACDEC_FLUSH) {
                Ok(FrameStatus::Frame(samples)) if !samples.is_empty() => samples,
                Ok(_) => break,
                Err(err) => {
                    warn!("Failed to flush FDK AAC decoder: {err}");
                    break;
                }
            };
            match &mut samples {
                AudioSamples::Mono(samples) => samples.truncate(remaining_samples),
                AudioSamples::Stereo(samples) => samples.truncate(remaining_samples),
            }
            remaining_samples -= samples.len();
            if let Some(batch) = self.prepare_batch(samples, pts) {
                trace!(?batch, "FDK AAC decoder flushed samples.");
                flushed.push(batch);
            }
        }
        flushed
    }

    /// Stamps the last produced frame, starting at `start_pts`, minus the part of it that still
    /// has to be skipped. Skipping moves the start but not the end, `None` when nothing is left.
    fn prepare_batch(
        &mut self,
        mut samples: AudioSamples,
        start_pts: Timestamp,
    ) -> Option<InputAudioSamples> {
        let info = self.stream_info();
        let skip_samples = self.skip_samples.get_or_insert(info.outputDelay as usize);
        let skipped = usize::min(*skip_samples, samples.len());
        *skip_samples -= skipped;
        match &mut samples {
            AudioSamples::Mono(samples) => drop(samples.drain(..skipped)),
            AudioSamples::Stereo(samples) => drop(samples.drain(..skipped)),
        }

        let batch = InputAudioSamples {
            samples,
            start_pts: start_pts + info.samples_to_duration(skipped),
            sample_rate: info.sampleRate as u32,
        };
        self.last_frame_end_pts = Some(batch.end_pts());
        match batch.is_empty() {
            true => None,
            false => Some(batch),
        }
    }

    /// Errors only when fdk can't continue.
    fn decode_frame(&mut self, flags: u32) -> Result<FrameStatus, FdkAacDecoderError> {
        let result = unsafe {
            fdk::aacDecoder_DecodeFrame(
                self.instance,
                self.decoded_samples_buffer.as_mut_ptr(),
                self.decoded_samples_buffer.len() as i32,
                flags,
            )
        };

        match result {
            fdk::AAC_DECODER_ERROR_AAC_DEC_OK => {}
            fdk::AAC_DECODER_ERROR_AAC_DEC_NOT_ENOUGH_BITS => {
                return Ok(FrameStatus::NotEnoughBits);
            }
            // fdk stepped past the bad data and searches for the next sync word on the next call.
            fdk::AAC_DECODER_ERROR_AAC_DEC_TRANSPORT_SYNC_ERROR => {
                warn!("FDK AAC decoder lost transport sync.");
                return Ok(FrameStatus::Skipped);
            }
            // The output holds fdk's concealment. Explicit concealment normally reports OK, the
            // warning is only for corrupt input.
            fdk::AAC_DECODER_ERROR_aac_dec_decode_error_start
                ..=fdk::AAC_DECODER_ERROR_aac_dec_decode_error_end => {
                if flags & fdk::AACDEC_CONCEAL == 0 {
                    warn!("FDK AAC decoder concealed a corrupt frame: {result:#x}");
                }
            }
            _ => return Err(FdkAacDecoderError::FdkDecoderError(result)),
        }

        let info = self.stream_info();
        let raw_frame_size = (info.frameSize * info.numChannels) as usize;
        let samples = match info.numChannels {
            1 => AudioSamples::Mono(
                self.decoded_samples_buffer[..raw_frame_size]
                    .iter()
                    .map(|value| *value as f64 / i16::MAX as f64)
                    .collect(),
            ),
            2 => AudioSamples::Stereo(
                self.decoded_samples_buffer[..raw_frame_size]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| (c[0] as f64 / i16::MAX as f64, c[1] as f64 / i16::MAX as f64))
                    .collect(),
            ),
            // Not reachable while fdk downmixes to two channels.
            channels => {
                warn!("FDK AAC decoder produced unsupported {channels} channels.");
                return Ok(FrameStatus::Skipped);
            }
        };
        Ok(FrameStatus::Frame(samples))
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            fdk::aacDecoder_Close(self.instance);
        }
    }
}

trait StreamInfoExt {
    fn samples_to_duration(&self, samples: usize) -> Duration;
    fn output_delay(&self) -> Duration;
    fn frame_duration(&self) -> Duration;
}

impl StreamInfoExt for fdk::CStreamInfo {
    fn samples_to_duration(&self, samples: usize) -> Duration {
        Duration::from_secs_f64(samples as f64 / self.sampleRate as f64)
    }

    fn output_delay(&self) -> Duration {
        self.samples_to_duration(self.outputDelay as usize)
    }

    fn frame_duration(&self) -> Duration {
        self.samples_to_duration(self.frameSize as usize)
    }
}
