use std::{
    any::Any,
    collections::VecDeque,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

type FinishResult = Result<(), Box<dyn Any + Send>>;

pub(crate) struct InFlightTracker {
    max_in_flight: usize,
    in_flight: VecDeque<Receiver<FinishResult>>,
}

#[must_use]
pub(crate) struct SubmissionToken {
    finished_sender: Sender<FinishResult>,
}

impl SubmissionToken {
    pub(crate) fn finish(self) {
        let _ = self.finished_sender.send(Ok(()));
    }

    pub(crate) fn finish_with_panic(self, payload: Box<dyn Any + Send>) {
        let _ = self.finished_sender.send(Err(payload));
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Timed out waiting for a submission to finish")]
pub(crate) struct SubmissionWaitTimeout;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SubmitError<E> {
    #[error(transparent)]
    Timeout(SubmissionWaitTimeout),

    #[error(transparent)]
    Submit(E),
}

impl InFlightTracker {
    pub(crate) fn new(max_in_flight: usize) -> Self {
        Self {
            max_in_flight,
            in_flight: VecDeque::new(),
        }
    }

    pub(crate) fn wait_if_full(&mut self, timeout: Duration) -> Result<(), SubmissionWaitTimeout> {
        if self.max_in_flight == 0 {
            return Ok(());
        }

        while self.in_flight.len() >= self.max_in_flight {
            self.wait_for_oldest(timeout)?;
        }

        Ok(())
    }

    pub(crate) fn submit<E>(
        &mut self,
        timeout: Duration,
        submit: impl FnOnce(SubmissionToken) -> Result<(), E>,
    ) -> Result<(), SubmitError<E>> {
        self.wait_if_full(timeout).map_err(SubmitError::Timeout)?;

        let (finished_sender, finished_receiver) = std::sync::mpsc::channel();
        submit(SubmissionToken { finished_sender }).map_err(SubmitError::Submit)?;
        self.in_flight.push_back(finished_receiver);

        if self.max_in_flight == 0 {
            self.wait_for_all(timeout).map_err(SubmitError::Timeout)?;
        }

        Ok(())
    }

    pub(crate) fn wait_for_all(&mut self, timeout: Duration) -> Result<(), SubmissionWaitTimeout> {
        while !self.in_flight.is_empty() {
            self.wait_for_oldest(timeout)?;
        }

        Ok(())
    }

    fn wait_for_oldest(&mut self, timeout: Duration) -> Result<(), SubmissionWaitTimeout> {
        let Some(oldest) = self.in_flight.front() else {
            return Ok(());
        };

        let result = oldest
            .recv_timeout(timeout)
            .map_err(|_| SubmissionWaitTimeout)?;
        self.in_flight.pop_front();

        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }

        Ok(())
    }
}
