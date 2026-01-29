use rtmp::{
    flv,
    server::{AudioConfig, AudioData},
};
use tracing::warn;

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
        },
        rtmp::rtmp_input::{RtmpConnectionContext, stream_state::RtmpStreamState},
    },
    prelude::*,
    thread_utils::InitializableThread,
};

pub(super) fn process_audio_config(ctx: &RtmpConnectionContext, config: AudioConfig) {
    if config.codec != flv::AudioCodec::Aac {
        warn!(?config.codec, "Unsupported audio codec");
        return;
    }

    let input_state = match ctx.inputs.get_input_state_by_ref(&ctx.input_ref) {
        Ok(state) => state,
        Err(err) => {
            warn!(?err, "Input state missing for AAC config");
            return;
        }
    };

    let options = FdkAacDecoderOptions {
        asc: Some(config.data.clone()),
    };

    let decoder_thread_options = AudioDecoderThreadOptions::<FdkAacDecoder> {
        ctx: ctx.ctx.clone(),
        decoder_options: options,
        samples_sender: input_state.input_samples_sender.clone(),
        input_buffer_size: 10,
    };

    let handle =
        AudioDecoderThread::<FdkAacDecoder>::spawn(ctx.input_ref.clone(), decoder_thread_options);

    match handle {
        Ok(handle) => {
            if let Err(err) = ctx.inputs.set_audio_decoder(&ctx.input_ref, handle) {
                warn!(?err, "Failed to store AAC decoder handle in state");
            }
        }
        Err(err) => warn!(?err, "Failed to init AAC decoder"),
    }
}

pub(super) fn process_audio(
    ctx: &RtmpConnectionContext,
    stream_state: &mut RtmpStreamState,
    audio: AudioData,
) {
    if audio.codec != flv::AudioCodec::Aac {
        return;
    }

    let Ok(Some(sender)) = ctx.inputs.audio_chunk_sender(&ctx.input_ref) else {
        warn!("Missing AAC decoder, skipping audio until config arrives");
        return;
    };

    let (pts, _dts) = stream_state.pts_dts_from_timestamps(audio.pts, audio.dts);

    let chunk = EncodedInputChunk {
        data: audio.data.clone(),
        pts,
        dts: None,
        kind: MediaKind::Audio(AudioCodec::Aac),
    };

    if sender.send(PipelineEvent::Data(chunk)).is_err() {
        warn!("Audio decoder channel closed");
    }
}
