use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common_pipeline::prelude as pipeline;
use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransportProtocol {
    /// UDP protocol.
    Udp,
    /// TCP protocol where Smelter is the server side of the connection.
    TcpServer,
}

impl From<TransportProtocol> for pipeline::RtpInputTransportProtocol {
    fn from(value: TransportProtocol) -> Self {
        match value {
            TransportProtocol::Udp => pipeline::RtpInputTransportProtocol::Udp,
            TransportProtocol::TcpServer => pipeline::RtpInputTransportProtocol::TcpServer,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum PortOrPortRange {
    String(String),
    U16(u16),
}

impl TryFrom<PortOrPortRange> for pipeline::PortOrRange {
    type Error = TypeError;

    fn try_from(value: PortOrPortRange) -> Result<Self, Self::Error> {
        const PORT_CONVERSION_ERROR_MESSAGE: &str = "Port needs to be a number between 1 and 65535 or a string in the \"START:END\" format, where START and END represent a range of ports.";
        match value {
            PortOrPortRange::U16(0) => Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)),
            PortOrPortRange::U16(v) => Ok(pipeline::PortOrRange::Exact(v)),
            PortOrPortRange::String(s) => {
                let (start, end) = s
                    .split_once(':')
                    .ok_or(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE))?;

                let start = start
                    .parse::<u16>()
                    .or(Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)))?;
                let end = end
                    .parse::<u16>()
                    .or(Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE)))?;

                if start > end {
                    return Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE));
                }

                if start == 0 || end == 0 {
                    return Err(TypeError::new(PORT_CONVERSION_ERROR_MESSAGE));
                }

                Ok(pipeline::PortOrRange::Range((start, end)))
            }
        }
    }
}
