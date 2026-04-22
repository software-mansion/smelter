//! Streaming MPEG-TS demuxer.
//!
//! Feed arbitrary byte chunks into [`Demuxer::push`] and drain
//! [`DemuxerEvent`]s via [`Demuxer::pop_event`]. The demuxer resyncs on the
//! `0x47` sync byte, tracks PAT/PMT to learn about elementary streams, and
//! reassembles PES packets into [`EsPacket`]s carrying PTS/DTS in 90 kHz
//! ticks.

use std::collections::{HashMap, VecDeque};

use bytes::Bytes;

use crate::{
    TS_PACKET_SIZE, TS_SYNC_BYTE,
    error::Error,
    packet::{PAT_PID, TsPacket},
    pes::PesHeader,
    psi::{SectionBuffer, pat::Pat, pmt::Pmt},
    stream_type::StreamType,
};

#[derive(Debug, Clone, Copy)]
pub struct StreamInfo {
    pub pid: u16,
    pub stream_type: StreamType,
}

/// A single elementary-stream access unit reconstructed from PES packets.
///
/// `pts` and `dts` are expressed in 90 kHz ticks (see [`crate::TS_CLOCK_HZ`]).
#[derive(Debug, Clone)]
pub struct EsPacket {
    pub pid: u16,
    pub stream_type: StreamType,
    pub pts: Option<u64>,
    pub dts: Option<u64>,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub enum DemuxerEvent {
    /// Emitted whenever a new PMT is parsed. Replaces any previous list.
    ProgramDiscovered(Vec<StreamInfo>),
    EsPacket(EsPacket),
}

pub struct Demuxer {
    /// Rolling buffer of raw bytes fed via [`Demuxer::push`].
    buffer: Vec<u8>,
    pat_section: SectionBuffer,
    pmt_pid: Option<u16>,
    pmt_section: SectionBuffer,
    streams: HashMap<u16, StreamType>,
    pes_assemblers: HashMap<u16, PesAssembler>,
    events: VecDeque<DemuxerEvent>,
}

struct PesAssembler {
    buf: Vec<u8>,
    /// `6 + PES_packet_length` when known, `0` for video streams that signal
    /// unbounded length — finalised on the next PUSI or on flush.
    expected_len: usize,
}

impl Demuxer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(TS_PACKET_SIZE * 16),
            pat_section: SectionBuffer::new(),
            pmt_pid: None,
            pmt_section: SectionBuffer::new(),
            streams: HashMap::new(),
            pes_assemblers: HashMap::new(),
            events: VecDeque::new(),
        }
    }

    /// Append bytes and process as many complete TS packets as possible.
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.process_buffer();
    }

    pub fn pop_event(&mut self) -> Option<DemuxerEvent> {
        self.events.pop_front()
    }

    /// Finalise any pending PES assemblers with unbounded length. Call this
    /// once the upstream stream has ended.
    pub fn flush(&mut self) {
        let pending: Vec<(u16, PesAssembler)> = self.pes_assemblers.drain().collect();
        for (pid, assembler) in pending {
            if !assembler.buf.is_empty() {
                self.finalize_pes(pid, assembler);
            }
        }
    }

    fn process_buffer(&mut self) {
        let mut cursor = 0;
        while self.buffer.len() >= cursor + TS_PACKET_SIZE {
            if self.buffer[cursor] != TS_SYNC_BYTE {
                cursor += 1;
                continue;
            }
            let mut packet = [0u8; TS_PACKET_SIZE];
            packet.copy_from_slice(&self.buffer[cursor..cursor + TS_PACKET_SIZE]);
            // Ignore individual packet errors — we keep scanning the stream.
            let _ = self.process_packet(&packet);
            cursor += TS_PACKET_SIZE;
        }
        if cursor > 0 {
            self.buffer.drain(..cursor);
        }
    }

    fn process_packet(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let pkt = TsPacket::parse(bytes)?;
        if pkt.header.transport_error
            || pkt.header.scrambling_control != 0
            || !pkt.header.has_payload
        {
            return Ok(());
        }

        let pid = pkt.header.pid;
        if pid == PAT_PID {
            if let Some(section) = self
                .pat_section
                .push(pkt.payload, pkt.header.payload_unit_start)?
            {
                let pat = Pat::parse(&section)?;
                // Pick the first program that carries a PMT.
                if let Some(entry) = pat.programs.iter().find(|p| p.program_number != 0) {
                    self.pmt_pid = Some(entry.pid);
                }
            }
            return Ok(());
        }

        if Some(pid) == self.pmt_pid {
            if let Some(section) = self
                .pmt_section
                .push(pkt.payload, pkt.header.payload_unit_start)?
            {
                let pmt = Pmt::parse(&section)?;
                self.streams.clear();
                let infos: Vec<StreamInfo> = pmt
                    .streams
                    .iter()
                    .map(|s| {
                        self.streams.insert(s.pid, s.stream_type);
                        StreamInfo {
                            pid: s.pid,
                            stream_type: s.stream_type,
                        }
                    })
                    .collect();
                self.events
                    .push_back(DemuxerEvent::ProgramDiscovered(infos));
            }
            return Ok(());
        }

        if self.streams.contains_key(&pid) {
            self.handle_pes(pid, pkt.header.payload_unit_start, pkt.payload);
        }
        Ok(())
    }

    fn handle_pes(&mut self, pid: u16, payload_unit_start: bool, payload: &[u8]) {
        if payload_unit_start {
            // Finalise the previous PES on this PID (common for video where
            // declared length is 0).
            if let Some(prev) = self.pes_assemblers.remove(&pid)
                && !prev.buf.is_empty()
            {
                self.finalize_pes(pid, prev);
            }

            if payload.len() < 6 {
                return;
            }
            let declared = u16::from_be_bytes([payload[4], payload[5]]) as usize;
            let expected_len = if declared == 0 { 0 } else { 6 + declared };

            let mut buf = Vec::with_capacity(expected_len.max(payload.len()));
            buf.extend_from_slice(payload);
            self.pes_assemblers
                .insert(pid, PesAssembler { buf, expected_len });
        } else if let Some(assembler) = self.pes_assemblers.get_mut(&pid) {
            assembler.buf.extend_from_slice(payload);
        }

        if let Some(assembler) = self.pes_assemblers.get(&pid)
            && assembler.expected_len != 0
            && assembler.buf.len() >= assembler.expected_len
        {
            let assembler = self.pes_assemblers.remove(&pid).unwrap();
            self.finalize_pes(pid, assembler);
        }
    }

    fn finalize_pes(&mut self, pid: u16, assembler: PesAssembler) {
        let Ok(header) = PesHeader::parse(&assembler.buf) else {
            return;
        };
        let Some(stream_type) = self.streams.get(&pid).copied() else {
            return;
        };
        if assembler.buf.len() < header.payload_offset {
            return;
        }
        let payload = &assembler.buf[header.payload_offset..];

        if matches!(stream_type, StreamType::AacAdts) {
            self.emit_adts_frames(pid, stream_type, header.pts, header.dts, payload);
            return;
        }

        self.events.push_back(DemuxerEvent::EsPacket(EsPacket {
            pid,
            stream_type,
            pts: header.pts,
            dts: header.dts,
            data: Bytes::copy_from_slice(payload),
        }));
    }

    /// Split an ADTS-framed AAC PES payload into individual access units,
    /// mirroring FFmpeg's `mpegts` demuxer: one `EsPacket` per frame with PTS
    /// interpolated from the ADTS sampling frequency. Falls back to emitting
    /// the whole payload as a single packet if the header is malformed.
    fn emit_adts_frames(
        &mut self,
        pid: u16,
        stream_type: StreamType,
        base_pts: Option<u64>,
        base_dts: Option<u64>,
        payload: &[u8],
    ) {
        let mut offset = 0;
        let mut frame_index: u64 = 0;
        let mut emitted_any = false;

        while offset + 7 <= payload.len() {
            let b = &payload[offset..];
            // ADTS sync word: 0xFFF.
            if b[0] != 0xFF || (b[1] & 0xF0) != 0xF0 {
                break;
            }
            let sampling_idx = ((b[2] >> 2) & 0x0F) as usize;
            let frame_length =
                (((b[3] as usize) & 0x03) << 11) | ((b[4] as usize) << 3) | ((b[5] as usize) >> 5);
            let blocks = ((b[6] & 0x03) as u64) + 1;

            if frame_length < 7 || offset + frame_length > payload.len() {
                break;
            }
            let Some(sample_rate) = ADTS_SAMPLING_FREQUENCIES.get(sampling_idx).copied() else {
                break;
            };
            if sample_rate == 0 {
                break;
            }

            // Each AAC raw data block = 1024 samples.
            let ticks_per_frame = (1024 * blocks * crate::TS_CLOCK_HZ) / sample_rate as u64;
            let pts_offset = frame_index * ticks_per_frame;
            let pts = base_pts.map(|p| p + pts_offset);
            let dts = base_dts.map(|d| d + pts_offset);

            self.events.push_back(DemuxerEvent::EsPacket(EsPacket {
                pid,
                stream_type,
                pts,
                dts,
                data: Bytes::copy_from_slice(&payload[offset..offset + frame_length]),
            }));

            offset += frame_length;
            frame_index += 1;
            emitted_any = true;
        }

        if !emitted_any {
            self.events.push_back(DemuxerEvent::EsPacket(EsPacket {
                pid,
                stream_type,
                pts: base_pts,
                dts: base_dts,
                data: Bytes::copy_from_slice(payload),
            }));
        }
    }
}

/// ISO/IEC 14496-3 Table 1.16 — AAC sampling-frequency index.
const ADTS_SAMPLING_FREQUENCIES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

impl Default for Demuxer {
    fn default() -> Self {
        Self::new()
    }
}
