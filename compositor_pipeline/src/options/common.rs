#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedPort {
    Exact(u16),
    Range((u16, u16)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);
