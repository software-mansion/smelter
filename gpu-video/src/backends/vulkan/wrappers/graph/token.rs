use std::sync::{Arc, atomic::AtomicU64};

use ash::vk;

use crate::backends::vulkan::wrappers::{ImageView, ResultQueryData};

use super::{BufferRange, DeclaredResource, GraphError, ImageRange, QueryRef, SubresourceKey};

#[derive(Clone)]
pub(crate) struct Produced<R: Resource> {
    pub(super) graph: GraphId,
    pub(super) pass: PassId,
    pub(super) version: u32,
    pub(super) resource: R,
}

impl<R: Resource> Produced<R> {
    pub(crate) fn resource(&self) -> &R {
        todo!()
    }
}

impl Produced<ImageRange> {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Self, GraphError> {
        todo!()
    }
}

pub(crate) enum Input<R: Resource> {
    Current(R),
    Produced(Produced<R>),
}

impl<R: Resource> From<R> for Input<R> {
    fn from(resource: R) -> Self {
        todo!()
    }
}

impl<R: Resource> From<&Produced<R>> for Input<R> {
    fn from(produced: &Produced<R>) -> Self {
        todo!()
    }
}

impl<R: Resource> From<Produced<R>> for Input<R> {
    fn from(produced: Produced<R>) -> Self {
        todo!()
    }
}

impl<R: Resource> Input<R> {
    pub(crate) fn resource(&self) -> &R {
        todo!()
    }
}

impl Input<ImageRange> {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Self, GraphError> {
        todo!()
    }
}

mod sealed {
    pub(crate) trait Sealed {}
}

pub(crate) trait Resource: sealed::Sealed + Clone + Send + Sync + 'static {
    fn declared(&self) -> DeclaredResource;
    fn subresource_keys(&self) -> Vec<SubresourceKey>;
}

pub(crate) trait HostReadable: Resource {}

impl sealed::Sealed for ImageRange {}
impl sealed::Sealed for BufferRange {}
impl<T: ResultQueryData + Send + Sync + 'static> sealed::Sealed for QueryRef<T> {}

impl Resource for ImageRange {
    fn declared(&self) -> DeclaredResource {
        todo!()
    }

    fn subresource_keys(&self) -> Vec<SubresourceKey> {
        todo!()
    }
}

impl Resource for BufferRange {
    fn declared(&self) -> DeclaredResource {
        todo!()
    }

    fn subresource_keys(&self) -> Vec<SubresourceKey> {
        todo!()
    }
}

impl<T: ResultQueryData + Send + Sync + 'static> Resource for QueryRef<T> {
    fn declared(&self) -> DeclaredResource {
        todo!()
    }

    fn subresource_keys(&self) -> Vec<SubresourceKey> {
        todo!()
    }
}

impl HostReadable for BufferRange {}
impl<T: ResultQueryData + Send + Sync + 'static> HostReadable for QueryRef<T> {}

#[derive(Clone)]
pub(crate) struct HostRead<R: HostReadable> {
    pub(super) produced: Produced<R>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PassId(pub(super) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GraphId(pub(super) u64);

static NEXT_GRAPH_ID: AtomicU64 = AtomicU64::new(0);

impl GraphId {
    pub(super) fn next() -> Self {
        todo!()
    }
}
