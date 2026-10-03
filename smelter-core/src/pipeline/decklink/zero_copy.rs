use std::{collections::VecDeque, ptr::NonNull, sync::Arc, time::Duration};

use decklink::{FrameAllocator, FrameBuffer, PixelFormat, VideoInputFrame};
use gpu_video::HostMemoryBuffer;
use smelter_render::{BufferFormat, BufferLayout, FramePreProcessor, Resolution};
use tracing::warn;

use crate::prelude::*;

/// DeckLink asks for 14 buffers per format up front but captures cleanly with
/// 4.
const POOL_SIZE: usize = 6;
/// Bounds the wait on a hung GPU; the frame is dropped past it.
const COPY_TIMEOUT: Duration = Duration::from_millis(100);

/// The pixel formats the pre-processor uploads with a GPU copy.
pub(super) fn buffer_format(pixel_format: PixelFormat) -> Option<BufferFormat> {
    match pixel_format {
        PixelFormat::Format8BitYUV => Some(BufferFormat::InterleavedUyvy422),
        PixelFormat::Format8BitBGRA => Some(BufferFormat::Bgra),
        _ => None,
    }
}

/// Uploads frames DeckLink captured into host memory with GPU copies, so the
/// CPU never touches the pixels.
pub(super) struct ZeroCopy {
    device: wgpu::Device,
    copies: VecDeque<wgpu::SubmissionIndex>,
}

impl ZeroCopy {
    pub fn new(device: &wgpu::Device) -> Result<Self, DeckLinkInputError> {
        if !HostMemoryBuffer::is_supported(device) {
            return Err(DeckLinkInputError::ZeroCopyUnsupportedByGpu);
        }
        Ok(Self {
            device: device.clone(),
            copies: VecDeque::new(),
        })
    }

    /// Buffers for one enabled format.
    pub fn allocator(&self) -> Box<dyn FrameAllocator> {
        Box::new(FramePool {
            device: self.device.clone(),
            lent: Arc::new(()),
        })
    }

    pub fn process(
        &mut self,
        pre_processor: &mut FramePreProcessor,
        video_frame: &VideoInputFrame,
        pixel_format: PixelFormat,
    ) -> Option<Arc<wgpu::Texture>> {
        let Some(buffer) = video_frame.buffer::<PoolBuffer>() else {
            warn!("DeckLink frame wasn't captured into a zero-copy buffer");
            return None;
        };
        let Some(format) = buffer_format(pixel_format) else {
            warn!(?pixel_format, "Unsupported zero-copy pixel format");
            return None;
        };
        let layout = BufferLayout {
            format,
            resolution: Resolution {
                width: video_frame.width(),
                height: video_frame.height(),
            },
            bytes_per_row: video_frame.bytes_per_row() as u32,
        };
        let (texture, copy) = pre_processor.process_buffer(buffer.memory.buffer(), layout);
        self.copies.push_back(copy);
        let pending = self.copies.len().saturating_sub(buffer.copies_in_flight());
        if let Some(oldest) = self.copies.drain(..pending).next_back()
            && let Err(err) = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(oldest),
                timeout: Some(COPY_TIMEOUT),
            })
        {
            warn!(%err, "DeckLink frame copy didn't finish, dropping the frame");
            return None;
        }
        Some(texture)
    }
}

struct FramePool {
    device: wgpu::Device,
    /// Referenced by every buffer the pool lent.
    lent: Arc<()>,
}

impl FrameAllocator for FramePool {
    fn allocate(&self, size: usize, row_bytes: usize) -> Option<Box<dyn FrameBuffer>> {
        if !(row_bytes as u32).is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) {
            warn!(row_bytes, "The GPU can't copy rows of this DeckLink format");
            return None;
        }
        if Arc::strong_count(&self.lent) > POOL_SIZE {
            return None;
        }
        let memory = HostMemoryBuffer::new(&self.device, size)
            .inspect_err(|err| warn!(%err, "Failed to allocate a DeckLink capture buffer"))
            .ok()?;
        Some(Box::new(PoolBuffer {
            memory,
            pool: self.lent.clone(),
        }))
    }
}

struct PoolBuffer {
    memory: HostMemoryBuffer,
    pool: Arc<()>,
}

impl PoolBuffer {
    /// DeckLink captures into the `n` buffers its pool lent in turn, whatever
    /// still references their frames, so frame N - (n - 3)'s buffer is written
    /// again from frame N + 2 on: callback N waits for that frame's copy, which
    /// rarely blocks.
    fn copies_in_flight(&self) -> usize {
        let lent = Arc::strong_count(&self.pool) - 1;
        lent.saturating_sub(3)
    }
}

// SAFETY: `HostMemoryBuffer` keeps at least `size` bytes at a fixed address
// until it drops, and only DeckLink writes them.
unsafe impl FrameBuffer for PoolBuffer {
    fn bytes(&self) -> NonNull<u8> {
        self.memory.as_ptr()
    }
}
