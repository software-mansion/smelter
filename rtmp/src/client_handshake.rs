use std::{
    io::{Read, Write},
    time::Instant,
};

use crate::error::RtmpError;
use rand::RngCore;

const RTMP_VERSION: u8 = 3;
const HANDSHAKE_SIZE: usize = 1536;

pub struct ClientHandshake;

impl ClientHandshake {
    pub fn perform<S>(stream: &mut S) -> Result<(), RtmpError>
    where
        S: Read + Write,
    {
        let send_time = Instant::now();

        // C0 version
        stream.write_all(&[RTMP_VERSION])?;

        // C1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut c1 = [0u8; HANDSHAKE_SIZE];
        let timestamp: u32 = 0;
        c1[0..4].copy_from_slice(&timestamp.to_be_bytes());
        c1[4..8].fill(0);
        rand::rng().fill_bytes(&mut c1[8..]);
        stream.write_all(&c1)?;
        stream.flush()?;

        // S0 version
        let mut s0 = [0u8; 1];
        stream.read_exact(&mut s0)?;

        // S1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut s1 = [0u8; HANDSHAKE_SIZE];
        stream.read_exact(&mut s1)?;
        let s1_read_timestamp = send_time.elapsed().as_millis() as u32;

        // C2 echo S1 with our timestamp
        let mut c2 = s1;
        c2[4..8].copy_from_slice(&s1_read_timestamp.to_be_bytes());
        stream.write_all(&c2)?;
        stream.flush()?;

        // S2 server echoes C1
        let mut s2 = [0u8; HANDSHAKE_SIZE];
        stream.read_exact(&mut s2)?;

        if s2[0..4] != c1[0..4] || s2[8..HANDSHAKE_SIZE] != c1[8..HANDSHAKE_SIZE] {
            return Err(RtmpError::HandshakeFailed("S2 does not match C1".into()));
        }

        Ok(())
    }
}
