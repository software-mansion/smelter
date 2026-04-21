//! Streaming MPEG-TS muxer.
//!
//! Configure the output streams via [`MuxerConfig`], then feed encoded
//! elementary-stream access units into [`Muxer::write`] to receive 188-byte
//! MPEG-TS packets ready to push into a transport (e.g. SRT, UDP).
//!
//! PAT/PMT tables are emitted before the first packet and re-emitted in front
//! of every video keyframe so late joiners can decode the stream.

use std::collections::HashMap;

use bytes::{Bytes, BytesMut};

use crate::{
    TS_PACKET_SIZE, TS_SYNC_BYTE, crc::mpeg2_crc32, packet::PAT_PID, stream_type::StreamType,
};

/// Default PID used by the muxer for the PMT section.
pub const DEFAULT_PMT_PID: u16 = 0x1000;
/// Default PID used by the muxer for the video elementary stream.
pub const DEFAULT_VIDEO_PID: u16 = 0x0100;
/// Default PID used by the muxer for the audio elementary stream.
pub const DEFAULT_AUDIO_PID: u16 = 0x0101;

const TS_PAYLOAD_SIZE: usize = 184;

#[derive(Debug, Clone, Copy)]
pub struct MuxerStream {
    pub pid: u16,
    pub stream_type: StreamType,
}

#[derive(Debug, Clone)]
pub struct MuxerConfig {
    pub transport_stream_id: u16,
    pub program_number: u16,
    pub pmt_pid: u16,
    /// PID that carries PCR in its adaptation field. Must match one of the
    /// configured [`MuxerStream`]s (conventionally the video PID).
    pub pcr_pid: u16,
    pub streams: Vec<MuxerStream>,
}

impl MuxerConfig {
    /// Convenience constructor for the common H.264 + AAC case.
    pub fn h264_aac() -> Self {
        Self {
            transport_stream_id: 1,
            program_number: 1,
            pmt_pid: DEFAULT_PMT_PID,
            pcr_pid: DEFAULT_VIDEO_PID,
            streams: vec![
                MuxerStream {
                    pid: DEFAULT_VIDEO_PID,
                    stream_type: StreamType::H264,
                },
                MuxerStream {
                    pid: DEFAULT_AUDIO_PID,
                    stream_type: StreamType::AacAdts,
                },
            ],
        }
    }
}

/// A single encoded access unit passed to [`Muxer::write`].
#[derive(Debug, Clone)]
pub struct MuxerInput<'a> {
    pub pid: u16,
    /// 90 kHz PTS (see [`crate::TS_CLOCK_HZ`]).
    pub pts: Option<u64>,
    /// 90 kHz DTS. If `None` and `pts` is `Some`, only PTS is signalled.
    pub dts: Option<u64>,
    /// Set `true` for video keyframes. Triggers PAT/PMT re-emission plus the
    /// `random_access_indicator` in the adaptation field.
    pub is_keyframe: bool,
    pub data: &'a [u8],
}

pub struct Muxer {
    config: MuxerConfig,
    continuity: HashMap<u16, u8>,
    emitted_psi: bool,
}

impl Muxer {
    pub fn new(config: MuxerConfig) -> Self {
        assert!(
            config.streams.iter().any(|s| s.pid == config.pcr_pid),
            "pcr_pid must reference a configured stream",
        );
        Self {
            config,
            continuity: HashMap::new(),
            emitted_psi: false,
        }
    }

    /// Explicitly emit PAT+PMT packets. Normally unnecessary — [`Muxer::write`]
    /// handles it — but exposed for callers that want to front-load PSI before
    /// the first media sample.
    pub fn write_psi(&mut self) -> Bytes {
        let mut out = BytesMut::new();
        self.emit_psi(&mut out);
        out.freeze()
    }

