use bytes::Bytes;

/// VPCodecConfigurationRecord (vpcC) required by E-RTMP SequenceStart
/// for VP8 and VP9 codecs. See VP Codec ISO Media File Format Binding, §4.
pub struct VpCodecConfig {
    profile: u8,
    chroma_subsampling: u8,
}

impl VpCodecConfig {
    pub fn vp8() -> Self {
        Self {
            profile: 0,
            chroma_subsampling: 1,
        }
    }

    pub fn vp9_yuv420p() -> Self {
        Self {
            profile: 0,
            chroma_subsampling: 1,
        }
    }

    pub fn vp9_yuv422p() -> Self {
        Self {
            profile: 1,
            chroma_subsampling: 2,
        }
    }

    pub fn vp9_yuv444p() -> Self {
        Self {
            profile: 1,
            chroma_subsampling: 3,
        }
    }

    pub fn to_bytes(self) -> Bytes {
        Bytes::copy_from_slice(&[
            1, // version
            0,
            0,
            0, // flags
            self.profile,
            0,                                         // level (undefined for VP8/VP9 live)
            (8 << 4) | (self.chroma_subsampling << 1), // bitDepth | chromaSubsampling | videoFullRangeFlag
            1,                                         // BT.709 color primaries
            1,                                         // BT.709 transfer characteristics
            1,                                         // BT.709 matrix coefficients
            0,
            0, // codecInitializationDataSize = 0
        ])
    }
}
