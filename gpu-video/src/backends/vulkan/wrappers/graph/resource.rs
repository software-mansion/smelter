use std::{
    any::Any,
    ops::Range,
    sync::{Arc, Mutex},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{
    Buffer, CodingImageBundle, DecodeInputBuffer, DecodedPicturesBuffer, EncodeInputImage,
    EncodeOutputBuffer, Image, ImageView, ResultQuery, ResultQueryData, VideoSession,
};

use super::{GraphError, GraphId, Sharing, Timeline};

#[derive(Clone)]
pub(crate) struct ImageRange {
    pub(crate) image: Arc<Image>,
    pub(crate) base_layer: u32,
    pub(crate) layer_count: u32,
    pub(super) view: Option<ViewRef>,
    pub(super) owner: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Clone)]
pub(crate) struct ViewRef {
    pub(super) view: vk::ImageView,
    pub(super) picture_base_layer: u32,
    pub(super) coded_extent: vk::Extent2D,
    pub(super) owner: Arc<dyn Any + Send + Sync>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RangeKey {
    image: *const Image,
    base_layer: u32,
    layer_count: u32,
}

unsafe impl Send for RangeKey {}
unsafe impl Sync for RangeKey {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SubresourceKey {
    ImageLayer { image: *const Image, layer: u32 },
    Buffer(*const Buffer),
    Query { pool: vk::QueryPool, index: u32 },
}

unsafe impl Send for SubresourceKey {}
unsafe impl Sync for SubresourceKey {}

impl ImageRange {
    pub(crate) fn with_view(
        self,
        view: Arc<ImageView>,
        coded_extent: vk::Extent2D,
    ) -> Result<Self, GraphError> {
        todo!()
    }

    pub(super) fn key(&self) -> RangeKey {
        todo!()
    }

    pub(super) fn layer_keys(&self) -> impl Iterator<Item = SubresourceKey> + '_ {
        (self.base_layer..self.base_layer + self.layer_count).map(|layer| {
            SubresourceKey::ImageLayer {
                image: Arc::as_ptr(&self.image),
                layer,
            }
        })
    }

    pub(super) fn picture_resource(
        &self,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, GraphError> {
        todo!()
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
        todo!()
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
        todo!()
    }
}

impl<T: ResultQueryData> ResultQuery<T> {
    pub(crate) fn pool_and_index(&self) -> (vk::QueryPool, u32) {
        todo!()
    }
}

impl Image {
    pub(crate) fn layer(self: &Arc<Self>, layer: u32) -> ImageRange {
        todo!()
    }

    pub(crate) fn layers(self: &Arc<Self>, layers: Range<u32>) -> ImageRange {
        todo!()
    }

    pub(crate) fn all_layers(self: &Arc<Self>) -> ImageRange {
        todo!()
    }

    pub(crate) fn state(&self) -> &ResourceState {
        todo!()
    }
}

impl Buffer {
    pub(crate) fn whole(self: &Arc<Self>) -> BufferRange {
        todo!()
    }

    pub(crate) fn range(self: &Arc<Self>, offset: u64, size: u64) -> BufferRange {
        todo!()
    }

    pub(crate) fn state(&self) -> &ResourceState {
        todo!()
    }
}

impl DecodeInputBuffer {
    pub(crate) fn range(self: &Arc<Self>) -> BufferRange {
        todo!()
    }

    pub(super) fn raw(&self) -> vk::Buffer {
        todo!()
    }
}

impl EncodeOutputBuffer {
    pub(crate) fn range(self: &Arc<Self>) -> BufferRange {
        todo!()
    }

    pub(super) fn raw(&self) -> vk::Buffer {
        todo!()
    }
}

impl EncodeInputImage {
    pub(crate) fn layer(self: &Arc<Self>, layer: u32) -> ImageRange {
        todo!()
    }

    pub(crate) fn image(&self) -> &Arc<Image> {
        todo!()
    }
}

impl CodingImageBundle<'_> {
    pub(crate) fn slot(&self, i: u32) -> ImageRange {
        todo!()
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = ImageRange> + '_ {
        (0..self.video_resource_info.len() as u32).map(|i| self.slot(i))
    }
}

impl DecodedPicturesBuffer<'_> {
    pub(crate) fn slot(&self, i: u32) -> ImageRange {
        todo!()
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = ImageRange> + '_ {
        self.image.slots()
    }
}

#[derive(Clone)]
pub(crate) struct SubresourceState {
    pub(crate) layout: vk::ImageLayout,
    pub(crate) last_write: Option<AccessRecord>,
    pub(crate) reads_since: Vec<AccessRecord>,
    pub(crate) owner: Option<u32>,
    pub(crate) exported: bool,
    pub(crate) lent_to_wgpu: bool,
}

impl SubresourceState {
    pub(super) fn fresh() -> Self {
        todo!()
    }

    pub(super) fn merge_read(&mut self, record: AccessRecord) {
        todo!()
    }

    pub(super) fn prune_reached(&mut self) {
        todo!()
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
        todo!()
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
        todo!()
    }
}

impl ResourceState {
    pub(crate) fn new(sharing: Sharing, usage: ResourceUsage, subresource_count: u32) -> Self {
        todo!()
    }

    pub(crate) fn reset_for_reuse(&self) -> bool {
        todo!()
    }

    pub(crate) fn is_lent_to_wgpu(&self) -> bool {
        todo!()
    }

    pub(crate) fn lend_to_wgpu(&self) {
        todo!()
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, Box<[SubresourceState]>> {
        todo!()
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

    pub(super) fn claim_for_graph(&self, graph: GraphId) -> Result<(), GraphError> {
        todo!()
    }

    pub(super) fn release_from_graph(&self, graph: GraphId) {
        todo!()
    }
}