    /// Mux one access unit. Returns the resulting TS packets concatenated in
    /// 188-byte units, ready to be handed to a transport.
    pub fn write(&mut self, input: MuxerInput<'_>) -> Bytes {
        let mut out = BytesMut::new();
        if !self.emitted_psi || input.is_keyframe {
            self.emit_psi(&mut out);
        }
        let stream = self
            .config
            .streams
            .iter()
            .find(|s| s.pid == input.pid)
            .copied()
            .unwrap_or_else(|| panic!("Muxer::write called with unknown pid {}", input.pid));

        let pes = build_pes(stream.stream_type, input.pts, input.dts, input.data);
        let is_pcr = input.pid == self.config.pcr_pid;
        self.packetize_pes(
            input.pid,
            &pes,
            input.pts.or(input.dts),
            input.is_keyframe,
            is_pcr,
            &mut out,
        );
        out.freeze()
    }

    fn next_cc(&mut self, pid: u16) -> u8 {
        let slot = self.continuity.entry(pid).or_insert(0);
        let current = *slot;
        *slot = (*slot + 1) & 0x0F;
        current
    }

    fn emit_psi(&mut self, out: &mut BytesMut) {
        let pat = build_pat_section(
            self.config.transport_stream_id,
            self.config.program_number,
            self.config.pmt_pid,
        );
        let pmt = build_pmt_section(
            self.config.program_number,
            self.config.pcr_pid,
            &self.config.streams,
        );
        let pat_cc = self.next_cc(PAT_PID);
        let pmt_cc = self.next_cc(self.config.pmt_pid);
        out.extend_from_slice(&build_psi_ts(PAT_PID, pat_cc, &pat));
        out.extend_from_slice(&build_psi_ts(self.config.pmt_pid, pmt_cc, &pmt));
        self.emitted_psi = true;
    }

    fn packetize_pes(
        &mut self,
        pid: u16,
        pes: &[u8],
        pcr_ts: Option<u64>,
        is_keyframe: bool,
        is_pcr_pid: bool,
        out: &mut BytesMut,
    ) {
        let mut offset = 0;
        let mut first = true;
        while offset < pes.len() {
            let remaining = pes.len() - offset;
            let cc = self.next_cc(pid);

            let needs_pcr = first && is_pcr_pid;
            let key_flag = first && is_keyframe;
            let must_have_flags = needs_pcr || key_flag;
            let need_af = must_have_flags || remaining < TS_PAYLOAD_SIZE;

            let mut pkt = [0u8; TS_PACKET_SIZE];
            pkt[0] = TS_SYNC_BYTE;
            pkt[1] = if first { 0x40 } else { 0x00 } | ((pid >> 8) as u8 & 0x1F);
            pkt[2] = (pid & 0xFF) as u8;

            if need_af {
                let fields_len = if must_have_flags {
                    1 + if needs_pcr { 6 } else { 0 }
                } else {
                    0
                };
                let max_payload = TS_PAYLOAD_SIZE - 1 - fields_len;
                let (payload_len, af_length) = if remaining <= max_payload {
                    let p = remaining;
                    (p, TS_PAYLOAD_SIZE - 1 - p)
                } else {
                    (max_payload, fields_len)
                };

                pkt[3] = 0x30 | (cc & 0x0F);
                pkt[4] = af_length as u8;

                let mut pos = 5;
                if af_length > 0 {
                    let mut flags = 0u8;
                    if key_flag {
                        flags |= 0x40;
                    }
                    if needs_pcr {
                        flags |= 0x10;
                    }
                    pkt[pos] = flags;
                    pos += 1;
                    if needs_pcr {
                        let pcr_base = pcr_ts.unwrap_or(0) & 0x1_FFFF_FFFF;
                        pkt[pos] = ((pcr_base >> 25) & 0xFF) as u8;
                        pkt[pos + 1] = ((pcr_base >> 17) & 0xFF) as u8;
                        pkt[pos + 2] = ((pcr_base >> 9) & 0xFF) as u8;
                        pkt[pos + 3] = ((pcr_base >> 1) & 0xFF) as u8;
                        pkt[pos + 4] = (((pcr_base & 0x01) as u8) << 7) | 0x7E;
                        pkt[pos + 5] = 0x00;
                        pos += 6;
                    }
                    while pos < 5 + af_length {
                        pkt[pos] = 0xFF;
                        pos += 1;
                    }
                }

                let payload_start = 5 + af_length;
                pkt[payload_start..payload_start + payload_len]
                    .copy_from_slice(&pes[offset..offset + payload_len]);
                offset += payload_len;
            } else {
                pkt[3] = 0x10 | (cc & 0x0F);
                pkt[4..4 + TS_PAYLOAD_SIZE].copy_from_slice(&pes[offset..offset + TS_PAYLOAD_SIZE]);
                offset += TS_PAYLOAD_SIZE;
            }

            out.extend_from_slice(&pkt);
            first = false;
        }
    }
}

