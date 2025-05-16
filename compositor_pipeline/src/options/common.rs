#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Udp,
    TcpServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedPort {
    Exact(u16),
    Range((u16, u16)),
}


