use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use crate::backends::vulkan::wrappers::VideoSession;

use super::{
    AccessSource, DeclaredResource, ExecutionContextShared, Executor, HostRead, HostReadable,
    Input, Outcome, Produced, QueueBinding, Resource, SessionToken, Step, StepBuilder, StepId,
    SubresourceKey, TaskError, TaskId,
};

#[cfg(feature = "wgpu")]
use super::{ExportedImage, ImageLayer, Timeline, WgpuBinding};

pub(crate) type CompletionCallback = Box<dyn FnOnce(Outcome<'_>) + Send>;

pub(crate) struct Task {
    pub(super) id: TaskId,
    pub(super) executor: Arc<Executor>,
    pub(super) context: Arc<ExecutionContextShared>,
    pub(super) steps: Vec<Step>,
    pub(super) unfinished: Option<&'static str>,
    pub(super) sessions: Vec<(Arc<VideoSession>, SessionStatus)>,
    pub(super) host_reads: Vec<HostReadDecl>,
    #[cfg(feature = "wgpu")]
    pub(super) exports: Vec<ExportDecl>,
    #[cfg(feature = "wgpu")]
    pub(super) import: Option<ImportDecl>,
    pub(super) keep_alive: Vec<Box<dyn Any + Send>>,
    pub(super) on_complete: Option<CompletionCallback>,
    pub(super) on_abandon: Vec<Arc<AtomicBool>>,
    pub(super) versions: HashMap<SubresourceKey, (StepId, u32)>,
    pub(super) submitted: bool,
    pub(super) next_step: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionStatus {
    Declared,
    Submitted,
}

pub(crate) struct HostReadDecl {
    pub(super) step: StepId,
    pub(super) version: u32,
    pub(super) resource: DeclaredResource,
}

#[cfg(feature = "wgpu")]
pub(crate) struct ExportDecl {
    pub(super) step: StepId,
    pub(super) image: ImageLayer,
    pub(super) queue: wgpu::Queue,
    pub(super) usage: wgpu::TextureUses,
    pub(super) format: wgpu::TextureFormat,
}

#[cfg(feature = "wgpu")]
pub(crate) struct ImportDecl {
    pub(super) step: StepId,
    pub(super) image: ImageLayer,
    pub(super) texture: wgpu::Texture,
    pub(super) timeline: Arc<Timeline>,
    pub(super) probe: wgpu::Buffer,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) committed: bool,
}

impl Task {
    pub(super) fn new(executor: Arc<Executor>, context: Arc<ExecutionContextShared>) -> Self {
        todo!()
    }

    pub(crate) fn step<'g>(
        &'g mut self,
        queue: &QueueBinding,
        label: &'static str,
    ) -> StepBuilder<'g> {
        todo!()
    }

    pub(crate) fn host_read<R: HostReadable>(
        &mut self,
        produced: &Produced<R>,
    ) -> Result<HostRead<R>, TaskError> {
        todo!()
    }

    pub(crate) fn keep_alive<T: Send + 'static>(&mut self, object: T) {
        todo!()
    }

    pub(crate) fn on_abandon(&mut self, flag: Arc<AtomicBool>) {
        todo!()
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn export_to_wgpu(
        &mut self,
        image: &Produced<ImageLayer>,
        wgpu_queue: &wgpu::Queue,
        usage: wgpu::TextureUses,
        format: wgpu::TextureFormat,
    ) -> Result<ExportedImage, TaskError> {
        todo!()
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn import_from_wgpu(
        &mut self,
        image: ImageLayer,
        texture: &wgpu::Texture,
        binding: &WgpuBinding,
    ) -> Result<Produced<ImageLayer>, TaskError> {
        todo!()
    }

    pub(crate) fn on_complete(
        &mut self,
        f: impl FnOnce(Outcome<'_>) + Send + 'static,
    ) -> Result<(), TaskError> {
        todo!()
    }

    pub(super) fn allocate_step_id(&mut self) -> StepId {
        todo!()
    }

    pub(super) fn register_session(
        &mut self,
        step: StepId,
        session: &Arc<VideoSession>,
        family_index: u32,
    ) -> Result<SessionToken, TaskError> {
        todo!()
    }

    pub(super) fn bump_version(&mut self, keys: &[SubresourceKey], step: StepId) -> u32 {
        todo!()
    }

    pub(super) fn check_input<R: Resource>(
        &self,
        input: &Input<R>,
    ) -> Result<AccessSource, TaskError> {
        todo!()
    }

    pub(super) fn push_step(&mut self, step: Step) -> StepId {
        todo!()
    }

    pub(super) fn poison_unsubmitted_sessions(&mut self) {
        todo!()
    }

    pub(super) fn set_abandon_flags(&self) {
        todo!()
    }

    pub(super) fn release_open_sessions(&self) {
        todo!()
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        todo!()
    }
}