fn pes_stream_id(stream_type: StreamType) -> u8 {
    if stream_type.is_video() { 0xE0 } else { 0xC0 }
}

fn build_pes(stream_type: StreamType, pts: Option<u64>, dts: Option<u64>, data: &[u8]) -> Vec<u8> {
    let stream_id = pes_stream_id(stream_type);
    let (pts_dts_flags, header_data_len) = match (pts, dts) {
        (Some(_), Some(_)) => (0b11u8, 10usize),
        (Some(_), None) => (0b10u8, 5usize),
        _ => (0b00u8, 0usize),
    };
    let header_len = 9 + header_data_len;
    let total = header_len + data.len();

    // `PES_packet_length` is 16 bits. For unbounded video streams the spec
    // allows signalling 0 (only valid on video PES).
    let pes_length_field = total - 6;
    let declared = if pes_length_field > u16::MAX as usize {
        if !stream_type.is_video() {
            panic!(
                "PES packet too large ({pes_length_field} bytes) for non-video stream {stream_type:?}"
            );
        }
        0u16
    } else {
        pes_length_field as u16
    };

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&[0x00, 0x00, 0x01, stream_id]);
    out.extend_from_slice(&declared.to_be_bytes());
    // Marker bits '10', no scrambling, no priority, alignment indicator,
    // no copyright, no original.
    out.push(0x84);
    out.push((pts_dts_flags << 6) & 0xC0);
    out.push(header_data_len as u8);
    match (pts, dts) {
        (Some(p), Some(d)) => {
            write_timestamp(&mut out, 0b0011, p);
            write_timestamp(&mut out, 0b0001, d);
        }
        (Some(p), None) => {
            write_timestamp(&mut out, 0b0010, p);
        }
        _ => {}
    }
    out.extend_from_slice(data);
    out
}

fn write_timestamp(buf: &mut Vec<u8>, prefix: u8, ts: u64) {
    let ts = ts & 0x1_FFFF_FFFF;
    buf.push((prefix << 4) | ((((ts >> 30) as u8) & 0x07) << 1) | 0x01);
    buf.push(((ts >> 22) & 0xFF) as u8);
    buf.push(((((ts >> 15) & 0x7F) as u8) << 1) | 0x01);
    buf.push(((ts >> 7) & 0xFF) as u8);
    buf.push(((((ts) & 0x7F) as u8) << 1) | 0x01);
}

fn build_psi_ts(pid: u16, cc: u8, section: &[u8]) -> [u8; TS_PACKET_SIZE] {
    assert!(
        section.len() < TS_PAYLOAD_SIZE,
        "PSI section too large for a single TS packet"
    );
    let mut pkt = [0xFFu8; TS_PACKET_SIZE];
    pkt[0] = TS_SYNC_BYTE;
    pkt[1] = 0x40 | ((pid >> 8) as u8 & 0x1F);
    pkt[2] = (pid & 0xFF) as u8;
    pkt[3] = 0x10 | (cc & 0x0F);
    pkt[4] = 0; // pointer_field
    pkt[5..5 + section.len()].copy_from_slice(section);
    pkt
}

fn build_pat_section(transport_stream_id: u16, program_number: u16, pmt_pid: u16) -> Vec<u8> {
    // table tail (after section_length field) + 4-byte program entry + 4-byte CRC.
    let section_length: u16 = 5 + 4 + 4;
    let mut section = Vec::with_capacity(3 + section_length as usize);
    section.push(0x00); // table_id = PAT
    section.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
    section.push((section_length & 0xFF) as u8);
    section.extend_from_slice(&transport_stream_id.to_be_bytes());
    section.push(0xC1); // reserved | version=0 | current_next=1
    section.push(0x00); // section_number
    section.push(0x00); // last_section_number
    section.extend_from_slice(&program_number.to_be_bytes());
    section.extend_from_slice(&(0xE000u16 | (pmt_pid & 0x1FFF)).to_be_bytes());
    let crc = mpeg2_crc32(&section);
    section.extend_from_slice(&crc.to_be_bytes());
    section
}

