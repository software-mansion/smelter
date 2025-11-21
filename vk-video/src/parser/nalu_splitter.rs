use std::{iter, mem, sync::LazyLock};

use bytes::{BufMut, BytesMut};
use memchr::memmem::Finder;

#[derive(Debug, Default)]
pub(crate) struct NALUSplitter {
    buffer: BytesMut,
    // should only by used when flushing
    last_pts: Option<u64>,
}

/// Find index of start of AnnexB prefix
fn find_next_prefix(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&[0, 0, 1]));

    let start_index = FINDER.find(buf)?;
    if start_index > 0 && buf[start_index - 1] == 0 {
        Some(start_index - 1)
    } else {
        Some(start_index)
    }
}

impl NALUSplitter {
    pub(crate) fn push(
        &mut self,
        bytestream: &[u8],
        pts: Option<u64>,
    ) -> Vec<(BytesMut, Option<u64>)> {
        self.buffer.put(bytestream);
        self.last_pts = pts;
        iter::from_fn(|| self.get_next_nalu(false).map(|nalu| (nalu, pts))).collect()
    }

    pub(crate) fn flush(&mut self) -> Vec<(BytesMut, Option<u64>)> {
        iter::from_fn(|| self.get_next_nalu(true).map(|nalu| (nalu, self.last_pts))).collect()
    }

    fn get_next_nalu(&mut self, force: bool) -> Option<BytesMut> {
        let first_prefix = find_next_prefix(&self.buffer)?;
        if first_prefix != 0 {
            // drop because does not start with prefix
            let _ = self.buffer.split_to(first_prefix);
        }

        // We know that first 3 element exist because we found prefix
        // in previous step.
        match find_next_prefix(&self.buffer[3..]) {
            Some(prefix_index) => Some(self.buffer.split_to(prefix_index + 3)),
            None => match force && !self.buffer.is_empty() {
                true => Some(mem::take(&mut self.buffer)),
                false => None,
            },
        }
    }
}
