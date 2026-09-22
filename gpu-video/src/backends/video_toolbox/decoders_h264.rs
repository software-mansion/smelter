use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    sync::{Arc, Condvar, Mutex, MutexGuard, atomic},
    time::Duration,
};

use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_video_toolbox as vt;
use tracing::debug;

use crate::{
    DecoderEvent, OutputFrame, RawFrameData, VideoDecoderError,
    backends::video_toolbox::{
        decoder::{Completion, VTDecoder, download_to_bytes},
        error::VTDecoderError,
    },
    decoders::{H264EventProcessor, VideoDecoderBackend},
    device::DecoderParameters,
    frame_sorter::{DecodeResult, FrameSorter},
    parser::{
        decoder_instructions::DecoderInstruction,
        h264::{AccessUnit, H264Parser},
        reference_manager::DecodeInformation,
    },
};

type ConvertFn<T> =
    Box<dyn Fn(&cf::CFRetained<cv::CVBuffer>) -> Result<T, VTDecoderError> + Send + Sync>;

pub(crate) struct VTDecoderH264<T> {
    event_processor: H264EventProcessor,
    decoder: VTDecoder,
    shared: Arc<Shared<T>>,
    max_in_flight: u64,
    decode_flags: vt::VTDecodeFrameFlags,
}

impl VTDecoderH264<RawFrameData> {
    pub(super) fn new_bytes(
        parameters: DecoderParameters,
        on_frame_callback: Box<dyn FnMut(OutputFrame<RawFrameData>) + Send>,
    ) -> Self {
        let convert = Box::new(|buffer: &_| download_to_bytes(buffer).map_err(Into::into));
        Self::new(parameters, convert, on_frame_callback, false)
    }
}

#[cfg(feature = "wgpu")]
impl VTDecoderH264<wgpu::Texture> {
    pub(super) fn new_wgpu_textures(
        wgpu_device: wgpu::Device,
        parameters: DecoderParameters,
        on_frame_callback: Box<dyn FnMut(OutputFrame<wgpu::Texture>) + Send>,
    ) -> Result<Self, super::error::VTInitError> {
        use super::decoder::wgpu_api::{make_texture_cache, to_wgpu_texture};

        let cache = make_texture_cache(&wgpu_device)?;
        let convert = Box::new(move |buffer: &cf::CFRetained<cv::CVBuffer>| {
            to_wgpu_texture(&wgpu_device, &cache, buffer)
        });
        Ok(Self::new(parameters, convert, on_frame_callback, true))
    }
}

impl<T: Send + 'static> VTDecoderH264<T> {
    fn new(
        parameters: DecoderParameters,
        convert: ConvertFn<T>,
        on_frame_callback: Box<dyn FnMut(OutputFrame<T>) + Send>,
        metal_compatible_output: bool,
    ) -> Self {
        let event_processor = H264EventProcessor::new(
            H264Parser::new_avcc_output(),
            parameters.corrupted_state_handling,
        );
        let max_in_flight = parameters.max_in_flight_submissions as u64;
        let decode_flags = if max_in_flight > 0 {
            vt::VTDecodeFrameFlags::Frame_EnableAsynchronousDecompression
        } else {
            vt::VTDecodeFrameFlags::empty()
        };

        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    next_submission_index: 0,
                    next_to_emit: 0,
                    completed: BinaryHeap::new(),
                    frame_sorter: FrameSorter::default(),
                    on_frame_callback,
                    decode_failed: event_processor.decode_failed_flag(),
                }),
                convert,
                changed: Condvar::new(),
            }),
            event_processor,
            decoder: VTDecoder::new(parameters.usage_flags, metal_compatible_output),
            max_in_flight,
            decode_flags,
        }
    }

    fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        let flush = matches!(event, DecoderEvent::Flush);
        let instructions = self.event_processor.process_event(event)?;

        for instruction in instructions {
            match instruction {
                DecoderInstruction::Sps { sps, raw_bytes } => {
                    self.decoder.process_sps(sps, raw_bytes)
                }
                DecoderInstruction::Pps { pps, raw_bytes } => {
                    self.decoder.process_pps(pps, raw_bytes)
                }
                DecoderInstruction::Decode { decode_info, .. } => {
                    self.submit(decode_info, false, timeout)?
                }
                DecoderInstruction::Idr { decode_info, .. } => {
                    self.submit(decode_info, true, timeout)?
                }
                DecoderInstruction::Drop { .. } => {}
            }
        }

        if flush {
            self.decoder.wait_for_pending_frames()?;

            let mut state = self
                .shared
                .wait_while(timeout, |state| state.in_flight() > 0)?;
            state.flush_sorter();
        }

        Ok(())
    }

    fn submit(
        &mut self,
        decode_info: DecodeInformation,
        is_idr: bool,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        let submission_index = self.allocate_submission_index(timeout)?;

        let shared = self.shared.clone();
        let result =
            self.decoder
                .submit(decode_info, is_idr, self.decode_flags, move |completion| {
                    shared.complete(submission_index, completion)
                });

        if let Err(err) = result {
            self.shared.complete(submission_index, Completion::Failed);
            return Err(err.into());
        }

        Ok(())
    }

    fn allocate_submission_index(&self, timeout: Duration) -> Result<u64, VideoDecoderError> {
        let mut state = if self.max_in_flight > 0 {
            self.shared
                .wait_while(timeout, |state| state.in_flight() >= self.max_in_flight)?
        } else {
            self.shared.state.lock().unwrap()
        };

        let submission_index = state.next_submission_index;
        state.next_submission_index += 1;
        Ok(submission_index)
    }
}

