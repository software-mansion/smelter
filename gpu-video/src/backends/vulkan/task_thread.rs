use std::sync::mpsc::{Receiver, Sender};

pub(crate) struct TaskThread {
    sender: Sender<WorkerMessage>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TaskThread {
    pub(crate) fn spawn() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("gpu-video: task thread".to_string())
            .spawn(move || Self::run_thread(receiver))
            .unwrap();

        Self {
            sender,
            handle: Some(handle),
        }
    }

    fn run_thread(receiver: Receiver<WorkerMessage>) {
        for msg in receiver {
            match msg {
                WorkerMessage::Task(task) => task(),
                WorkerMessage::Sync(sender) => sender.send(()).unwrap(),
                WorkerMessage::Quit => return,
            }
        }
    }

    pub(crate) fn submit(&self, task: impl FnOnce() + Send + 'static) {
        self.send(WorkerMessage::Task(Box::new(task)))
    }

    /// Waits for all tasks scheduled until this point to complete.
    pub(crate) fn sync(&self) {
        let (sync_sender, sync_receiver) = std::sync::mpsc::channel();
        self.send(WorkerMessage::Sync(sync_sender));
        sync_receiver.recv().unwrap()
    }

    fn send(&self, msg: WorkerMessage) {
        self.sender.send(msg).unwrap()
    }
}

impl Drop for TaskThread {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        self.send(WorkerMessage::Quit);
        handle.join().unwrap()
    }
}

enum WorkerMessage {
    Task(Box<dyn FnOnce() + Send>),
    Sync(Sender<()>),
    Quit,
}
