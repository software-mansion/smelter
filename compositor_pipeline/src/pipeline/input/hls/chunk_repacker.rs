use std::{io::Read, slice};

use bytes::{Buf, Bytes, BytesMut};
use ffmpeg_next::Stream;
use tracing::warn;

pub struct ChunkRepacker {
    extra_data: Option<ExtraData>,
    sps_pps: Option<Bytes>,
}

impl ChunkRepacker {
    pub fn new(stream: &Stream<'_>) -> Self {
        let extra_data = unsafe {
            let codecpar = (*stream.as_ptr()).codecpar;
            let size = (*codecpar).extradata_size;
            if size > 0 {
                Some(Bytes::copy_from_slice(slice::from_raw_parts(
                    (*codecpar).extradata,
                    size as usize,
                )))
            } else {
                None
            }
        };

        let extra_data = extra_data
            .map(ExtraData::parse)
            .transpose()
            .unwrap_or_else(|e| match e {
                ExtraDataParseError::NotExtraData => None,
                _ => {
                    warn!("Could not parse extra data: {e}");
                    None
                }
            });

        let sps_pps = extra_data.as_ref().map(|extra_data| {
            let mut data = BytesMut::new();
            data.extend(
                extra_data
                    .spss
                    .iter()
                    .flat_map(|sps| [0, 0, 0, 1].iter().chain(sps)),
            );
            data.extend(
                extra_data
                    .ppss
                    .iter()
                    .flat_map(|pps| [0, 0, 0, 1].iter().chain(pps)),
            );
            data.freeze()
        });

        Self {
            extra_data,
            sps_pps,
        }
    }

    /// Repacks data from AVCC to Annex-B
    pub fn repack(&mut self, chunk_data: Bytes) -> Bytes {
        let Some(nalu_length_size) = self
            .extra_data
            .as_ref()
            .map(|extra_data| extra_data.nalu_length_size)
        else {
            // No repacking needed
            return chunk_data;
        };

        let mut data = BytesMut::new();
        if let Some(sps_pps) = self.sps_pps.take() {
            data.extend_from_slice(&sps_pps);
        }

        let mut reader = chunk_data.reader();
        loop {
            let mut len = [0u8; 4];

            if reader.read_exact(&mut len[4 - nalu_length_size..]).is_err() {
                break;
            }

            let len = u32::from_be_bytes(len);

            let mut nalu = BytesMut::zeroed(len as usize);
            reader.read_exact(&mut nalu).unwrap();

            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(&nalu);
        }

        data.freeze()
    }
}

struct ExtraData {
    nalu_length_size: usize,
    spss: Vec<Bytes>,
    ppss: Vec<Bytes>,
}

impl ExtraData {
    fn parse(mut data: Bytes) -> Result<Self, ExtraDataParseError> {
        let is_avcc = data.try_get_u8()? == 0x1;
        if !is_avcc {
            return Err(ExtraDataParseError::NotExtraData);
        }

        // Skip not needed information
        data = data.slice(3..);

        let nalu_length_size = (data.try_get_u8()? & 3) as usize + 1;

        let sps_num = data.try_get_u8()? & 0x1F;
        let spss = (0..sps_num)
            .map(|_| Self::parse_nalu(&mut data))
            .collect::<Result<_, _>>()?;

        let pps_num = data.try_get_u8()?;
        let ppss = (0..pps_num)
            .map(|_| Self::parse_nalu(&mut data))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            nalu_length_size,
            spss,
            ppss,
        })
    }

    fn parse_nalu(data: &mut Bytes) -> Result<Bytes, ExtraDataParseError> {
        let nalu_length = data.try_get_u16()? as usize;
        let contents = data.slice(0..nalu_length);
        *data = data.slice(nalu_length..);
        Ok(contents)
    }
}

#[derive(Debug, thiserror::Error)]
enum ExtraDataParseError {
    #[error("Incorrect extra data. Expected more bytes.")]
    NotEnoughBytes(#[from] bytes::TryGetError),

    #[error("Not extra data")]
    NotExtraData,
}
