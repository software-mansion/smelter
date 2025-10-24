use std::sync::LazyLock;

use bytes::{BufMut, BytesMut};
use memchr::memmem::Finder;

#[derive(Debug, Default)]
pub struct NALUSplitter {
    buffer: BytesMut,
    pts: Option<u64>,
    previous_search_end: usize,
}

fn find_start_of_next_nalu(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&[0, 0, 1]));

    if buf.len() < 4 {
        return None;
    };

    // If the start code is at the beginning of nalu, we need to skip it, because this means we
    // would give the parser only the start code, without a nal unit before it.
    //
    // The code can be either 0, 0, 0, 1 or 0, 0, 1.
    // We're looking for the sequence 0, 0, 1. If we find it starting at the second byte of input,
    // this could mean either that it's the longer 0, 0, 0, 1 code, or that there is one byte of
    // the previous nalu and the shorter code. This is what is checked here.
    if buf[0] != 0 && buf[1..4] == [0, 0, 1] {
        return Some(5);
    }

    FINDER.find(&buf[2..]).map(|i| i + 5)
}

impl NALUSplitter {
    pub fn push(
        &mut self,
        bytestream: &[u8],
        pts: Option<u64>,
    ) -> Vec<(Vec<u8>, Option<u64>)> {
        let mut output_pts = if self.buffer.is_empty() {
            pts
        } else {
            self.pts
        };

        self.buffer.put(bytestream);
        let mut result = Vec::new();

        while let Some(i) = find_start_of_next_nalu(&self.buffer[self.previous_search_end..]) {
            let nalu = self.buffer.split_to(self.previous_search_end + i);
            self.previous_search_end = 0;
            result.push((nalu.to_vec(), output_pts));
            output_pts = pts;
        }

        // This will cause the whole start code to be reprocessed when the beginning of the next start code
        // is at the end of current buffer.
        self.previous_search_end = self.buffer.len().saturating_sub(3);

        self.pts = pts;

        result
    }
}
