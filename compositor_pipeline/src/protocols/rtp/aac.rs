#[derive(Debug, thiserror::Error)]
pub enum AacDepayloadingError {
    #[error("Packet too short")]
    PacketTooShort,

    #[error("Interleaving is not supported")]
    InterleavingNotSupported,
}

/// [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
/// [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpAacDepayloaderMode {
    LowBitrate,
    HighBitrate,
}
