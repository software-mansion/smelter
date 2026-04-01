use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    pipeline::{
        hls::HlsInput,
        mp4::Mp4Input,
        rtmp::RtmpServerInput,
        rtp::RtpInput,
        webrtc::{WhepInput, WhipInput},
    },
    queue::QueueInput,
};

use crate::prelude::*;

pub struct PipelineInput {
    pub input: Input,

    /// Some(received) - Whether EOS was received from queue on audio stream for that input.
    /// None - No audio configured for that input.
    pub(super) audio_eos_received: Option<bool>,
    /// Some(received) - Whether EOS was received from queue on video stream for that input.
    /// None - No video configured for that input.
    pub(super) video_eos_received: Option<bool>,
}

pub enum Input {
    Rtp(RtpInput),
    RtmpServer(RtmpServerInput),
    Mp4(Mp4Input),
    Whip(WhipInput),
    Whep(WhepInput),
    Hls(HlsInput),
    #[cfg(target_os = "linux")]
    V4l2(super::v4l2::V4l2Input),
    #[cfg(feature = "decklink")]
    DeckLink(super::decklink::DeckLink),
    RawDataChannel,
}

impl Input {
    pub fn kind(&self) -> InputProtocolKind {
        match self {
            Input::Rtp(_input) => InputProtocolKind::Rtp,
            Input::RtmpServer(_input) => InputProtocolKind::Rtmp,
            Input::Mp4(_input) => InputProtocolKind::Mp4,
            Input::Whip(_input) => InputProtocolKind::Whip,
            Input::Whep(_input) => InputProtocolKind::Whep,
            Input::Hls(_input) => InputProtocolKind::Hls,
            #[cfg(target_os = "linux")]
            Input::V4l2(_input) => InputProtocolKind::V4l2,
            #[cfg(feature = "decklink")]
            Input::DeckLink(_input) => InputProtocolKind::DeckLink,
            Input::RawDataChannel => InputProtocolKind::RawDataChannel,
        }
    }

    pub fn seek(&self, position: Duration) -> Result<(), UpdateInputError> {
        match self {
            Input::Mp4(input) => {
                input.seek(position);
                Ok(())
            }
            _ => Err(UpdateInputError::SeekNotSupported(self.kind())),
        }
    }

    pub fn pause(&self) -> Result<(), UpdateInputError> {
        match self {
            Input::Mp4(input) => {
                input.pause();
                Ok(())
            }
            _ => Err(UpdateInputError::PausingNotSupported(self.kind())),
        }
    }

    pub fn resume(&self) -> Result<(), UpdateInputError> {
        match self {
            Input::Mp4(input) => {
                input.resume();
                Ok(())
            }
            _ => Err(UpdateInputError::PausingNotSupported(self.kind())),
        }
    }
}

pub(super) fn new_external_input(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    options: RegisterInputOptions,
) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
    match options {
        RegisterInputOptions::Rtp(opts) => RtpInput::new_input(ctx, input_ref, opts),
        RegisterInputOptions::RtmpServer(opts) => RtmpServerInput::new_input(ctx, input_ref, opts),
        RegisterInputOptions::Mp4(opts) => Mp4Input::new_input(ctx, input_ref, opts),
        RegisterInputOptions::Hls(opts) => HlsInput::new_input(ctx, input_ref, opts),
        RegisterInputOptions::Whip(opts) => WhipInput::new_input(ctx, input_ref, opts),
        RegisterInputOptions::Whep(opts) => WhepInput::new_input(ctx, input_ref, opts),
        #[cfg(target_os = "linux")]
        RegisterInputOptions::V4l2(opts) => super::v4l2::V4l2Input::new_input(ctx, input_ref, opts),
        #[cfg(feature = "decklink")]
        RegisterInputOptions::DeckLink(opts) => {
            super::decklink::DeckLink::new_input(ctx, input_ref, opts)
        }
    }
}

/// This method doesn't take pipeline lock for the whole scope,
/// because input registration can potentially take a relatively long time.
pub(super) fn register_pipeline_input<BuildFn, NewInputResult>(
    pipeline: &Arc<Mutex<Pipeline>>,
    input_id: InputId,
    build_input: BuildFn,
) -> Result<NewInputResult, RegisterInputError>
where
    BuildFn: FnOnce(
        Arc<PipelineCtx>,
        Ref<InputId>,
    ) -> Result<(Input, NewInputResult, QueueInput), InputInitError>,
{
    if pipeline.lock().unwrap().inputs.contains_key(&input_id) {
        return Err(RegisterInputError::AlreadyRegistered(input_id));
    }

    let pipeline_ctx = pipeline.lock().unwrap().ctx.clone();

    let (input, input_result, queue_input) = build_input(pipeline_ctx, Ref::new(&input_id))
        .map_err(|err| RegisterInputError::InputError(input_id.clone(), err))?;

    // TODO: for now assume that
    let (audio_eos_received, video_eos_received) = (Some(false), Some(false));

    let pipeline_input = PipelineInput {
        input,
        audio_eos_received,
        video_eos_received,
    };

    let mut guard = pipeline.lock().unwrap();

    if guard.inputs.contains_key(&input_id) {
        return Err(RegisterInputError::AlreadyRegistered(input_id));
    };

    if pipeline_input.audio_eos_received.is_some() {
        for (_, output) in guard.outputs.iter_mut() {
            if let Some(ref mut cond) = output.audio_end_condition {
                cond.on_input_registered(&input_id);
            }
        }
    }

    if pipeline_input.video_eos_received.is_some() {
        for (_, output) in guard.outputs.iter_mut() {
            if let Some(ref mut cond) = output.video_end_condition {
                cond.on_input_registered(&input_id);
            }
        }
    }

    guard.inputs.insert(input_id.clone(), pipeline_input);
    guard.queue.add_input(&input_id, queue_input);
    guard.audio_mixer.register_input(input_id.clone());
    guard.renderer.register_input(input_id);

    Ok(input_result)
}

impl PipelineInput {
    pub(super) fn on_audio_eos(&mut self) {
        self.audio_eos_received = self.audio_eos_received.map(|_| true);
    }
    pub(super) fn on_video_eos(&mut self) {
        self.video_eos_received = self.video_eos_received.map(|_| true);
    }
}
