use std::{sync::Arc, time::Duration};

use crate::{
    error::InputInitError,
    pipeline::webrtc::{WhipInput, WhipInputOptions},
    queue::PipelineEvent,
};

use compositor_render::{Frame, InputId};
use crossbeam_channel::Receiver;
use rtp::{RtpInput, RtpInputOptions};

use self::mp4::{Mp4, Mp4Options};

use super::{
    decoder::{AudioDecoderOptions, DecodedDataReceiver, VideoDecoderOptions},
    types::{DecodedSamples, EncodedChunk},
    PipelineCtx, Port,
};

#[cfg(feature = "decklink")]
pub mod decklink;
pub mod mp4;
pub mod raw_data;
pub mod rtp;

pub enum Input {
    Rtp(RtpInput),
    Mp4(Mp4),
    Whip(WhipInput),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLink),
    RawDataInput,
}

#[derive(Debug, Clone)]
pub enum InputOptions {
    Rtp(RtpInputOptions),
    Mp4(Mp4Options),
    Whip(WhipInputOptions),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLinkOptions),
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

pub(super) fn new_external_input(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    options: InputOptions,
) -> Result<(Input, InputInitInfo, DecodedDataReceiver), InputInitError> {
    match options {
        InputOptions::Rtp(opts) => RtpInput::new(ctx, input_id, opts),
        InputOptions::Mp4(opts) => Mp4::new(ctx, &input_id, opts),
        InputOptions::Whip(opts) => todo!(),
        InputOptions::DeckLink(opts) => todo!(),
    }
}

//pub(super) trait InputOptionsExt<NewInputResult> {
//    fn new_input(
//        &self,
//        input_id: &InputId,
//        ctx: &PipelineCtx,
//    ) -> Result<(Input, DecodedDataReceiver, NewInputResult), RegisterInputError>;
//}
//
//impl InputOptionsExt<InputInitInfo> for InputOptions {
//    fn new_input(
//        &self,
//        input_id: InputId,
//        ctx: PipelineCtx,
//    ) -> Result<(Input, InputInitInfo, DecodedDataReceiver), RegisterInputError> {
//        start_input_threads(ctx, input_id, self.clone())
//            .map_err(|e| RegisterInputError::InputError(input_id.clone(), e))
//    }
//}
//
///// Start entire processing pipeline for an input, including decoders and resamplers.
//fn start_input_threads(
//    ctx: Arc<PipelineCtx>,
//    input_id: InputId,
//    options: InputOptions,
//) -> Result<(Input, InputInitInfo, DecodedDataReceiver), InputInitError> {
//    match options {
//        InputOptions::Rtp(opts) => RtpInput::new(ctx, input_id, opts),
//        InputOptions::Mp4(opts) => {
//            let InputInitResult {
//                input,
//                video,
//                audio,
//                init_info,
//            } = Mp4::start_new_input(input_id, opts, &pipeline_ctx.download_dir)?;
//            let decoder_data_receiver =
//                setup_and_start_decoders_threads(pipeline_ctx, input_id, video, audio)?;
//            Ok((input, decoder_data_receiver, init_info))
//        }
//        InputOptions::Whip(opts) => {
//            let (video_sender, video_receiver) = bounded(10);
//            let (audio_sender, audio_receiver) = bounded(10);
//            let (input, init_info) = WhipInput::start_new_input(
//                input_id,
//                opts,
//                pipeline_ctx,
//                video_sender,
//                audio_sender,
//            )?;
//            Ok((
//                input,
//                DecodedDataReceiver {
//                    video: Some(video_receiver),
//                    audio: Some(audio_receiver),
//                },
//                init_info,
//            ))
//        }
//        #[cfg(feature = "decklink")]
//        InputOptions::DeckLink(opts) => {
//            let InputInitResult {
//                input,
//                video,
//                audio,
//                init_info,
//            } = decklink::DeckLink::start_new_input(input_id, opts)?;
//            let decoder_data_receiver =
//                setup_and_start_decoders_threads(pipeline_ctx, input_id, video, audio)?;
//            Ok((input, decoder_data_receiver, init_info))
//        }
//    }
//}
//
//fn setup_and_start_decoders_threads(
//    pipeline_ctx: &PipelineCtx,
//    input_id: &InputId,
//    video: Option<VideoInputReceiver>,
//    audio: Option<AudioInputReceiver>,
//) -> Result<DecodedDataReceiver, InputInitError> {
//    let video = if let Some(video) = video {
//        match video {
//            VideoInputReceiver::Raw { frame_receiver } => Some(frame_receiver),
//            VideoInputReceiver::Encoded {
//                chunk_receiver,
//                decoder_options,
//            } => {
//                let (sender, receiver) = bounded(10);
//                start_video_decoder_thread(
//                    decoder_options,
//                    pipeline_ctx,
//                    chunk_receiver,
//                    sender,
//                    input_id.clone(),
//                    true,
//                )?;
//                Some(receiver)
//            }
//        }
//    } else {
//        None
//    };
//
//    let audio = if let Some(audio) = audio {
//        match audio {
//            AudioInputReceiver::Raw {
//                sample_receiver, ..
//            } => {
//                let (sender, receiver) = bounded(10);
//                start_audio_resampler_only_thread(
//                    pipeline_ctx.mixing_sample_rate,
//                    sample_receiver,
//                    sender,
//                    input_id.clone(),
//                );
//                Some(receiver)
//            }
//            AudioInputReceiver::Encoded {
//                chunk_receiver,
//                decoder_options,
//            } => {
//                let (sender, receiver) = bounded(10);
//                start_audio_decoder_thread(
//                    decoder_options,
//                    pipeline_ctx.mixing_sample_rate,
//                    chunk_receiver,
//                    sender,
//                    input_id.clone(),
//                    true,
//                )?;
//                Some(receiver)
//            }
//        }
//    } else {
//        None
//    };
//
//    Ok(DecodedDataReceiver { video, audio })
//}
