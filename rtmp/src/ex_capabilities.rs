use std::collections::HashMap;

use crate::amf0::AmfValue;

/// Extended capability flags for the `capsEx` property in the E-RTMP connect
/// handshake. See `enum CapsExMask` in the spec.
pub(crate) const CAPS_EX_RECONNECT: u8 = 0x01;
pub(crate) const CAPS_EX_MULTITRACK: u8 = 0x02;
pub(crate) const CAPS_EX_MODEX: u8 = 0x04;
pub(crate) const CAPS_EX_TIMESTAMP_NANO: u8 = 0x08;

/// Parsed E-RTMP feature support negotiated through the connect handshake.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExCapabilities {
    pub(crate) reconnect: bool,
    pub(crate) multitrack: bool,
    pub(crate) mod_ex: bool,
    pub(crate) timestamp_nano: bool,
}

impl ExCapabilities {
    pub(crate) fn from_connect_response(
        properties: &HashMap<String, AmfValue>,
        information: &HashMap<String, AmfValue>,
    ) -> Self {
        let bits = parse_caps_ex_bits(properties) | parse_caps_ex_bits(information);
        Self::from_caps_ex_bits(bits)
    }

    fn from_caps_ex_bits(caps_ex_bits: u8) -> Self {
        Self {
            reconnect: (caps_ex_bits & CAPS_EX_RECONNECT) != 0,
            multitrack: (caps_ex_bits & CAPS_EX_MULTITRACK) != 0,
            mod_ex: (caps_ex_bits & CAPS_EX_MODEX) != 0,
            timestamp_nano: (caps_ex_bits & CAPS_EX_TIMESTAMP_NANO) != 0,
        }
    }

    pub(crate) fn supports_timestamp_nano_mod_ex(self) -> bool {
        self.mod_ex && self.timestamp_nano
    }
}

fn parse_caps_ex_bits(map: &HashMap<String, AmfValue>) -> u8 {
    match map.get("capsEx") {
        Some(AmfValue::Number(bits)) if bits.is_finite() => {
            bits.floor().clamp(0.0, u8::MAX as f64) as u8
        }
        _ => 0,
    }
}
