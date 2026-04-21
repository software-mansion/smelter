//! Program-Specific Information (PSI) tables and their cross-packet
//! reassembly.
//!
//! Sections can be split across multiple TS packets. [`SectionBuffer`] feeds
//! TS-packet payloads in, watches the `section_length` header field and yields
//! a complete section once enough bytes have arrived.

pub mod pat;
pub mod pmt;

use crate::error::Error;

pub struct SectionBuffer {
    buf: Vec<u8>,
    expected_len: Option<usize>,
    started: bool,
}

impl SectionBuffer {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            expected_len: None,
            started: false,
        }
    }

    /// Feed the payload of a TS packet carrying this section's PID.
    /// Returns a complete section once enough bytes have been accumulated.
    pub fn push(
        &mut self,
        payload: &[u8],
        payload_unit_start: bool,
    ) -> Result<Option<Vec<u8>>, Error> {
        let data = if payload_unit_start {
            // The first byte is `pointer_field`. Bytes between the pointer and
            // the new section belong to a previous section we don't track.
            if payload.is_empty() {
                return Err(Error::InvalidPsi);
            }
            let pointer = payload[0] as usize;
            if 1 + pointer > payload.len() {
                return Err(Error::InvalidPsi);
            }
            self.buf.clear();
            self.expected_len = None;
            self.started = true;
            &payload[1 + pointer..]
        } else if !self.started {
            return Ok(None);
        } else {
            payload
        };

        self.buf.extend_from_slice(data);

        if self.expected_len.is_none() && self.buf.len() >= 3 {
            let section_length =
                ((u16::from(self.buf[1] & 0x0F) << 8) | u16::from(self.buf[2])) as usize;
            self.expected_len = Some(3 + section_length);
        }

        if let Some(len) = self.expected_len
            && self.buf.len() >= len
        {
            let section: Vec<u8> = self.buf.drain(..len).collect();
            // Any trailing bytes belong to a subsequent section (e.g. stuffing
            // of 0xFF). We restart from scratch on the next PUSI anyway.
            self.buf.clear();
            self.expected_len = None;
            self.started = false;
            return Ok(Some(section));
        }
        Ok(None)
    }
}

impl Default for SectionBuffer {
    fn default() -> Self {
        Self::new()
    }
}
