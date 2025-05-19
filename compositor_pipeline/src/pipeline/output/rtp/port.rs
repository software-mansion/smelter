use std::net;

use super::Port;

pub(crate) const VIDEO_PAYLOAD_TYPE: u8 = 96;
pub(crate) const AUDIO_PAYLOAD_TYPE: u8 = 97;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Udp,
    TcpServer,
}

pub(super) enum BindToPortError {
    SocketBind(std::io::Error),
    PortAlreadyInUse(u16),
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedPort {
    Exact(u16),
    Range((u16, u16)),
}

pub(super) fn bind_to_requested_port(
    requested_port: RequestedPort,
    socket: &socket2::Socket,
) -> Result<Port, BindToPortError> {
    let port = match requested_port {
        RequestedPort::Exact(port) => {
            socket
                .bind(
                    &net::SocketAddr::V4(net::SocketAddrV4::new(net::Ipv4Addr::UNSPECIFIED, port))
                        .into(),
                )
                .map_err(|err| match err.kind() {
                    std::io::ErrorKind::AddrInUse => BindToPortError::PortAlreadyInUse(port),
                    _ => BindToPortError::SocketBind(err),
                })?;
            port
        }
        RequestedPort::Range((lower_bound, upper_bound)) => {
            let port = (lower_bound..upper_bound).find(|port| {
                let bind_res = socket.bind(
                    &net::SocketAddr::V4(net::SocketAddrV4::new(net::Ipv4Addr::UNSPECIFIED, *port))
                        .into(),
                );

                bind_res.is_ok()
            });

            match port {
                Some(port) => port,
                None => {
                    return Err(BindToPortError::AllPortsAlreadyInUse {
                        lower_bound,
                        upper_bound,
                    })
                }
            }
        }
    };
    Ok(Port(port))
}
