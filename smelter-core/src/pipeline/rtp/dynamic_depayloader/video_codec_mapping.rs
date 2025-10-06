use webrtc::rtp_transceiver::PayloadType;

#[derive(Debug, Clone)]
pub struct VideoPayloadTypeMapping {
    pub h264: Option<Vec<PayloadType>>,
    pub vp8: Option<Vec<PayloadType>>,
    pub vp9: Option<Vec<PayloadType>>,
}

impl VideoPayloadTypeMapping {
    pub fn is_payload_type_h264(&self, pt: u8) -> bool {
        matches!(&self.h264, Some(payload_types) if payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp8(&self, pt: u8) -> bool {
        matches!(&self.vp8, Some(payload_types) if payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp9(&self, pt: u8) -> bool {
        matches!(&self.vp9, Some(payload_types) if payload_types.contains(&pt))
    }

    pub fn has_any_codec(&self) -> bool {
        self.h264.is_some() || self.vp8.is_some() || self.vp9.is_some()
    }
}
