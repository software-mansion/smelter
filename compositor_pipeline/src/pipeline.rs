use compositor_common::Frame;
use crossbeam_channel::unbounded;
use std::{sync::Arc, thread};

use crate::{
    map::SyncHashMap,
    queue::{Framerate, Queue},
};

pub trait PipelineOutput {
    fn send_frame(&self, frame: Frame);
}

pub struct Pipeline<Output: PipelineOutput> {
    outputs: SyncHashMap<u32, Arc<Output>>,
    queue: Queue, //renderer: Renderer,
}

impl<Output: PipelineOutput> Pipeline<Output> {
    pub fn new() -> Self {
        Pipeline {
            outputs: SyncHashMap::new(),
            queue: Queue::new(Framerate(30)),
        }
    }

    pub fn add_input(&self, input_id: u32) {
        self.queue.add_input(input_id);
        // self.renderer.add_input();
    }

    pub fn add_output(&self, output_id: u32, output: Arc<Output>) {
        self.outputs.insert(output_id, output);
    }

    pub fn push_input_data(&self, input_id: u32, frame: Frame) {
        self.queue.enqueue_frame(input_id, frame).unwrap();
        // self.outputs.get_cloned(&8002).unwrap().send_frame(frame);
    }

    #[allow(dead_code)]
    fn on_output_data_received(&self, output_id: u32, frame: Frame) {
        match self.outputs.get_cloned(&output_id) {
            Some(output) => output.send_frame(frame),
            None => {
                eprintln!("Output {} not found", output_id);
            }
        }
    }

    pub fn start(self: &Arc<Self>) {
        let (frames_sender, frames_receiver) = unbounded();
        let pipeline = self.clone();

        pipeline.queue.start(frames_sender);

        thread::spawn(move || {
            loop {
                let input_frames = frames_receiver.recv().unwrap();
                println!("Input frames batch: {:#?}", input_frames);
                // let pipeline.render.render(output_frames);
                // for let (output_id, frames) in input_frames {
                // self.on_output_data_received(output_id, frames)
                // }
                // eprintln!("render loop");
                // thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}

impl<Output: PipelineOutput> Default for Pipeline<Output> {
    fn default() -> Self {
        Self::new()
    }
}
