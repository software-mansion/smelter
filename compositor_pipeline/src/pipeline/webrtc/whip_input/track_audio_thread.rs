use std::sync::Arc;

use compositor_render::InputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, trace, warn, Level};
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::{
    audio_mixer::InputSamples,
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
    queue::PipelineEvent,
};

pub async fn process_audio_track(
    state: WhipWhepServerState,
    input_id: InputId,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
) -> Result<(), WhipServerError> {
    let Some(negotiated_codecs) = NegotiatedAudioCodecsInfo::new(transceiver).await else {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx } = state;
    let samples_sender =
        inputs.get_with(&input_id, |input| Ok(input.input_samples_sender.clone()))?;
    let handle =
        spawn_audio_track_thread(ctx.clone(), input_id, negotiated_codecs, samples_sender)?;

    let mut timestamp_sync = RtpTimestampSync::new(ctx.queue_sync_time, 48_000);

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

struct AudioTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

fn spawn_audio_track_thread(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    codec_info: NegotiatedAudioCodecsInfo,
    samples_sender: Sender<PipelineEvent<InputSamples>>,
) -> Result<AudioTrackThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!(
            "WHIP input audio track thread for input {}",
            &input_id
        ))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "WHIP input audio thread",
                input_id = input_id.to_string(),
            )
            .entered();

            let result = init_stream(ctx, codec_info);
            let stream = match result {
                Ok((stream, handle)) => {
                    result_sender.send(Ok(handle)).unwrap();
                    stream
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in stream {
                if samples_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream(
    ctx: Arc<PipelineCtx>,
    _codec_info: NegotiatedAudioCodecsInfo,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<InputSamples>>,
        AudioTrackThreadHandle,
    ),
    DecoderInitError,
> {
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

    Ok((result_stream, AudioTrackThreadHandle { rtp_packet_sender }))
}
