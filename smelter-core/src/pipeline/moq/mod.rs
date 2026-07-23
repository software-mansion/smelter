mod input;
mod output;
mod server;

use hang::moq_net::Session;
use std::{ops::Deref, sync::Arc};

pub use input::{MoqClientInput, MoqServerInput};
pub use output::MoqClientOutput;
pub(crate) use server::SelfSignedTlsError;
pub(super) use server::{MoqPipelineState, MoqServer, spawn_moq_server};

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
