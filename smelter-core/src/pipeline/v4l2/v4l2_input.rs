use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use smelter_render::{Frame, FrameData, Framerate, InputId, NvPlanes, Resolution};
use tracing::{Level, debug, error, info, span, warn};

use crate::{pipeline::input::Input, prelude::*, queue::QueueDataReceiver};

use v4l::{
    Format, FourCC,
    frameinterval::FrameIntervalEnum,
    io::traits::CaptureStream,
    prelude::*,
    video::{Capture, capture::Parameters},
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

pub struct V4l2Input {
    should_close: Arc<AtomicBool>,
}

impl V4l2Input {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: V4l2InputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let device_config = V4l2DeviceConfig::initialize(&opts)?;

        let mut stream =
            MmapStream::with_buffers(&device_config.device, v4l::buffer::Type::VideoCapture, 4)
                .map_err(V4l2InputError::IoError)?;
        // the library recommends to skip the first frame
        stream.next().map_err(V4l2InputError::IoError)?;

        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(10);
        let should_close = Arc::new(AtomicBool::new(false));

        let mut state = InputState {
            config: device_config,
            ctx,
            sender: frame_sender,
            should_close: should_close.clone(),
            stream,
        };

        std::thread::Builder::new()
            .name(format!("V4L2 reader thread for input {input_ref}"))
            .spawn(move || {
                let _span = span!(Level::INFO, "V4L2", input_id = input_ref.to_string()).entered();
                state.run();
                info!("Stopping input.");
            })
            .unwrap();

        Ok((
            Input::V4l2(Self { should_close }),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: Some(frame_receiver),
                audio: None,
            },
        ))
    }
}

impl Drop for V4l2Input {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct V4l2DeviceConfig {
    device: Device,
    resolution: Resolution,
    format: V4l2Format,
}

impl V4l2DeviceConfig {
    fn refresh_format(&mut self) -> Result<(), V4l2InputError> {
        let new_format = self.device.format()?;

        self.format = new_format.fourcc.try_into()?;
        self.resolution = Resolution {
            width: new_format.width as usize,
            height: new_format.height as usize,
        };

        info!(
            new_format = new_format.fourcc.str().unwrap_or("<unknown format>"),
            new_resolution = ?self.resolution,
            "Format changed.",
        );

        Ok(())
    }

    fn initialize(opts: &V4l2InputOptions) -> Result<V4l2DeviceConfig, V4l2InputError> {
        let device = Device::with_path(&opts.path)
            .map_err(|e| V4l2InputError::OpeningDeviceFailed(opts.path.clone(), e))?;

        let caps = device.query_caps().map_err(V4l2InputError::IoError)?;
        if !caps
            .capabilities
            .contains(v4l::capability::Flags::VIDEO_CAPTURE)
        {
            return Err(V4l2InputError::CaptureNotSupported);
        }

        let format = Self::try_set_format(&device, opts.format)?;

        let resolution = match opts.resolution {
            Some(resolution) => Self::try_set_resolution(&device, resolution)?,
            None => {
                let format = device.format()?;
                Resolution {
                    width: format.width as usize,
                    height: format.height as usize,
                }
            }
        };

        if let Some(framerate) = opts.framerate {
            Self::try_set_framerate(&device, framerate, &opts.path)?;
        }

        Ok(Self {
            device,
            resolution,
            format,
        })
    }

    fn try_set_format(device: &Device, format: V4l2Format) -> Result<V4l2Format, V4l2InputError> {
        let requested_fourcc = format.into();
        let current_format = device.format()?;

        let negotiated_format = device.set_format(&Format {
            fourcc: format.into(),
            ..current_format
        })?;

        if negotiated_format.fourcc != requested_fourcc {
            warn!(
                requested_format = requested_fourcc.str().unwrap_or("<unknown format>"),
                configured_format = negotiated_format.fourcc.str().unwrap_or("<unknown format>"),
                "Failed to configure requested format.",
            );
        }

        negotiated_format.fourcc.try_into()
    }

