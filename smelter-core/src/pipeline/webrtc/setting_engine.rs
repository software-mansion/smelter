use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use tokio::{net::UdpSocket, runtime::Runtime};
use tracing::warn;
use webrtc::{
    api::setting_engine::SettingEngine,
    ice::{
        network_type::NetworkType,
        udp_mux::{UDPMux, UDPMuxDefault, UDPMuxParams},
        udp_network::{EphemeralUDP, UDPNetwork},
    },
    ice_transport::ice_candidate_type::RTCIceCandidateType,
};
use webrtc_util::{Conn, sync::Mutex};

use crate::{error::InitPipelineError, prelude::WebrtcUdpPortStrategy};

#[derive(Clone)]
pub(crate) enum WebrtcSettingEngineCtx {
    AnyPort {
        nat_1to1_ips: Arc<Vec<String>>,
    },
    PortRange {
        start: u16,
        end: u16,
        nat_1to1_ips: Arc<Vec<String>>,
    },
    MuxOnSinglePort {
        nat_1to1_ips: Arc<Vec<String>>,
        udp_mux: Arc<UDPMuxDefault>,
        socket: Arc<Mutex<Option<Arc<UdpSocket>>>>,
        tokio_rt: Arc<Runtime>,
    },
}

impl WebrtcSettingEngineCtx {
    pub fn new(
        nat_1to1_ips: Arc<Vec<String>>,
        port_strategy: Option<WebrtcUdpPortStrategy>,
        tokio_rt: &Arc<Runtime>,
    ) -> Result<Self, InitPipelineError> {
        match port_strategy {
            Some(WebrtcUdpPortStrategy::PortRange(start, end)) => Ok(Self::PortRange {
                start,
                end,
                nat_1to1_ips,
            }),
            Some(WebrtcUdpPortStrategy::Mux(port)) => {
                // WARNING: Make sure this code is never run in async context.
                let (udp_mux, socket) = tokio_rt
                    .block_on(setup_socket_for_muxing(port))
                    .map_err(|e| InitPipelineError::BindUdpMuxSocket(port, e))?;
                Ok(Self::MuxOnSinglePort {
                    nat_1to1_ips,
                    udp_mux,
                    socket: Arc::new(Mutex::new(Some(socket))),
                    tokio_rt: tokio_rt.clone(),
                })
            }
            None => Ok(Self::AnyPort { nat_1to1_ips }),
        }
    }

    pub fn close(&self) {
        if let WebrtcSettingEngineCtx::MuxOnSinglePort {
            udp_mux,
            socket,
            tokio_rt,
            ..
        } = self
        {
            let udp_mux = udp_mux.clone();
            let socket = socket.clone();
            tokio_rt.spawn(async move {
                if let Err(err) = udp_mux.close().await {
                    warn!(%err, "Failed to close UDP socket")
                }
                socket.lock().take();
            });
        }
    }

    pub fn create_setting_engine(&self) -> SettingEngine {
        let mut setting_engine = SettingEngine::default();

        if !self.nat_1to1_ips().is_empty() {
            setting_engine
                .set_nat_1to1_ips(self.nat_1to1_ips().to_vec(), RTCIceCandidateType::Host);
            setting_engine.set_network_types(vec![NetworkType::Udp4]);
        };

        match self {
            WebrtcSettingEngineCtx::AnyPort { .. } => (),
            WebrtcSettingEngineCtx::PortRange { start, end, .. } => {
                let mut ephemeral_udp = EphemeralUDP::default();
                ephemeral_udp
                    .set_ports(*start, u16::max(*end, *start))
                    .unwrap(); // It can only fail if start>end
                setting_engine.set_udp_network(UDPNetwork::Ephemeral(ephemeral_udp));
            }
            WebrtcSettingEngineCtx::MuxOnSinglePort { udp_mux, .. } => {
                setting_engine.set_udp_network(UDPNetwork::Muxed(udp_mux.clone()));
                if !self.nat_1to1_ips().is_empty() {
                    // If:
                    //  - NAT 1to1 IP is provided
                    //  - UDP muxing is enabled
                    //  - device has more than one network interfaces
                    // It triggers a bug in webrtc that is causing attempt to create
                    // multiple candidates with the same IP. When new candidate is a duplicate
                    // webrtc is closing the candidate, but as a result it also closes Conn
                    // that is shared between all those candidates.
                    //
                    // This workaround stores first IP and only allows that ip to be used.
                    let once = Mutex::new(None);
                    setting_engine.set_ip_filter(Box::new(move |ip: IpAddr| -> bool {
                        once.lock().get_or_insert(ip) == &ip
                    }));
                }
            }
        };
        setting_engine
    }

    fn nat_1to1_ips(&self) -> &Vec<String> {
        match self {
            WebrtcSettingEngineCtx::AnyPort { nat_1to1_ips } => nat_1to1_ips,
            WebrtcSettingEngineCtx::PortRange { nat_1to1_ips, .. } => nat_1to1_ips,
            WebrtcSettingEngineCtx::MuxOnSinglePort { nat_1to1_ips, .. } => nat_1to1_ips,
        }
    }
}

async fn setup_socket_for_muxing(
    port: u16,
) -> Result<(Arc<UDPMuxDefault>, Arc<UdpSocket>), io::Error> {
    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let mut last_error: Option<std::io::Error> = None;
    for _ in 0..5 {
        match UdpSocket::bind(addr).await {
            Ok(socket) => {
                let socket = Arc::new(socket);
                let mux_socket = UdpMuxSocket(Arc::downgrade(&socket));
                let udp_mux = UDPMuxDefault::new(UDPMuxParams::new(mux_socket));
                return Ok((udp_mux, socket));
            }
            Err(err) => {
                warn!("Failed to bind to port {port}. Retrying ...");
                last_error = Some(err)
            }
        };
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Err(last_error.unwrap())
}

struct UdpMuxSocket(Weak<UdpSocket>);

#[async_trait]
impl Conn for UdpMuxSocket {
    async fn connect(&self, addr: SocketAddr) -> webrtc_util::Result<()> {
        Ok(self.try_socket()?.connect(addr).await?)
    }

    async fn recv(&self, buf: &mut [u8]) -> webrtc_util::Result<usize> {
        Ok(self.try_socket()?.recv(buf).await?)
    }

    async fn recv_from(&self, buf: &mut [u8]) -> webrtc_util::Result<(usize, SocketAddr)> {
        Ok(self.try_socket()?.recv_from(buf).await?)
    }

    async fn send(&self, buf: &[u8]) -> webrtc_util::Result<usize> {
        Ok(self.try_socket()?.send(buf).await?)
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> webrtc_util::Result<usize> {
        Ok(self.try_socket()?.send_to(buf, target).await?)
    }

    fn local_addr(&self) -> webrtc_util::Result<SocketAddr> {
        Ok(self.try_socket()?.local_addr()?)
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    async fn close(&self) -> webrtc_util::Result<()> {
        Ok(())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

impl UdpMuxSocket {
    fn try_socket(&self) -> webrtc_util::Result<Arc<UdpSocket>> {
        self.0.upgrade().ok_or(webrtc_util::Error::ErrAlreadyClosed)
    }
}
