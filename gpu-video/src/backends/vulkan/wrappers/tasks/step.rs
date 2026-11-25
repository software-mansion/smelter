use std::{
    any::Any,
    sync::{Arc, atomic::AtomicBool},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{
    ResultQuery, ResultQueryData, VideoSession, VideoSessionParameters,
};

use super::{
    BufferRange, CommandBufferPool, ImageLayer, Input, Picture, Produced, QueryRef, Queue,
    QueueBinding, Recorder, Resource, Role, SessionToken, StepId, Task, TaskError, TaskId,
    Timeline, Use, ViewRef,
};

pub(crate) struct StepQueue {
    pub(super) role: Role,
    pub(super) queue: Arc<Queue>,
    pub(super) timeline: Arc<Timeline>,
    pub(super) pool: Arc<CommandBufferPool>,
}

impl QueueBinding {
    pub(super) fn step_queue(&self) -> StepQueue {
        todo!()
    }
}

pub(crate) struct Step {
    pub(super) id: StepId,
    pub(super) label: &'static str,
    pub(super) queue: StepQueue,
    pub(super) accesses: Vec<Access>,
    pub(super) binds: Vec<Picture>,
    pub(super) queries: Vec<DeclaredQuery>,
    pub(super) session: Option<DeclaredSession>,
    pub(super) after: Vec<StepId>,
    pub(super) body: Option<vk::CommandBuffer>,
    pub(super) keep_alive: Vec<Box<dyn Any + Send>>,
    pub(super) submitted: bool,
}

pub(crate) struct DeclaredSession {
    pub(super) session: Arc<VideoSession>,
    pub(super) parameters: Arc<VideoSessionParameters>,
    pub(super) resets: bool,
}

pub(crate) struct Access {
    pub(super) resource: DeclaredResource,
    pub(super) usage: Use,
    pub(super) direction: Direction,
    pub(super) source: AccessSource,
}

#[derive(Clone)]
pub(crate) enum DeclaredResource {
    Image {
        range: ImageLayer,
        view: Option<ViewRef>,
    },
    Buffer(BufferRange),
    Query {
        pool: vk::QueryPool,
        index: u32,
        owner: Arc<dyn Any + Send + Sync>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Read,
    Write,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessSource {
    Current,
    Produced {
        task: TaskId,
        step: StepId,
        version: u32,
    },
}

pub(crate) struct DeclaredQuery {
    pub(super) pool: vk::QueryPool,
    pub(super) index: u32,
    pub(super) owner: Arc<dyn Any + Send + Sync>,
}

#[must_use = "a step that is not finished poisons its task"]
pub(crate) struct StepBuilder<'g> {
    pub(super) task: &'g mut Task,
    pub(super) step: Option<Step>,
}

impl<'g> StepBuilder<'g> {
    pub(crate) fn reads<R: Resource>(
        &mut self,
        input: impl Into<Input<R>>,
        usage: Use,
    ) -> &mut Self {
        todo!()
    }

    pub(crate) fn writes<R: Resource>(
        &mut self,
        input: impl Into<Input<R>>,
        usage: Use,
    ) -> Produced<R> {
        todo!()
    }

    pub(crate) fn binds(&mut self, picture: Picture) -> &mut Self {
        todo!()
    }

    pub(crate) fn query<T: ResultQueryData + Send + Sync + 'static>(
        &mut self,
        query: &Arc<ResultQuery<T>>,
    ) -> Produced<QueryRef<T>> {
        todo!()
    }

    pub(crate) fn uses_session(
        &mut self,
        session: &Arc<VideoSession>,
        parameters: &Arc<VideoSessionParameters>,
    ) -> Result<SessionToken, TaskError> {
        todo!()
    }

    pub(crate) fn after(&mut self, step: StepId) -> &mut Self {
        todo!()
    }

    pub(crate) fn keep_alive<T: Send + 'static>(&mut self, object: T) -> &mut Self {
        todo!()
    }

    pub(crate) fn on_abandon(&mut self, flag: Arc<AtomicBool>) -> &mut Self {
        todo!()
    }

    pub(crate) fn record(
        &mut self,
        f: impl FnOnce(&mut Recorder<'_>) -> Result<(), TaskError>,
    ) -> Result<&mut Self, TaskError> {
        todo!()
    }

    pub(crate) fn finish(self) -> Result<StepId, TaskError> {
        todo!()
    }

    fn step_mut(&mut self) -> &mut Step {
        todo!()
    }

    fn declare<R: Resource>(
        &mut self,
        input: Input<R>,
        usage: Use,
        direction: Direction,
    ) -> AccessSource {
        todo!()
    }

    fn check(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_directions(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_queue_flags(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_usage(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_sharing(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn merge_declarations(&mut self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_tokens(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_wgpu_state(&self) -> Result<(), TaskError> {
        todo!()
    }

    fn check_pictures(&self) -> Result<(), TaskError> {
        todo!()
    }
}

impl Drop for StepBuilder<'_> {
    fn drop(&mut self) {
        todo!()
    }
}
