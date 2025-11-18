use std::{slice, sync::Arc};

use bytes::Bytes;
use crossbeam_channel::Sender;
use ffmpeg_next::Stream;
use smelter_render::InputId;

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            fdk_aac,
        },
        rtmp::rtmp_input::{Track, stream_state::StreamState},
        utils::input_buffer::InputBuffer,
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(super) fn handle_audio_track(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    stream: &Stream<'_>,
    buffer: InputBuffer,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
) -> Result<Track, InputInitError> {
    // not tested it was always null, but audio is in ADTS, so config is not
    // necessary
    let asc = read_extra_data(stream);
    let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer);
    let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
        input_ref.clone(),
        AudioDecoderThreadOptions {
            ctx: ctx.clone(),
            decoder_options: FdkAacDecoderOptions { asc },
            samples_sender,
            input_buffer_size: 2000,
        },
    )?;

    Ok(Track {
        index: stream.index(),
        handle,
        state,
    })
}

fn read_extra_data(stream: &Stream<'_>) -> Option<Bytes> {
    unsafe {
        let codecpar = (*stream.as_ptr()).codecpar;
        let size = (*codecpar).extradata_size;
        if size > 0 {
            Some(Bytes::copy_from_slice(slice::from_raw_parts(
                (*codecpar).extradata,
                size as usize,
            )))
        } else {
            None
        }
    }
}
