//! MPEG-2 CRC-32 used by PSI sections (ISO/IEC 13818-1 Annex B).
//!
//! Polynomial `0x04C11DB7`, initial value `0xFFFFFFFF`, no input/output
//! reflection, no final XOR.

pub fn mpeg2_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            if crc & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
