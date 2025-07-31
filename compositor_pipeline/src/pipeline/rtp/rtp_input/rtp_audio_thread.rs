use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
use crate::thread_utils::ThreadMetadata;
use crate::{
    pipeline::{
        decoder::{AudioDecoder, AudioDecoderStream},
        resampler::decoder_resampler::ResampledDecoderStream,
        rtp::{
            depayloader::{DepayloaderOptions, DepayloaderStream},
            RtpPacket,
        },
    },
    thread_utils::InitializableThread,
};

pub(crate) struct RtpAudioTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpPacket>>,
    pub sample_rate: u32,
}

pub(super) struct RtpAudioThreadOptions<Decoder: AudioDecoder> {
    pub ctx: Arc<PipelineCtx>,
    pub decoder_options: Decoder::Options,
    pub depayloader_options: DepayloaderOptions,
    pub decoded_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub sample_rate: u32,
}

pub(super) struct RtpAudioThread<Decoder: AudioDecoder + 'static> {
    stream: Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    _decoder: PhantomData<Decoder>,
}

impl<Decoder: AudioDecoder + 'static> InitializableThread for RtpAudioThread<Decoder> {
    type InitOptions = RtpAudioThreadOptions<Decoder>;

    type SpawnOutput = RtpAudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    const LABEL: &'static str = Decoder::LABEL;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let RtpAudioThreadOptions {
            ctx,
            decoder_options,
            depayloader_options,
            decoded_samples_sender,
            sample_rate,
        } = options;

        let mixing_sample_rate = ctx.mixing_sample_rate;
        let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);

        let depayloader_stream =
            DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());

        let decoder_stream = AudioDecoderStream::<Decoder, _>::new(
            ctx,
            decoder_options,
            depayloader_stream.flatten(),
        )?;

        let resampled_stream =
            ResampledDecoderStream::new(mixing_sample_rate, decoder_stream.flatten()).flatten();

        let state = Self {
            stream: Box::new(resampled_stream),
            samples_sender: decoded_samples_sender,
            _decoder: PhantomData,
        };
        let output = RtpAudioTrackThreadHandle {
            rtp_packet_sender,
            sample_rate,
        };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.samples_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Rtp Audio Decoder",
            thread_instance_name: "Input",
        }
    }
}
