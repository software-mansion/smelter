use compositor_render::{Frame, InputId};
use crossbeam_channel::{Receiver, Sender};

use crate::{
    error::InputInitError,
    pipeline::{types::EncodedChunk, PipelineCtx, VideoDecoder},
    queue::PipelineEvent,
};

use super::VideoDecoderOptions;
