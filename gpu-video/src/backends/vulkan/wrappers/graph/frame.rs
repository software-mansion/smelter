use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{
    ResultQuery, ResultQueryData, VideoSession, VideoSessionParameters,
};

use super::{
    BufferRange, CommandBufferPool, Executor, GraphError, GraphId, HostRead, HostReadable,
    ImageRange, Input, Outcome, PassId, Produced, QueryRef, Queue, QueueBinding, Recorder,
    Resource, Role, SubresourceKey, Timeline, Use, WindowShared,
};

#[cfg(feature = "wgpu")]
use super::{ExportedImage, WgpuBinding};

pub(crate) type CompletionCallback = Box<dyn FnOnce(Outcome<'_>) + Send>;

pub(crate) struct Graph {
    pub(super) id: GraphId,
    pub(super) executor: Arc<Executor>,
    pub(super) window: Arc<WindowShared>,
    pub(super) passes: Vec<Pass>,
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
    pub(super) versions: HashMap<SubresourceKey, (PassId, u32)>,
    pub(super) submitted: bool,
    pub(super) next_pass: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionStatus {
    Declared,
    Submitted,
}

pub(crate) struct PassQueue {
    pub(super) role: Role,
    pub(super) queue: Arc<Queue>,
    pub(super) timeline: Arc<Timeline>,
    pub(super) pool: Arc<CommandBufferPool>,
}

impl QueueBinding {
    pub(super) fn pass_queue(&self) -> PassQueue {
        todo!()
    }
}

pub(crate) struct Pass {
    pub(super) id: PassId,
    pub(super) label: &'static str,
    pub(super) queue: PassQueue,
    pub(super) accesses: Vec<Access>,
    pub(super) binds: Vec<ImageRange>,
    pub(super) queries: Vec<DeclaredQuery>,
    pub(super) session: Option<DeclaredSession>,
    pub(super) after: Vec<PassId>,
    pub(super) body: Option<vk::CommandBuffer>,
    pub(super) keep_alive: Vec<Box<dyn Any + Send>>,
    pub(super) coding_scope: CodingScopeState,
    pub(super) submitted: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CodingScopeState {
    pub(super) began: bool,
    pub(super) ended: bool,
    pub(super) reset: bool,
}

pub(crate) struct DeclaredSession {
    pub(super) session: Arc<VideoSession>,
    pub(super) parameters: Arc<VideoSessionParameters>,
}

pub(crate) struct Access {
    pub(super) resource: DeclaredResource,
    pub(super) usage: Use,
    pub(super) direction: Direction,
    pub(super) source: AccessSource,
}

#[derive(Clone)]
pub(crate) enum DeclaredResource {
    Image(ImageRange),
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
    WriteDiscarding,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessSource {
    Current,
    Produced {
        graph: GraphId,
        pass: PassId,
        version: u32,
    },
}

pub(crate) struct DeclaredQuery {
    pub(super) pool: vk::QueryPool,
    pub(super) index: u32,
    pub(super) owner: Arc<dyn Any + Send + Sync>,
}

pub(crate) struct HostReadDecl {
    pub(super) pass: PassId,
    pub(super) version: u32,
    pub(super) resource: DeclaredResource,
}

#[cfg(feature = "wgpu")]
pub(crate) struct ExportDecl {
    pub(super) pass: PassId,
    pub(super) image: ImageRange,
    pub(super) queue: wgpu::Queue,
    pub(super) usage: wgpu::TextureUses,
    pub(super) format: wgpu::TextureFormat,
}

#[cfg(feature = "wgpu")]
pub(crate) struct ImportDecl {
    pub(super) pass: PassId,
    pub(super) image: ImageRange,
    pub(super) texture: wgpu::Texture,
    pub(super) timeline: Arc<Timeline>,
    pub(super) probe: wgpu::Buffer,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) committed: bool,
}

impl Graph {
    pub(super) fn new(executor: Arc<Executor>, window: Arc<WindowShared>) -> Self {
        todo!()
    }

    pub(crate) fn pass<'g>(
        &'g mut self,
        queue: &QueueBinding,
        label: &'static str,
    ) -> PassBuilder<'g> {
        todo!()
    }

    pub(crate) fn host_read<R: HostReadable>(
        &mut self,
        produced: &Produced<R>,
    ) -> Result<HostRead<R>, GraphError> {
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
        image: &Produced<ImageRange>,
        wgpu_queue: &wgpu::Queue,
        usage: wgpu::TextureUses,
        format: wgpu::TextureFormat,
    ) -> Result<ExportedImage, GraphError> {
        todo!()
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn import_from_wgpu(
        &mut self,
        image: ImageRange,
        texture: &wgpu::Texture,
        binding: &WgpuBinding,
    ) -> Result<Produced<ImageRange>, GraphError> {
        todo!()
    }

    pub(crate) fn on_complete(
        &mut self,
        f: impl FnOnce(Outcome<'_>) + Send + 'static,
    ) -> Result<(), GraphError> {
        todo!()
    }

    pub(super) fn allocate_pass_id(&mut self) -> PassId {
        todo!()
    }

    pub(super) fn register_session(
        &mut self,
        session: &Arc<VideoSession>,
        family_index: u32,
    ) -> Result<(), GraphError> {
        todo!()
    }

    pub(super) fn bump_version(&mut self, keys: &[SubresourceKey], pass: PassId) -> u32 {
        todo!()
    }

    pub(super) fn check_input<R: Resource>(
        &self,
        input: &Input<R>,
    ) -> Result<AccessSource, GraphError> {
        todo!()
    }

    pub(super) fn push_pass(&mut self, pass: Pass) -> PassId {
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

impl Drop for Graph {
    fn drop(&mut self) {
        todo!()
    }
}

#[must_use = "a pass that is not finished poisons its graph"]
pub(crate) struct PassBuilder<'g> {
    pub(super) graph: &'g mut Graph,
    pub(super) pass: Option<Pass>,
}

impl<'g> PassBuilder<'g> {
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

    pub(crate) fn writes_discarding(
        &mut self,
        input: impl Into<Input<ImageRange>>,
        usage: Use,
    ) -> Produced<ImageRange> {
        todo!()
    }

    pub(crate) fn binds(&mut self, range: ImageRange) -> &mut Self {
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
    ) -> Result<&mut Self, GraphError> {
        todo!()
    }

    pub(crate) fn after(&mut self, pass: PassId) -> &mut Self {
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
        f: impl FnOnce(&mut Recorder<'_>) -> Result<(), GraphError>,
    ) -> Result<&mut Self, GraphError> {
        todo!()
    }

    pub(crate) fn finish(self) -> Result<PassId, GraphError> {
        todo!()
    }

    fn pass_mut(&mut self) -> &mut Pass {
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

    fn check(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_directions(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_queue_flags(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_usage(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_sharing(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn merge_declarations(&mut self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_tokens(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_wgpu_state(&self) -> Result<(), GraphError> {
        todo!()
    }

    fn check_session_reset(&self) -> Result<(), GraphError> {
        todo!()
    }
}

impl Drop for PassBuilder<'_> {
    fn drop(&mut self) {
        todo!()
    }
}
