use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<V4l2Input> for core::RegisterInputOptions {
    type Error = TypeError;

    #[cfg(target_os = "linux")]
    fn try_from(value: V4l2Input) -> Result<Self, Self::Error> {
        use super::queue_options::new_queue_options;
        let queue_options = new_queue_options(value.required, None)?;

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::V4l2(core::V4l2InputOptions {
                path: value.path,
                format: value.format.into(),
                resolution: value.resolution.map(Into::into),
                framerate: value
                    .framerate
                    .map(smelter_render::Framerate::try_from)
                    .transpose()?,
            }),
            queue_options,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn try_from(_value: V4l2Input) -> Result<Self, Self::Error> {
        Err(TypeError::new(
            "Unsupported platform. \"v4l2\" inputs are only available on Linux.",
        ))
    }
}

impl From<V4l2InputFormat> for core::V4l2Format {
    fn from(value: V4l2InputFormat) -> Self {
        match value {
            V4l2InputFormat::Yuyv => core::V4l2Format::Yuyv,
            V4l2InputFormat::Nv12 => core::V4l2Format::Nv12,
        }
    }
}
