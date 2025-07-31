use std::sync::Arc;

use compositor_render::InputId;
use crossbeam_channel::Sender;
use tracing::{debug, trace, warn};
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::prelude::*;
use crate::thread_utils::{InitializableThread, ThreadMetadata};
use crate::{
    error::DecoderInitError,
    pipeline::{
        decoder::{libopus::OpusDecoder, AudioDecoderStream},
        resampler::decoder_resampler::ResampledDecoderStream,
        rtp::{
            depayloader::{DepayloaderOptions, DepayloaderStream},
            RtpPacket, RtpTimestampSync,
        },
        webrtc::{
            error::WhipServerError,
            whip_input::{negotiated_codecs::NegotiatedAudioCodecsInfo, AsyncReceiverIter},
            WhipWhepServerState,
        },
        PipelineCtx,
    },
};

pub async fn process_audio_track(
    state: WhipWhepServerState,
    input_id: InputId,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
) -> Result<(), WhipServerError> {
    let Some(_negotiated_codecs) = NegotiatedAudioCodecsInfo::new(transceiver).await else {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx } = state;
    let samples_sender =
        inputs.get_with(&input_id, |input| Ok(input.input_samples_sender.clone()))?;
    let handle = AudioTrackThread::spawn(&input_id.0, (ctx.clone(), samples_sender))?;

    let mut timestamp_sync = RtpTimestampSync::new(ctx.queue_sync_point, 48_000);

    while let Ok((packet, _)) = track.read_rtp().await {
        let timestamp = timestamp_sync.timestamp(packet.header.timestamp);

        if let Err(e) = handle
            .rtp_packet_sender
            .send(PipelineEvent::Data(RtpPacket { packet, timestamp }))
            .await
        {
            debug!("Failed to send audio RTP packet: {e}");
        }
    }
    Ok(())
}

pub(super) struct AudioTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct AudioTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
}

impl InitializableThread for AudioTrackThread {
    type InitOptions = (Arc<PipelineCtx>, Sender<PipelineEvent<InputAudioSamples>>);

    type SpawnOutput = AudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    const LABEL: &'static str = "Whip audio decoder";

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, samples_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5);
        let output_sample_rate = ctx.mixing_sample_rate;

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DepayloaderStream::new(DepayloaderOptions::Opus, packet_stream).flatten();

        let decoded_stream =
            AudioDecoderStream::<OpusDecoder, _>::new(ctx, (), depayloader_stream)?.flatten();

        let resampled_stream =
            ResampledDecoderStream::new(output_sample_rate, decoded_stream).flatten();

        let result_stream = resampled_stream
            .filter_map(|event| match event {
                PipelineEvent::Data(batch) => Some(PipelineEvent::Data(batch)),
                // Do not send EOS to queue
                // TODO: maybe queue should be able to handle packets after EOS
                PipelineEvent::EOS => None,
            })
            .inspect(|batch| trace!(?batch, "WHIP input produced a sample batch"));

        let state = Self {
            stream: Box::new(result_stream),
            samples_sender,
        };
        let output = AudioTrackThreadHandle { rtp_packet_sender };
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
            thread_name: "Whip Audio Decoder",
            thread_instance_name: "Input",
        }
    }
}
