use std::{
    any::Any,
    sync::{Arc, Mutex, PoisonError},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{
    Buffer, CodingImageBundle, DecodeInputBuffer, DecodedPicturesBuffer, EncodeInputImage,
    EncodeOutputBuffer, Image, ImageView, ResultQuery, ResultQueryData, VideoSession,
};

use super::{Sharing, TaskError, TaskId, Timeline};

#[derive(Clone)]
pub(crate) struct ImageLayer {
    pub(crate) image: Arc<Image>,
    pub(crate) layer: u32,
    pub(super) owner: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Clone)]
pub(crate) struct Picture {
    pub(crate) layer: ImageLayer,
    pub(super) view: ViewRef,
}

#[derive(Clone)]
pub(crate) struct ViewRef {
    pub(super) view: vk::ImageView,
    pub(super) layer_in_view: u32,
    pub(super) coded_extent: vk::Extent2D,
    pub(super) owner: Arc<dyn Any + Send + Sync>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SubresourceKey {
    ImageLayer { image: vk::Image, layer: u32 },
    Buffer(vk::Buffer),
    Query { pool: vk::QueryPool, index: u32 },
}

impl ImageLayer {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Picture, TaskError> {
        if !Arc::ptr_eq(&self.image, &view.image) {
            return Err(TaskError::ViewImageMismatch);
        }

        let view_base = view.subresource_range.base_array_layer;
        let view_range = view_base..view_base + view.subresource_range.layer_count;

        if !view_range.contains(&self.layer) {
            return Err(TaskError::ViewImageMismatch);
        }

        Ok(Picture {
            view: ViewRef {
                view: view.view,
                layer_in_view: self.layer - view_base,
                coded_extent,
                owner: view,
            },
            layer: self,
        })
    }

    pub(super) fn key(&self) -> SubresourceKey {
        SubresourceKey::ImageLayer {
            image: self.image.image,
            layer: self.layer,
        }
    }
}

impl Picture {
    pub(super) fn picture_resource(&self) -> vk::VideoPictureResourceInfoKHR<'static> {
        todo!()
    }
}

impl From<Picture> for ImageLayer {
    fn from(picture: Picture) -> Self {
        picture.layer
    }
}

#[derive(Clone)]
pub(crate) struct BufferRange {
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(super) owner: Option<Arc<dyn Any + Send + Sync>>,
}

impl BufferRange {
    pub(super) fn key(&self) -> SubresourceKey {
        SubresourceKey::Buffer(self.buffer.buffer)
    }
}

pub(crate) struct QueryRef<T: ResultQueryData> {
    pub(crate) query: Arc<ResultQuery<T>>,
}

impl<T: ResultQueryData> Clone for QueryRef<T> {
    fn clone(&self) -> Self {
        Self {
            query: self.query.clone(),
        }
    }
}

impl<T: ResultQueryData> QueryRef<T> {
    pub(super) fn key(&self) -> SubresourceKey {
        let (pool, index) = self.query.pool_and_index();
        SubresourceKey::Query { pool, index }
    }
}

impl Image {
    pub(crate) fn layer(self: &Arc<Self>, layer: u32) -> ImageLayer {
        todo!()
    }

    pub(crate) fn state(&self) -> &ResourceState {
        &self.state
    }
}

impl Buffer {
    pub(crate) fn whole(self: &Arc<Self>) -> BufferRange {
        BufferRange {
            buffer: self.clone(),
            size: self.size,
            offset: 0,
            owner: None,
        }
    }

    pub(crate) fn range(self: &Arc<Self>, offset: u64, size: u64) -> Result<BufferRange, TaskError> {
        if offset + size > self.size {
            return Err(TaskError::BufferRangeOutOfBounds { offset, range_size: size, buffer_size: self.size });
        }

        Ok(BufferRange {
            buffer: self.clone(),
            size, offset,
            owner: None,
        })
    }

    pub(crate) fn state(&self) -> &ResourceState {
        &self.state
    }
}

impl EncodeInputImage {
    pub(crate) fn layer(self: &Arc<Self>, layer: u32) -> ImageLayer {
        todo!()
    }

    pub(crate) fn image(&self) -> &Arc<Image> {
        todo!()
    }
}

impl CodingImageBundle<'_> {
    pub(crate) fn slot(&self, i: u32) -> Picture {
        todo!()
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = Picture> + '_ {
        (0..self.video_resource_info.len() as u32).map(|i| self.slot(i))
    }
}

impl DecodedPicturesBuffer<'_> {
    pub(crate) fn slot(&self, i: u32) -> Picture {
        todo!()
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = Picture> + '_ {
        self.image.slots()
    }
}

