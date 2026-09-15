use std::sync::Arc;

use crate::{
    QueueInputOptions,
    pipeline::input::Input,
    queue::{QueueInput, QueueTrackOptions},
};

use crate::prelude::*;

/// RawData input - receives raw video frames and audio samples via in-process channels
/// and feeds them directly into the queue.
///
/// ## Timestamps
///
/// - Queue tracks are created immediately, with the `QueueTrackOffset` provided in options.
/// - Frames and samples are passed to the queue as is, PTS is not modified. Caller is
///   responsible for producing PTS values that match the selected offset mode:
///   - `QueueTrackOffset::Pts(offset)` - PTS relative to `sync_point + offset`
///   - `QueueTrackOffset::FromStart(offset)` - PTS of first frame should be zero
///   - `QueueTrackOffset::None` - PTS of first frame should be zero
/// - End of stream is signaled by dropping the sender.
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
pub struct RawDataInput;

impl RawDataInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: RawDataInputOptions,
    ) -> Result<(Input, RawDataInputSender, QueueInput), InputInitError> {
        let queue_input = QueueInput::new(
            &ctx,
            &input_ref,
            QueueInputOptions {
                required: options.required,
                ..Default::default()
            },
        );
        queue_input.set_stale_frame_timeout(ctx.stale_frame_timeout);
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: options.video,
            audio: options.audio,
            offset: options.offset,
        });

        Ok((
            Input::RawDataChannel,
            RawDataInputSender {
                video: video_sender.map(|sender| sender.into_inner()),
                audio: audio_sender.map(|sender| sender.into_inner()),
            },
            queue_input,
        ))
    }
}