    fn try_set_resolution(
        device: &Device,
        resolution: Resolution,
    ) -> Result<Resolution, V4l2InputError> {
        let current_format = device.format()?;

        let negotiated_format = device.set_format(&Format {
            width: resolution.width as u32,
            height: resolution.height as u32,
            ..current_format
        })?;

        let negotiated_resolution = Resolution {
            width: negotiated_format.width as usize,
            height: negotiated_format.height as usize,
        };

        if negotiated_resolution != resolution {
            warn!(
                requested_resolution = ?resolution,
                configured_resolution = ?negotiated_resolution,
                "Failed to configure requested resolution.",
            );
        }

        Ok(resolution)
    }

    fn try_set_framerate(
        device: &Device,
        framerate: Framerate,
        path: &Path,
    ) -> Result<Framerate, V4l2InputError> {
        let current_params = device.params()?;

        if !current_params
            .capabilities
            .contains(v4l::parameters::Capabilities::TIME_PER_FRAME)
        {
            warn!(device_path=?path, "Device does not support setting the framerate.");
        }

        let negotiated_params = device.set_params(&Parameters {
            interval: v4l::Fraction {
                numerator: framerate.den,
                denominator: framerate.num,
            },
            ..current_params
        })?;

        let negotiated_framerate = Framerate {
            num: negotiated_params.interval.denominator,
            den: negotiated_params.interval.numerator,
        };

        if negotiated_framerate.num != framerate.num || negotiated_framerate.den != framerate.den {
            warn!(
                requested_framerate = ?framerate,
                configured_framerate = ?negotiated_framerate,
                "Failed to configure requested resolution.",
            );
        }

        Ok(negotiated_framerate)
    }
}

struct InputState<'a> {
    config: V4l2DeviceConfig,
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
                Err(err) => {
                    warn!(%err, "Cannot receive frame.");
                    continue;
                }
            };

            let V4l2DeviceConfig {
                resolution, format, ..
            } = &self.config;

            let data = match format {
                V4l2Format::Yuyv => {
                    if frame.len() != resolution.width * resolution.height * 2 {
                        if let Err(err) = self.config.refresh_format() {
                            error!(%err, "Error when trying to refresh parameters.");
                            self.send_eos();
                            return;
                        }

                        continue;
                    }

                    FrameData::InterleavedYuyv422(bytes::Bytes::copy_from_slice(frame))
                }
                V4l2Format::Nv12 => {
                    let y_length = resolution.width * resolution.height;
                    if frame.len() != y_length * 3 / 2 {
                        if let Err(err) = self.config.refresh_format() {
                            error!(%err, "Fatal error when trying to refresh parameters.");
                            self.send_eos();
                            return;
                        }

                        continue;
                    }

                    FrameData::Nv12(NvPlanes {
                        y_plane: bytes::Bytes::copy_from_slice(&frame[..y_length]),
                        uv_planes: bytes::Bytes::copy_from_slice(&frame[y_length..]),
                    })
                }
            };

            let frame = Frame {
                pts: self.ctx.queue_sync_point.elapsed() + Duration::from_millis(20),
                data,
                resolution: self.config.resolution,
            };

            if self.sender.send(PipelineEvent::Data(frame)).is_err() {
                debug!("Failed to send video chunk. Channel closed.");
            }
        }
    }

    fn send_eos(&self) {
        if self.sender.send(PipelineEvent::EOS).is_err() {
            debug!("Cannot send EOS. Channel closed.");
        }
    }
}

#[derive(Debug, Clone)]
pub struct V4l2DeviceInfo {
    pub path: Arc<Path>,
    pub name: String,
    pub formats: Vec<V4l2FormatInfo>,
}