#[derive(Clone)]
pub(crate) struct SubresourceState {
    pub(crate) layout: vk::ImageLayout,
    pub(crate) last_write: Option<AccessRecord>,
    pub(crate) reads_since: Vec<AccessRecord>,
    pub(crate) owned_by_family: Option<u32>,
    pub(crate) exported: bool,
    pub(crate) lent_to_wgpu: bool,
}

impl SubresourceState {
    pub(super) fn fresh() -> Self {
        Self {
            layout: vk::ImageLayout::UNDEFINED,
            last_write: None,
            reads_since: Vec::new(),
            owned_by_family: None,
            exported: false,
            lent_to_wgpu: false,
        }
    }

    pub(super) fn merge_read(&mut self, record: AccessRecord) {
        let slot = self
            .reads_since
            .iter_mut()
            .find(|other| Arc::ptr_eq(&other.timeline, &record.timeline));

        if let Some(slot) = slot {
            slot.value = slot.value.max(record.value);
            slot.stage |= record.stage;
            slot.access |= record.access;
        } else {
            self.reads_since.push(record);
        }
    }

    pub(super) fn prune_reached(&mut self) {
        self.reads_since.retain(|r| !r.is_reached())
    }
}

#[derive(Clone)]
pub(crate) struct AccessRecord {
    pub(crate) timeline: Arc<Timeline>,
    pub(crate) value: u64,
    pub(crate) family: u32,
    pub(crate) stage: vk::PipelineStageFlags2,
    pub(crate) access: vk::AccessFlags2,
}

impl AccessRecord {
    pub(super) fn is_reached(&self) -> bool {
        self.timeline.completed() >= self.value
    }
}

pub(crate) struct ResourceState {
    pub(crate) sharing: Sharing,
    pub(crate) usage: ResourceUsage,
    pub(crate) subresources: Mutex<Box<[SubresourceState]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceUsage {
    Image(vk::ImageUsageFlags),
    Buffer(vk::BufferUsageFlags),
    Session,
}

impl ResourceUsage {
    pub(super) fn contains(self, required: ResourceUsage) -> bool {
        match (self, required) {
            (ResourceUsage::Image(own), ResourceUsage::Image(required)) => own.contains(required),
            (ResourceUsage::Buffer(own), ResourceUsage::Buffer(required)) => own.contains(required),
            (ResourceUsage::Session, ResourceUsage::Session) => true,
            _ => false,
        }
    }
}

impl ResourceState {
    pub(crate) fn new(sharing: Sharing, usage: ResourceUsage, subresource_count: u32) -> Self {
        Self {
            sharing,
            usage,
            subresources: Mutex::new(
                vec![SubresourceState::fresh(); subresource_count as usize].into_boxed_slice(),
            ),
        }
    }

    /// returns whether all waits are done
    pub(crate) fn reset_for_reuse(&self) -> bool {
        let mut lock = self.lock();
        let mut timelines: Vec<&Arc<Timeline>> = Vec::new();

        for state in lock.iter() {
            for access in state.reads_since.iter().chain(state.last_write.iter()) {
                if !timelines.iter().any(|t| Arc::ptr_eq(&access.timeline, t)) {
                    timelines.push(&access.timeline)
                }
            }
        }

        for timeline in timelines {
            _ = timeline.refresh();
        }

        let mut all_clear = true;
        for state in lock.iter_mut() {
            state.prune_reached();
            if let Some(write) = &state.last_write
                && write.is_reached()
            {
                state.last_write = None;
            }

            if state.last_write.is_some() || !state.reads_since.is_empty() {
                all_clear = false
            }
        }

        all_clear
    }

    pub(crate) fn is_lent_to_wgpu(&self) -> bool {
        self.lock().iter().any(|s| s.lent_to_wgpu)
    }

    pub(crate) fn lend_to_wgpu(&self) {
        for state in self.lock().iter_mut() {
            state.lent_to_wgpu = true;
        }
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, Box<[SubresourceState]>> {
        self.subresources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl VideoSession {
    pub(crate) fn is_poisoned(&self) -> bool {
        todo!()
    }

    pub(crate) fn poison(&self) {
        todo!()
    }

    pub(crate) fn family_index(&self) -> u32 {
        todo!()
    }

    pub(crate) fn state(&self) -> &ResourceState {
        todo!()
    }

    pub(super) fn clear_poison(&self) {
        todo!()
    }

    pub(super) fn claim_for_task(&self, task: TaskId) -> Result<(), TaskError> {
        todo!()
    }

    pub(super) fn release_from_task(&self, task: TaskId) {
        todo!()
    }
}
