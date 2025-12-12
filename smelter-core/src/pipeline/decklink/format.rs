use decklink::{DetectedVideoInputFormatFlags, DisplayModeType, PixelFormat};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct Format {
    pub display_mode: DisplayModeType,
    pub bit_depth: BitDepth,
    pub colorspace: Colorspace,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum BitDepth {
    Depth8Bit,
    Depth10Bit,
    Depth12Bit,
    Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum Colorspace {
    YCbCr422,
    RGB444,
    Unknown,
}
impl Format {
    pub fn new(display_mode: DisplayModeType, pixel_format: PixelFormat) -> Self {
        let (bit_depth, colorspace) = match pixel_format {
            PixelFormat::Format8BitBGRA => (BitDepth::Depth8Bit, Colorspace::RGB444),
            PixelFormat::Format8BitARGB => (BitDepth::Depth8Bit, Colorspace::RGB444),
            PixelFormat::Format10BitRGB => (BitDepth::Depth10Bit, Colorspace::RGB444),
            PixelFormat::Format10BitRGBX => (BitDepth::Depth10Bit, Colorspace::RGB444),
            PixelFormat::Format10BitRGBXLE => (BitDepth::Depth10Bit, Colorspace::RGB444),
            PixelFormat::Format12BitRGB => (BitDepth::Depth12Bit, Colorspace::RGB444),
            PixelFormat::Format12BitRGBLE => (BitDepth::Depth12Bit, Colorspace::RGB444),
            PixelFormat::Format8BitYUV => (BitDepth::Depth8Bit, Colorspace::YCbCr422),
            PixelFormat::Format10BitYUV => (BitDepth::Depth10Bit, Colorspace::YCbCr422),
            PixelFormat::Format10BitYUVA => (BitDepth::Depth10Bit, Colorspace::YCbCr422),
            _ => (BitDepth::Unknown, Colorspace::Unknown),
        };
        Self {
            display_mode,
            bit_depth,
            colorspace,
        }
    }

    pub fn from_mode_change(
        display_mode: DisplayModeType,
        flags: DetectedVideoInputFormatFlags,
    ) -> Self {
        let bit_depth = if flags.bit_depth_8 {
            BitDepth::Depth8Bit
        } else if flags.bit_depth_10 {
            BitDepth::Depth10Bit
        } else if flags.bit_depth_12 {
            BitDepth::Depth12Bit
        } else {
            BitDepth::Unknown
        };

        let colorspace = if flags.format_y_cb_cr_422 {
            Colorspace::YCbCr422
        } else if flags.format_rgb_444 {
            Colorspace::RGB444
        } else {
            Colorspace::Unknown
        };
        Self {
            display_mode,
            bit_depth,
            colorspace,
        }
    }
}
