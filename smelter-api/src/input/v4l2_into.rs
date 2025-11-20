use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<V4L2Input> for core::RegisterInputOptions {
    type Error = TypeError;

    #[cfg(target_os = "linux")]
    fn try_from(value: V4L2Input) -> Result<Self, Self::Error> {
        let queue_options = smelter_core::QueueInputOptions {
            required: value.required.unwrap_or(false),
            offset: None,
        };

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::V4L2(core::V4L2InputOptions {
                path: value.path,
                resolution: value.resolution.into(),
                format: value.format.into(),
                framerate: value.framerate.try_into()?,
            }),
            queue_options,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn try_from(_value: DeckLink) -> Result<Self, Self::Error> {
        Err(TypeError::new(
            "The platform that Smelter is running on does not support Video for Linux.",
        ))
    }
}

impl From<Format> for core::V4l2Format {
    fn from(value: Format) -> Self {
        match value {
            Format::Yuyv => core::V4l2Format::Yuyv,
            Format::Nv12 => core::V4l2Format::Nv12,
        }
    }
}
