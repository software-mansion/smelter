use std::time::Duration;

use compositor_render::scene;

use super::util::*;

impl From<Resolution> for compositor_render::Resolution {
    fn from(resolution: Resolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
        }
    }
}

impl From<Resolution> for scene::Size {
    fn from(resolution: Resolution) -> Self {
        Self {
            width: resolution.width as f32,
            height: resolution.height as f32,
        }
    }
}

impl TryFrom<Transition> for scene::Transition {
    type Error = TypeError;

    fn try_from(transition: Transition) -> Result<Self, Self::Error> {
        let interpolation_kind = match transition.easing_function.unwrap_or(EasingFunction::Linear)
        {
            EasingFunction::Linear => scene::InterpolationKind::Linear,
            EasingFunction::Bounce => scene::InterpolationKind::Bounce,
            EasingFunction::CubicBezier { points } => {
                if points[0] < 0.0 || points[0] > 1.0 {
                    return Err(TypeError::new(
                        "Control point x1 has to be in the range [0, 1].",
                    ));
                }
                if points[2] < 0.0 || points[2] > 1.0 {
                    return Err(TypeError::new(
                        "Control point x2 has to be in the range [0, 1].",
                    ));
                }

                scene::InterpolationKind::CubicBezier {
                    x1: points[0],
                    y1: points[1],
                    x2: points[2],
                    y2: points[3],
                }
            }
        };

        Ok(Self {
            duration: Duration::from_secs_f64(transition.duration_ms / 1000.0),
            interpolation_kind,
            reset_on_update: transition.reset_on_update.unwrap_or(false),
        })
    }
}

impl From<HorizontalAlign> for scene::HorizontalAlign {
    fn from(alignment: HorizontalAlign) -> Self {
        match alignment {
            HorizontalAlign::Left => scene::HorizontalAlign::Left,
            HorizontalAlign::Right => scene::HorizontalAlign::Right,
            HorizontalAlign::Justified => scene::HorizontalAlign::Justified,
            HorizontalAlign::Center => scene::HorizontalAlign::Center,
        }
    }
}

impl From<VerticalAlign> for scene::VerticalAlign {
    fn from(alignment: VerticalAlign) -> Self {
        match alignment {
            VerticalAlign::Top => scene::VerticalAlign::Top,
            VerticalAlign::Center => scene::VerticalAlign::Center,
            VerticalAlign::Bottom => scene::VerticalAlign::Bottom,
            VerticalAlign::Justified => scene::VerticalAlign::Justified,
        }
    }
}

impl From<Degree> for scene::Degree {
    fn from(value: Degree) -> Self {
        Self(value.0)
    }
}

impl TryFrom<Framerate> for compositor_render::Framerate {
    type Error = TypeError;

    fn try_from(framerate: Framerate) -> Result<Self, Self::Error> {
        const ERROR_MESSAGE: &str = "Framerate needs to be an unsigned integer or a string in the \"NUM/DEN\" format, where NUM and DEN are both unsigned integers.";
        match framerate {
            Framerate::String(text) => {
                let Some((num_str, den_str)) = text.split_once('/') else {
                    return Err(TypeError::new(ERROR_MESSAGE));
                };
                let num = num_str
                    .parse::<u32>()
                    .or(Err(TypeError::new(ERROR_MESSAGE)))?;
                let den = den_str
                    .parse::<u32>()
                    .or(Err(TypeError::new(ERROR_MESSAGE)))?;
                Ok(compositor_render::Framerate { num, den })
            }
            Framerate::U32(num) => Ok(compositor_render::Framerate { num, den: 1 }),
        }
    }
}

impl TryFrom<AspectRatio> for (u32, u32) {
    type Error = TypeError;

    fn try_from(text: AspectRatio) -> Result<Self, Self::Error> {
        const ERROR_MESSAGE: &str = "Aspect ratio needs to be a string in the \"W:H\" format, where W and H are both unsigned integers.";
        let Some((v1_str, v2_str)) = text.0.split_once(':') else {
            return Err(TypeError::new(ERROR_MESSAGE));
        };
        let v1 = v1_str
            .parse::<u32>()
            .or(Err(TypeError::new(ERROR_MESSAGE)))?;
        let v2 = v2_str
            .parse::<u32>()
            .or(Err(TypeError::new(ERROR_MESSAGE)))?;
        Ok((v1, v2))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TryFrom<PortOrPortRange> for compositor_pipeline::pipeline::rtp::RequestedPort {
    type Error = TypeError;

    fn try_from(value: PortOrPortRange) -> Result<Self, Self::Error> {
        use compositor_pipeline::pipeline::rtp;
        const PORT_CONVERSION_ERROR_MESSAGE: &str = "Port needs to be a number between 1 and 65535 or a string in the \"START:END\" format, where START and END represent a range of ports.";
        match value {
            PortOrPortRange::U16(0) => Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)),
            PortOrPortRange::U16(v) => Ok(rtp::RequestedPort::Exact(v)),
            PortOrPortRange::String(s) => {
                let (start, end) = s
                    .split_once(':')
                    .ok_or(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE))?;

                let start = start
                    .parse::<u16>()
                    .or(Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)))?;
                let end = end
                    .parse::<u16>()
                    .or(Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)))?;

                if start > end {
                    return Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE));
                }

                if start == 0 || end == 0 {
                    return Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE));
                }

                Ok(rtp::RequestedPort::Range((start, end)))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<TransportProtocol> for compositor_pipeline::pipeline::rtp::TransportProtocol {
    fn from(value: TransportProtocol) -> Self {
        use compositor_pipeline::pipeline::rtp;

        match value {
            TransportProtocol::Udp => rtp::TransportProtocol::Udp,
            TransportProtocol::TcpServer => rtp::TransportProtocol::TcpServer,
        }
    }
}
