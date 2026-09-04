mod input;
mod output;
mod server;

use hang::moq_net::Session;
use moq_native::ClientConfig;
use std::{
    net::{SocketAddr, ToSocketAddrs},
    ops::Deref,
    sync::Arc,
};
use url::Url;

pub use input::{MoqClientInput, MoqServerInput};
pub use output::MoqClientOutput;
pub(crate) use server::SelfSignedTlsError;
pub(super) use server::{MoqPipelineState, MoqServer, spawn_moq_server};

/// Client config for connecting to `url`. The client picks the DNS entry
/// matching the family of its local socket, so bind to IPv4 whenever the host
/// resolves to one; the default `[::]` socket would pick an IPv6 entry even
/// on hosts without IPv6 connectivity.
fn client_config(url: &Url, disable_tls_verification: bool) -> ClientConfig {
    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(disable_tls_verification);

    let host = url.host_str().unwrap_or_default();
    let port = url.port().unwrap_or(443);
    let has_ipv4 = (host, port)
        .to_socket_addrs()
        .map(|mut addrs| addrs.any(|addr| addr.is_ipv4()))
        .unwrap_or(false);
    if has_ipv4 {
        config.bind = SocketAddr::from(([0, 0, 0, 0], 0));
    }
    config
}

pub(super) struct MoqSession {
    session: Session,
    rt: Arc<tokio::runtime::Runtime>,
}

impl MoqSession {
    fn new(session: Session, rt: Arc<tokio::runtime::Runtime>) -> Self {
        Self { session, rt }
    }
}

impl Deref for MoqSession {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl Drop for MoqSession {
    fn drop(&mut self) {
        let _guard = self.rt.enter();
        self.session.close(hang::moq_net::Error::Cancel);
        tracing::info!("MoQ session closed!");
    }
}
