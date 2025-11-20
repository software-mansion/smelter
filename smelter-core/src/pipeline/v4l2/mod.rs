use std::sync::{Arc, atomic::AtomicBool};

use smelter_render::{Frame, InputId, Resolution};
use tracing::Level;

use crate::{
    InputInitInfo, PipelineCtx, PipelineEvent, Ref,
    error::InputInitError,
    pipeline::input::Input,
    prelude::{V4L2InputOptions, V4l2Format, V4l2InputError},
    queue::QueueDataReceiver,
};

use v4l::{
    Format, FourCC,
    io::traits::CaptureStream,
    parameters::Capabilities,
    prelude::*,
    video::{Capture, capture::parameters::Modes},
};

impl From<V4l2Format> for FourCC {
    fn from(value: V4l2Format) -> Self {
        match value {
            V4l2Format::Yuyv => FourCC::new(b"YUYV"),
            V4l2Format::Nv12 => FourCC::new(b"NV12"),
        }
    }
}

impl TryFrom<FourCC> for V4l2Format {
    type Error = V4l2InputError;

    fn try_from(fourcc: FourCC) -> Result<Self, Self::Error> {
        match &fourcc.repr {
            b"YUYV" => Ok(V4l2Format::Yuyv),
            b"NV12" => Ok(V4l2Format::Nv12),
            format => Err(V4l2InputError::UnsupportedFormat(
                String::from_utf8_lossy(format).to_string(),
            )),
        }
    }
}

pub struct V4L2Input {
    should_close: Arc<AtomicBool>,
}

impl V4L2Input {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: V4L2InputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let device = Device::with_path(&opts.path)
            .map_err(|e| V4l2InputError::OpeningDeviceFailed(opts.path.clone(), e))?;

        let caps = device.query_caps().map_err(V4l2InputError::IoError)?;

        if !caps
            .capabilities
            .contains(v4l::capability::Flags::VIDEO_CAPTURE)
        {
            return Err(V4l2InputError::CaptureNotSupported.into());
        }

        let requested_fourcc = opts.format.into();

        let format = device.format().map_err(V4l2InputError::IoError)?;

        let format = device
            .set_format(&Format {
                width: opts.resolution.width as u32,
                height: opts.resolution.height as u32,
                fourcc: requested_fourcc,
                ..format
            })
            .map_err(V4l2InputError::IoError)?;

        if format.fourcc != requested_fourcc {
            tracing::warn!(
                "Tried to negotiate format {}, but the capture device switched to {}.",
                requested_fourcc.str().unwrap_or("<unknown format>"),
                format.fourcc.str().unwrap_or("<unknown format>")
            );
        }

        let negotiated_format = format.fourcc.try_into()?;

        let negotiated_resolution = Resolution {
            width: format.width as usize,
            height: format.height as usize,
        };

        if opts.resolution != negotiated_resolution {
            tracing::warn!(
                "Tried to negotiate resolution {}x{}, but the capture device switched to {}x{}",
                opts.resolution.width,
                opts.resolution.height,
                negotiated_resolution.width,
                negotiated_resolution.height,
            );
        }

        let negotiated_parameters = device
            .set_params(&v4l::video::capture::Parameters {
                capabilities: Capabilities::TIME_PER_FRAME,
                modes: Modes::empty(),
                interval: v4l::Fraction {
                    numerator: opts.framerate.den,
                    denominator: opts.framerate.num,
                },
            })
            .map_err(V4l2InputError::IoError)?;

        if opts.framerate.num != negotiated_parameters.interval.denominator
            || opts.framerate.den != negotiated_parameters.interval.numerator
        {
            tracing::warn!(
                "Tried to negotiate framerate {}/{}, but the capture device switched to {}/{}",
                opts.framerate.num,
                opts.framerate.den,
                negotiated_parameters.interval.denominator,
                negotiated_parameters.interval.numerator,
            );
        }
        let mut stream = MmapStream::with_buffers(&device, v4l::buffer::Type::VideoCapture, 4)
            .map_err(V4l2InputError::IoError)?;

