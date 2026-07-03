mod certificate;
mod client_input;
mod connection;
mod server;
mod server_input;

use hang::moq_net::Session;
use std::{ops::Deref, sync::Arc};

pub(crate) use certificate::SelfSignedTlsError;
pub use client_input::MoqClientInput;
pub(super) use server::{MoqPipelineState, MoqServer, spawn_moq_server};
pub use server_input::MoqServerInput;

pub(crate) struct MoqSession {
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
