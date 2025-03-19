use std::time::Duration;

use crate::{
    error::{InputInitError, RegisterInputError},
    queue::PipelineEvent,
};

use compositor_render::{Frame, InputId};
use crossbeam_channel::{bounded, Receiver};
use rtp::{RtpReceiver, RtpReceiverOptions};
use whip::WhipReceiver;

use self::mp4::{Mp4, Mp4Options};

use super::{
    decoder::{
        start_audio_decoder_thread, start_audio_resampler_only_thread, start_video_decoder_thread,
        AudioDecoderOptions, DecodedDataReceiver, VideoDecoderOptions,
    },
    types::{DecodedSamples, EncodedChunk, RawDataSender},
    PipelineCtx, Port,
};

#[cfg(feature = "decklink")]
pub mod decklink;
pub mod mp4;
pub mod rtp;
pub mod whip;

pub enum Input {
    Rtp(RtpReceiver),
    Mp4(Mp4),
    Whip(WhipReceiver),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLink),
    RawDataInput,
}

#[derive(Debug, Clone)]
pub enum InputOptions {
    Rtp(RtpReceiverOptions),
    Mp4(Mp4Options),
    Whip,
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLinkOptions),
}

#[derive(Debug, Clone)]
pub struct RawDataInputOptions {
    pub video: bool,
    pub audio: bool,
}

pub enum InputInitInfo {
    Rtp {
        port: Option<Port>,
    },
    Mp4 {
        video_duration: Option<Duration>,
        audio_duration: Option<Duration>,
    },
    Whip {
        bearer_token: String,
    },
    Other,
}

struct InputInitResult {
    input: Input,
    video: Option<VideoInputReceiver>,
    audio: Option<AudioInputReceiver>,
    init_info: InputInitInfo,
}

#[derive(Debug)]
pub(super) enum VideoInputReceiver {
    #[allow(dead_code)]
    Raw {
        frame_receiver: Receiver<PipelineEvent<Frame>>,
    },
    Encoded {
        chunk_receiver: Receiver<PipelineEvent<EncodedChunk>>,
        decoder_options: VideoDecoderOptions,
    },
}

pub(super) enum AudioInputReceiver {
    #[allow(dead_code)]
    Raw {
        sample_receiver: Receiver<PipelineEvent<DecodedSamples>>,
        sample_rate: u32,
    },
    Encoded {
        chunk_receiver: Receiver<PipelineEvent<EncodedChunk>>,
        decoder_options: AudioDecoderOptions,
    },
}

pub(super) trait InputOptionsExt<NewInputResult> {
    fn new_input(
        &self,
        input_id: &InputId,
        ctx: &PipelineCtx,
    ) -> Result<(Input, DecodedDataReceiver, NewInputResult), RegisterInputError>;
}

impl InputOptionsExt<InputInitInfo> for InputOptions {
    fn new_input(
        &self,
        input_id: &InputId,
        ctx: &PipelineCtx,
    ) -> Result<(Input, DecodedDataReceiver, InputInitInfo), RegisterInputError> {
        start_input_threads(input_id, self.clone(), ctx)
            .map_err(|e| RegisterInputError::InputError(input_id.clone(), e))
    }
}

impl InputOptionsExt<RawDataSender> for RawDataInputOptions {
    fn new_input(
        &self,
        _input_id: &InputId,
        _ctx: &PipelineCtx,
    ) -> Result<(Input, DecodedDataReceiver, RawDataSender), RegisterInputError> {
        let (video_sender, video_receiver) = match self.video {
            true => {
                let (sender, receiver) = bounded(1000);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        let (audio_sender, audio_receiver) = match self.audio {
            true => {
                let (sender, receiver) = bounded(1000);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        Ok((
            Input::RawDataInput,
            DecodedDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
            RawDataSender {
                video: video_sender,
                audio: audio_sender,
            },
        ))
    }
}

/// Start entire processing pipeline for an input, including decoders and resamplers.
fn start_input_threads(
    input_id: &InputId,
    options: InputOptions,
    pipeline_ctx: &PipelineCtx,
) -> Result<(Input, DecodedDataReceiver, InputInitInfo), InputInitError> {
    match options {
        InputOptions::Rtp(opts) => {
            let InputInitResult {
                input,
                video,
                audio,
                init_info,
            } = RtpReceiver::start_new_input(input_id, opts)?;
            let decoder_data_receiver =
                setup_audio_video_start_decoder(pipeline_ctx, input_id, video, audio)?;
            Ok((input, decoder_data_receiver, init_info))
        }
        InputOptions::Mp4(opts) => {
            let InputInitResult {
                input,
                video,
                audio,
                init_info,
            } = Mp4::start_new_input(input_id, opts, &pipeline_ctx.download_dir)?;
            let decoder_data_receiver =
                setup_audio_video_start_decoder(pipeline_ctx, input_id, video, audio)?;
            Ok((input, decoder_data_receiver, init_info))
        }
        InputOptions::Whip => {
            let (video_sender, video_receiver) = bounded(10);
            let (audio_sender, audio_receiver) = bounded(10);
            let (input, init_info) =
                WhipReceiver::start_new_input(input_id, pipeline_ctx, video_sender, audio_sender)?;
            Ok((
                input,
                DecodedDataReceiver {
                    video: Some(video_receiver),
                    audio: Some(audio_receiver),
                },
                init_info,
            ))
        }
        #[cfg(feature = "decklink")]
        InputOptions::DeckLink(opts) => {
            let InputInitResult {
                input,
                video,
                audio,
                init_info,
            } = decklink::DeckLink::start_new_input(input_id, opts)?;
            let decoder_data_receiver =
                setup_audio_video_start_decoder(pipeline_ctx, input_id, video, audio)?;
            Ok((input, decoder_data_receiver, init_info))
        }
    }
}

fn setup_audio_video_start_decoder(
    pipeline_ctx: &PipelineCtx,
    input_id: &InputId,
    video: Option<VideoInputReceiver>,
    audio: Option<AudioInputReceiver>,
) -> Result<DecodedDataReceiver, InputInitError> {
    let video = if let Some(video) = video {
        match video {
            VideoInputReceiver::Raw { frame_receiver } => Some(frame_receiver),
            VideoInputReceiver::Encoded {
                chunk_receiver,
                decoder_options,
            } => {
                let (sender, receiver) = bounded(10);
                start_video_decoder_thread(
                    decoder_options,
                    pipeline_ctx,
                    chunk_receiver,
                    sender,
                    input_id.clone(),
                    true,
                )?;
                Some(receiver)
            }
        }
    } else {
        None
    };

    let audio = if let Some(audio) = audio {
        match audio {
            AudioInputReceiver::Raw {
                sample_receiver,
                sample_rate,
            } => {
                let (sender, receiver) = bounded(10);
                start_audio_resampler_only_thread(
                    sample_rate,
                    pipeline_ctx.mixing_sample_rate,
                    sample_receiver,
                    sender,
                    input_id.clone(),
                )?;
                Some(receiver)
            }
            AudioInputReceiver::Encoded {
                chunk_receiver,
                decoder_options,
            } => {
                let (sender, receiver) = bounded(10);
                start_audio_decoder_thread(
                    decoder_options,
                    pipeline_ctx.mixing_sample_rate,
                    chunk_receiver,
                    sender,
                    input_id.clone(),
                    true,
                )?;
                Some(receiver)
            }
        }
    } else {
        None
    };

    Ok(DecodedDataReceiver { video, audio })
}
