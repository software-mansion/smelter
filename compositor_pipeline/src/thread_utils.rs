use tracing::{span, Level};

pub(crate) trait InitializableThread: Sized {
    type InitOptions: Send + 'static;

    /// Represents type returned on successful `init` to the caller of `Self::spawn`
    type SpawnOutput: Send + 'static;
    /// Represents type returned on failed `init` to the caller of `Self::spawn`
    type SpawnError: std::error::Error + Send + 'static;

    const LABEL: &'static str;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError>;

    fn run(self);

    fn spawn<Id: ToString>(
        thread_instance_id: Id,
        opts: Self::InitOptions,
    ) -> Result<Self::SpawnOutput, Self::SpawnError> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

        let instance_id = thread_instance_id.to_string();
        let metadata = Self::metadata();
        std::thread::Builder::new()
            .name(metadata.thread_name.to_string())
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "Thread",
                    label = Self::LABEL,
                    thread = metadata.thread_name,
                    instance = format!("{} {}", metadata.thread_instance_name, instance_id),
                )
                .entered();
                let state = match Self::init(opts) {
                    Ok((state, init_output)) => {
                        result_sender.send(Ok(init_output)).unwrap();
                        state
                    }
                    Err(err) => {
                        result_sender.send(Err(err)).unwrap();
                        return;
                    }
                };
                Self::run(state);
            })
            .unwrap();

        result_receiver.recv().unwrap()
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Initializable thread",
            thread_instance_name: "Instance",
        }
    }
}

pub(crate) struct ThreadMetadata {
    pub thread_name: &'static str,
    pub thread_instance_name: &'static str,
}
