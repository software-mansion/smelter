use crate::error::RtmpError;
use rand::RngCore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const RTMP_VERSION: u8 = 3;
const HANDSHAKE_SIZE: usize = 1536;

pub struct Handshake;

impl Handshake {
    pub async fn perform<S>(stream: &mut S) -> Result<(), RtmpError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // C0 version
        let mut c0 = [0u8; 1];
        stream.read_exact(&mut c0).await?;

        if c0[0] != RTMP_VERSION {
            return Err(RtmpError::InvalidVersion(c0[0]));
        }
        // S0 version
        stream.write_all(&[RTMP_VERSION]).await?;

        // S1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut s1 = [0u8; HANDSHAKE_SIZE];
        let timestamp: u32 = 0;
        s1[0..4].copy_from_slice(&timestamp.to_be_bytes());
        s1[4..8].copy_from_slice(&[0u8; 4]); // zeros
        rand::rng().fill_bytes(&mut s1[8..]);
        stream.write_all(&s1).await?;

        // C1 timestamp(4 bytes), zero(4 bytes), random(1528 bytes)
        let mut c1 = [0u8; HANDSHAKE_SIZE];
        stream.read_exact(&mut c1).await?;

        // S2 echo C1 with our timestamp
        let mut s2 = c1;
        s2[4..8].copy_from_slice(&timestamp.to_be_bytes());
        stream.write_all(&s2).await?;
        stream.flush().await?;

        // C2 client echoes S1
        let mut c2 = [0u8; HANDSHAKE_SIZE];
        stream.read_exact(&mut c2).await?;

        if c2 != s1 {
            return Err(RtmpError::HandshakeFailed("C2 does not match S1".into()));
        }

        Ok(())
    }
}
