/// Pixel dimensions used by hardware video backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

/// Rational frame rate used by hardware video backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoFramerate {
    pub num: u32,
    pub den: u32,
}

impl VideoFramerate {
    pub fn get_interval_duration(self) -> std::time::Duration {
        std::time::Duration::from_nanos(
            1_000_000_000u64 * self.den as u64 / self.num as u64,
        )
    }
}
