use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid MPEG-TS packet size: expected 188, got {0}")]
    InvalidPacketSize(usize),

    #[error("Invalid MPEG-TS sync byte: {0:#x}")]
    InvalidSyncByte(u8),

    #[error("Malformed adaptation field")]
    InvalidAdaptationField,

    #[error("Malformed PSI section")]
    InvalidPsi,

    #[error("Unexpected PSI table id: {0:#x}")]
    UnexpectedTableId(u8),

    #[error("PES packet truncated")]
    PesTooShort,

    #[error("Invalid PES packet start code prefix")]
    InvalidPesStartCode,
}