impl VideoDecoderBackend for VTDecoderH264<RawFrameData> {
    fn process_event_bytes(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        self.process_event(event, timeout)
    }
}

#[cfg(feature = "wgpu")]
impl crate::decoders::WgpuVideoDecoderBackend for VTDecoderH264<wgpu::Texture> {
    fn process_event_textures(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        self.process_event(event, timeout)
    }
}

/// VideoToolbox's output handlers run on its own threads with no ordering guarantee, but the
/// frame sorter needs frames in decode order, so a completion is held until every earlier
/// submission has been emitted.
struct Shared<T> {
    state: Mutex<State<T>>,
    convert: ConvertFn<T>,
    changed: Condvar,
}

struct State<T> {
    next_submission_index: u64,
    next_to_emit: u64,
    completed: BinaryHeap<Reverse<Entry<T>>>,
    frame_sorter: FrameSorter<T>,
    on_frame_callback: Box<dyn FnMut(OutputFrame<T>) + Send>,
    decode_failed: Arc<atomic::AtomicBool>,
}

impl<T: Send + 'static> Shared<T> {
    fn wait_while(
        &self,
        timeout: Duration,
        condition: impl FnMut(&mut State<T>) -> bool,
    ) -> Result<MutexGuard<'_, State<T>>, VideoDecoderError> {
        let state = self.state.lock().unwrap();
        let (state, result) = self
            .changed
            .wait_timeout_while(state, timeout, condition)
            .unwrap();

        if result.timed_out() {
            return Err(VideoDecoderError::DecodeSubmissionTimeout);
        }

        Ok(state)
    }

    fn complete(&self, submission_index: u64, completion: Completion) {
        let converted = self.convert(completion);

        let mut state = self.state.lock().unwrap();
        state.completed.push(Reverse(Entry {
            submission_index,
            converted,
        }));

        while state
            .completed
            .peek()
            .is_some_and(|Reverse(entry)| entry.submission_index == state.next_to_emit)
        {
            let Reverse(entry) = state.completed.pop().unwrap();
            state.next_to_emit += 1;
            state.emit(entry.converted);
        }

        self.changed.notify_all();
    }

    fn convert(&self, completion: Completion) -> Converted<T> {
        let frame = match completion {
            Completion::Frame(frame) => frame,
            Completion::Dropped => return Converted::Dropped,
            Completion::Failed => return Converted::Failed,
        };

        match (self.convert)(&frame.frame) {
            Ok(converted) => Converted::Frame(DecodeResult {
                frame: converted,
                metadata: frame.metadata,
            }),
            Err(err) => {
                debug!("Failed to convert a decoded frame: {err}");
                Converted::Failed
            }
        }
    }
}

impl<T> State<T> {
    fn in_flight(&self) -> u64 {
        self.next_submission_index - self.next_to_emit
    }

    fn emit(&mut self, converted: Converted<T>) {
        let frame = match converted {
            Converted::Frame(frame) => frame,
            Converted::Dropped => {
                debug!("Skipping a frame dropped by VideoToolbox");
                return;
            }
            Converted::Failed => {
                self.decode_failed.store(true, atomic::Ordering::Relaxed);
                return;
            }
        };

        for frame in self.frame_sorter.put(frame) {
            (self.on_frame_callback)(frame);
        }
    }

    fn flush_sorter(&mut self) {
        for frame in self.frame_sorter.flush() {
            (self.on_frame_callback)(frame);
        }
    }
}

enum Converted<T> {
    Frame(DecodeResult<T>),
    Dropped,
    Failed,
}

struct Entry<T> {
    submission_index: u64,
    converted: Converted<T>,
}

impl<T> PartialEq for Entry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.submission_index == other.submission_index
    }
}

impl<T> Eq for Entry<T> {}

impl<T> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Entry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.submission_index.cmp(&other.submission_index)
    }
}
