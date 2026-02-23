use std::{io::Write, time::Instant};

use rand::RngCore;
use tracing::warn;

use crate::{
    error::RtmpError,
    protocol::socket::{BufferedReader, BufferedWriter},
};

const RTMP_VERSION: u8 = 3;
const HANDSHAKE_SIZE: usize = 1536;

pub struct Handshake;

impl Handshake {
    pub fn perform_as_server(
        reader: &mut BufferedReader,
        writer: &mut BufferedWriter,
    ) -> Result<(), RtmpError> {
        // C0 version
        let mut c0 = [0u8; 1];
        reader.read_exact(&mut c0)?;
        if c0[0] != RTMP_VERSION {
            warn!("C0 should be {RTMP_VERSION}, but received {}", c0[0]);
        };
        let c0_read_time = Instant::now();

        // S0 version
        writer.write_all(&[RTMP_VERSION])?;

        // S1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut s1 = [0u8; HANDSHAKE_SIZE];
        let timestamp: u32 = 0;
        s1[0..4].copy_from_slice(&timestamp.to_be_bytes());
        s1[4..8].fill(0); // zeros
        rand::rng().fill_bytes(&mut s1[8..]);
        writer.write_all(&s1)?;

        // C1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut c1 = [0u8; HANDSHAKE_SIZE];
        reader.read_exact(&mut c1)?;
        let c1_read_timestamp = c0_read_time.elapsed().as_millis() as u32;

        // S2 echo C1 with our timestamp
        let mut s2 = c1;
        s2[4..8].copy_from_slice(&c1_read_timestamp.to_be_bytes());
        writer.write_all(&s2)?;
        writer.flush()?;

        // C2 client echoes S1
        let mut c2 = [0u8; HANDSHAKE_SIZE];
        reader.read_exact(&mut c2)?;

        // timestamp and random bytes should match
        if c2[0..4] != s1[0..4] || c2[8..HANDSHAKE_SIZE] != s1[8..HANDSHAKE_SIZE] {
            return Err(RtmpError::HandshakeFailed("C2 does not match S1".into()));
        }

        Ok(())
    }

    pub fn perform_as_client(
        reader: &mut BufferedReader,
        writer: &mut BufferedWriter,
    ) -> Result<(), RtmpError> {
        let send_time = Instant::now();

        // C0 version
        writer.write_all(&[RTMP_VERSION])?;

        // C1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut c1 = [0u8; HANDSHAKE_SIZE];
        let timestamp: u32 = 0;
        c1[0..4].copy_from_slice(&timestamp.to_be_bytes());
        c1[4..8].fill(0);
        rand::rng().fill_bytes(&mut c1[8..]);
        writer.write_all(&c1)?;
        writer.flush()?;

        // S0 version
        let mut s0 = [0u8; 1];
        reader.read_exact(&mut s0)?;
        if s0[0] != RTMP_VERSION {
            return Err(RtmpError::HandshakeFailed(format!(
                "S0 should be {RTMP_VERSION}, but received {}",
                s0[0]
            )));
        };

        // S1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut s1 = [0u8; HANDSHAKE_SIZE];
        reader.read_exact(&mut s1)?;
        let s1_read_timestamp = send_time.elapsed().as_millis() as u32;

        // C2 echo S1 with our timestamp
        let mut c2 = s1;
        c2[4..8].copy_from_slice(&s1_read_timestamp.to_be_bytes());
        writer.write_all(&c2)?;
        writer.flush()?;

        // S2 server echoes C1
        let mut s2 = [0u8; HANDSHAKE_SIZE];
        reader.read_exact(&mut s2)?;

        if s2[0..4] != c1[0..4] || s2[8..HANDSHAKE_SIZE] != c1[8..HANDSHAKE_SIZE] {
            return Err(RtmpError::HandshakeFailed("S2 does not match C1".into()));
        }

        Ok(())
    }
}