        stream.next().map_err(V4l2InputError::IoError)?;

        let (tx, rx) = crossbeam_channel::bounded(10);
        let should_close = Arc::new(AtomicBool::new(false));

        let mut state = InputState {
            resolution: negotiated_resolution,
            format: negotiated_format,
            v4l_device: device,
            ctx,
            sender: tx,
            should_close: should_close.clone(),
            stream,
        };

        std::thread::Builder::new()
            .name(format!("V4L2 reader thread for input {input_ref}"))
            .spawn(move || {
                let _span =
                    tracing::span!(Level::INFO, "V4L2", input_id = input_ref.to_string()).entered();
                state.run()
            })
            .unwrap();

        Ok((
            Input::V4L2(Self { should_close }),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: Some(rx),
                audio: None,
            },
        ))
    }
}

impl Drop for V4L2Input {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct InputState<'a> {
    v4l_device: Device,
    resolution: Resolution,
    format: V4l2Format,
    ctx: Arc<PipelineCtx>,
    should_close: Arc<AtomicBool>,
    sender: crossbeam_channel::Sender<PipelineEvent<Frame>>,
    stream: v4l::io::mmap::Stream<'a>,
}

impl InputState<'_> {
    fn run(&mut self) {
        loop {
            if self.should_close.load(std::sync::atomic::Ordering::Relaxed) {
                self.send_eos();
                return;
            }

            let frame = match self.stream.next() {
                Ok((frame, _)) => frame,
                Err(e) => {
                    tracing::warn!("Cannot receive frame: {e}");
                    continue;
                }
            };

            let data = match self.format {
                V4l2Format::Yuyv => {
                    if frame.len() != self.resolution.width * self.resolution.height * 2 {
                        if let Err(e) = self.refresh_format() {
                            tracing::error!("Fatal error when trying to refresh parameters: {e}");
                            self.send_eos();
                            return;
                        }

                        continue;
                    }

                    smelter_render::FrameData::InterleavedYuyv422(bytes::Bytes::copy_from_slice(
                        frame,
                    ))
                }
                V4l2Format::Nv12 => {
                    let y_length = self.resolution.width * self.resolution.height;

                    if frame.len() != y_length * 3 / 2 {
                        if let Err(e) = self.refresh_format() {
                            tracing::error!("Fatal error when trying to refresh parameters: {e}");
                            self.send_eos();
                            return;
                        }

                        continue;
                    }

                    smelter_render::FrameData::Nv12(smelter_render::NvPlanes {
                        y_plane: bytes::Bytes::copy_from_slice(&frame[..y_length]),
                        uv_planes: bytes::Bytes::copy_from_slice(&frame[y_length..]),
                    })
                }
            };

            let frame = Frame {
                pts: self.ctx.queue_sync_point.elapsed() + std::time::Duration::from_millis(20),
                data,
                resolution: self.resolution,
            };

            if let Err(e) = self.sender.send(PipelineEvent::Data(frame)) {
                tracing::debug!("Failed to send video chunk: {e}");
            }
        }
    }

    fn send_eos(&self) {
        if let Err(e) = self.sender.send(PipelineEvent::EOS) {
            tracing::warn!("Cannot send EOS: {e}");
        }
    }

    fn refresh_format(&mut self) -> Result<(), V4l2InputError> {
        let new_format = self.v4l_device.format()?;

        self.format = new_format.fourcc.try_into()?;
        self.resolution = Resolution {
            width: new_format.width as usize,
            height: new_format.height as usize,
        };

        tracing::info!(
            "Format changed to {}, {}x{}",
            new_format.fourcc.str().unwrap_or("<unknown format>"),
            self.resolution.width,
            self.resolution.height
        );

        Ok(())
    }
}
