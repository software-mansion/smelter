use std::sync::{Arc, atomic::AtomicU64};

use ash::vk;

use crate::backends::vulkan::wrappers::{ImageView, ResultQueryData};

use super::{
    BufferRange, DeclaredResource, ImageLayer, Picture, QueryRef, SubresourceKey, TaskError,
};

#[derive(Clone)]
pub(crate) struct Produced<R: Resource> {
    pub(super) task: TaskId,
    pub(super) step: StepId,
    pub(super) version: u32,
    pub(super) resource: R,
}

impl<R: Resource> Produced<R> {
    pub(crate) fn resource(&self) -> &R {
        &self.resource
    }
}

impl Produced<ImageLayer> {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Produced<Picture>, TaskError> {
        Ok(Produced {
            task: self.task,
            step: self.step,
            version: self.version,
            resource: self.resource.with_view(view, coded_extent)?,
        })
    }
}

impl From<Produced<Picture>> for Produced<ImageLayer> {
    fn from(picture: Produced<Picture>) -> Self {
        Produced {
            task: picture.task,
            version: picture.version,
            step: picture.step,
            resource: picture.resource.layer,
        }
    }
}

pub(crate) enum Input<R: Resource> {
    Current(R),
    Produced(Produced<R>),
}

impl<R: Resource> From<R> for Input<R> {
    fn from(resource: R) -> Self {
        Input::Current(resource)
    }
}

impl<R: Resource> From<&Produced<R>> for Input<R> {
    fn from(produced: &Produced<R>) -> Self {
        Input::Produced(produced.clone())
    }
}

impl<R: Resource> From<Produced<R>> for Input<R> {
    fn from(produced: Produced<R>) -> Self {
        Input::Produced(produced)
    }
}

impl<R: Resource> Input<R> {
    pub(crate) fn resource(&self) -> &R {
        match self {
            Input::Current(resource) => resource,
            Input::Produced(produced) => &produced.resource,
        }
    }
}

impl Input<ImageLayer> {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Input<Picture>, TaskError> {
        let result = match self {
            Input::Current(layer) => Input::Current(layer.with_view(view, coded_extent)?),
            Input::Produced(produced) => Input::Produced(produced.with_view(view, coded_extent)?),
        };

        Ok(result)
    }
}

mod sealed {
    pub(crate) trait Sealed {}
}

pub(crate) trait Resource: sealed::Sealed + Clone + Send + Sync + 'static {
    fn declared(&self) -> DeclaredResource;
    fn subresource_key(&self) -> SubresourceKey;
}

pub(crate) trait HostReadable: Resource {}

impl sealed::Sealed for ImageLayer {}
impl sealed::Sealed for Picture {}
impl sealed::Sealed for BufferRange {}
impl<T: ResultQueryData + Send + Sync + 'static> sealed::Sealed for QueryRef<T> {}

impl Resource for ImageLayer {
    fn declared(&self) -> DeclaredResource {
        DeclaredResource::Image {
            range: self.clone(),
            view: None,
        }
    }

    fn subresource_key(&self) -> SubresourceKey {
        self.key()
    }
}

impl Resource for Picture {
    fn declared(&self) -> DeclaredResource {
        DeclaredResource::Image {
            range: self.layer.clone(),
            view: Some(self.view.clone()),
        }
    }

    fn subresource_key(&self) -> SubresourceKey {
        self.layer.key()
    }
}

impl Resource for BufferRange {
    fn declared(&self) -> DeclaredResource {
        DeclaredResource::Buffer(self.clone())
    }

    fn subresource_key(&self) -> SubresourceKey {
        self.key()
    }
}

impl<T: ResultQueryData + Send + Sync + 'static> Resource for QueryRef<T> {
    fn declared(&self) -> DeclaredResource {
        let (pool, index) = self.query.pool_and_index();
        DeclaredResource::Query {
            pool,
            index,
            owner: self.query.clone(),
        }
    }

    fn subresource_key(&self) -> SubresourceKey {
        self.key()
    }
}

impl HostReadable for BufferRange {}
impl<T: ResultQueryData + Send + Sync + 'static> HostReadable for QueryRef<T> {}

#[derive(Clone)]
pub(crate) struct HostRead<R: HostReadable> {
    pub(super) produced: Produced<R>,
}

#[must_use = "pass the token to Recorder::coding"]
pub(crate) struct SessionToken {
    pub(super) task: TaskId,
    pub(super) step: StepId,
    pub(super) needs_reset: bool,
}

impl SessionToken {
    pub(crate) fn needs_reset(&self) -> bool {
        self.needs_reset
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StepId(pub(super) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TaskId(pub(super) u64);

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(0);

impl TaskId {
    pub(super) fn next() -> Self {
        TaskId(NEXT_TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}