#[derive(Debug, Clone)]
pub struct V4l2FormatInfo {
    pub format: V4l2Format,
    pub resolutions: Vec<V4l2ResolutionInfo>,
}

#[derive(Debug, Clone)]
pub struct V4l2ResolutionInfo {
    pub resolution: Resolution,
    pub framerates: Vec<Framerate>,
}

/// List Video for Linux 2 devices that have the `VIDEO_CAPTURE` capability. The devices will also
/// be queried for supported parameters.
pub fn list_v4l2_devices(
    directory_path: impl AsRef<Path>,
) -> Result<Vec<V4l2DeviceInfo>, V4l2InputError> {
    let dir = std::fs::read_dir(directory_path)?;
    let mut devices = Vec::new();

    for entry in dir {
        let entry = entry?;
        let path: Arc<Path> = entry.path().into();

        if let Some(device_info) = read_device(path)? {
            devices.push(device_info);
        }
    }

    Ok(devices)
}

fn read_device(path: Arc<Path>) -> Result<Option<V4l2DeviceInfo>, V4l2InputError> {
    let Ok(device) = v4l::Device::with_path(&path) else {
        return Ok(None);
    };
    let Ok(caps) = device.query_caps() else {
        return Ok(None);
    };

    if !caps
        .capabilities
        .contains(v4l::capability::Flags::VIDEO_CAPTURE)
    {
        return Ok(None);
    }

    let mut formats = Vec::new();

    for format in device.enum_formats()? {
        if let Some(format) = read_format(&device, &path, format)? {
            formats.push(format);
        }
    }

    Ok(Some(V4l2DeviceInfo {
        path,
        name: caps.card,
        formats,
    }))
}

fn read_format(
    device: &Device,
    path: &Path,
    desc: v4l::format::Description,
) -> Result<Option<V4l2FormatInfo>, V4l2InputError> {
    let fourcc = desc.fourcc;
    let Ok(format) = fourcc.try_into() else {
        return Ok(None);
    };

    let mut resolutions = Vec::new();

    for framesize in device.enum_framesizes(fourcc)? {
        for framesize in framesize.size.to_discrete() {
            if let Some(resolution_info) = read_framesize(device, path, fourcc, framesize)? {
                resolutions.push(resolution_info);
            }
        }
    }

    Ok(Some(V4l2FormatInfo {
        format,
        resolutions,
    }))
}

fn read_framesize(
    device: &Device,
    path: &Path,
    fourcc: v4l::FourCC,
    framesize: v4l::framesize::Discrete,
) -> Result<Option<V4l2ResolutionInfo>, V4l2InputError> {
    let mut framerates = Vec::new();

    for framerate in device.enum_frameintervals(fourcc, framesize.width, framesize.height)? {
        match framerate.interval {
            FrameIntervalEnum::Discrete(interval) => framerates.push(Framerate {
                num: interval.denominator,
                den: interval.numerator,
            }),

            FrameIntervalEnum::Stepwise(stepwise) => {
                if let Some(framerates_iter) = read_stepwise_frame_interval(path, stepwise) {
                    framerates.extend(framerates_iter)
                }
            }
        }
    }

    Ok(Some(V4l2ResolutionInfo {
        resolution: Resolution {
            width: framesize.width as usize,
            height: framesize.height as usize,
        },
        framerates,
    }))
}

fn read_stepwise_frame_interval(
    path: &Path,
    stepwise: v4l::frameinterval::Stepwise,
) -> Option<impl Iterator<Item = Framerate>> {
    if stepwise.min.denominator != stepwise.max.denominator
        || stepwise.min.denominator != stepwise.step.denominator
    {
        warn!(device=?path, "Cannot read frame interval.");
        return None;
    }

    Some(
        (stepwise.min.numerator..=stepwise.max.numerator)
            .step_by(stepwise.step.numerator as usize)
            .map(move |interval_num| Framerate {
                num: stepwise.min.denominator,
                den: interval_num,
            }),
    )
}
