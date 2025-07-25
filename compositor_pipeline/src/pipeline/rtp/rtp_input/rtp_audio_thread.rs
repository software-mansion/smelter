use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
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

pub(super) struct RtpAudioThread<Decoder: AudioDecoder + 'static>(PhantomData<Decoder>);

pub(super) struct RtpAudioThreadOptions<Decoder: AudioDecoder> {
    pub ctx: Arc<PipelineCtx>,
    pub decoder_options: Decoder::Options,
    pub depayloader_options: DepayloaderOptions,
    pub decoded_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub sample_rate: u32,
}

impl<Decoder: AudioDecoder + 'static> InitializableThread for RtpAudioThread<Decoder> {
    type InitOptions = RtpAudioThreadOptions<Decoder>;

    type SpawnOutput = RtpAudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    type ThreadState = (
        Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
        Sender<PipelineEvent<InputAudioSamples>>,
    );

    const LABEL: &'static str = Decoder::LABEL;

    fn init(
        options: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, Self::ThreadState), Self::SpawnError> {
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

        let output = RtpAudioTrackThreadHandle {
            rtp_packet_sender,
            sample_rate,
        };
        let state = (Box::new(resampled_stream) as Box<_>, decoded_samples_sender);
        Ok((output, state))
    }

    fn run(state: Self::ThreadState) {
        let (stream, decoded_samples_sender) = state;
        for event in stream {
            if decoded_samples_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                return;
            }
        }
    }
}