fn build_pmt_section(program_number: u16, pcr_pid: u16, streams: &[MuxerStream]) -> Vec<u8> {
    let per_stream = 5usize; // stream_type + pid(2) + es_info_length(2)
    let section_length: u16 = (9 + per_stream * streams.len() + 4) as u16;
    let mut section = Vec::with_capacity(3 + section_length as usize);
    section.push(0x02); // table_id = PMT
    section.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
    section.push((section_length & 0xFF) as u8);
    section.extend_from_slice(&program_number.to_be_bytes());
    section.push(0xC1);
    section.push(0x00);
    section.push(0x00);
    section.extend_from_slice(&(0xE000u16 | (pcr_pid & 0x1FFF)).to_be_bytes());
    section.extend_from_slice(&0xF000u16.to_be_bytes()); // program_info_length = 0
    for s in streams {
        section.push(s.stream_type.as_u8());
        section.extend_from_slice(&(0xE000u16 | (s.pid & 0x1FFF)).to_be_bytes());
        section.extend_from_slice(&0xF000u16.to_be_bytes());
    }
    let crc = mpeg2_crc32(&section);
    section.extend_from_slice(&crc.to_be_bytes());
    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Demuxer, DemuxerEvent};

    fn collect_events(bytes: &[u8]) -> Vec<DemuxerEvent> {
        let mut demux = Demuxer::new();
        demux.push(bytes);
        demux.flush();
        let mut out = Vec::new();
        while let Some(ev) = demux.pop_event() {
            out.push(ev);
        }
        out
    }

    #[test]
    fn muxed_video_roundtrips_through_demuxer() {
        let config = MuxerConfig {
            transport_stream_id: 1,
            program_number: 1,
            pmt_pid: DEFAULT_PMT_PID,
            pcr_pid: DEFAULT_VIDEO_PID,
            streams: vec![MuxerStream {
                pid: DEFAULT_VIDEO_PID,
                stream_type: StreamType::H264,
            }],
        };
        let mut muxer = Muxer::new(config);

        // Two access units — one keyframe, one non-keyframe.
        let au1: Vec<u8> = (0..500).map(|i| i as u8).collect();
        let au2: Vec<u8> = (0..2000).map(|i| (i * 3) as u8).collect();
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(&muxer.write(MuxerInput {
            pid: DEFAULT_VIDEO_PID,
            pts: Some(90_000),
            dts: Some(90_000),
            is_keyframe: true,
            data: &au1,
        }));
        bytes.extend_from_slice(&muxer.write(MuxerInput {
            pid: DEFAULT_VIDEO_PID,
            pts: Some(93_000),
            dts: Some(93_000),
            is_keyframe: false,
            data: &au2,
        }));

        let events = collect_events(&bytes);
        let es: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                DemuxerEvent::EsPacket(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(es.len(), 2);
        assert_eq!(&es[0].data[..], &au1[..]);
        assert_eq!(es[0].pts, Some(90_000));
        assert_eq!(es[0].dts, Some(90_000));
        assert_eq!(&es[1].data[..], &au2[..]);
        assert_eq!(es[1].pts, Some(93_000));

        assert!(
            events
                .iter()
                .any(|e| matches!(e, DemuxerEvent::ProgramDiscovered(_)))
        );
    }

    #[test]
    fn ts_packets_are_aligned() {
        let mut muxer = Muxer::new(MuxerConfig::h264_aac());
        let data: Vec<u8> = (0..4000).map(|i| i as u8).collect();
        let bytes = muxer.write(MuxerInput {
            pid: DEFAULT_VIDEO_PID,
            pts: Some(0),
            dts: Some(0),
            is_keyframe: true,
            data: &data,
        });
        assert_eq!(bytes.len() % TS_PACKET_SIZE, 0);
        for chunk in bytes.chunks(TS_PACKET_SIZE) {
            assert_eq!(chunk[0], TS_SYNC_BYTE);
        }
    }
}
