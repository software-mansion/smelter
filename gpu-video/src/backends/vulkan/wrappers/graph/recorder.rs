use std::sync::Arc;

use ash::vk;

use crate::backends::vulkan::wrappers::{Device, VideoSession, VideoSessionParameters};

use super::{BufferRange, GraphError, ImageRange, Pass};

pub(crate) struct Recorder<'a> {
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) device: &'a Device,
    pub(super) pass: &'a mut Pass,
    pub(super) scope_open: bool,
    pub(super) coding_op_recorded: bool,
}

pub(crate) struct BeginCoding<'a> {
    pub(crate) session: &'a Arc<VideoSession>,
    pub(crate) parameters: &'a Arc<VideoSessionParameters>,
    pub(crate) active: u32,
    pub(crate) order: Option<&'a [u32]>,
    pub(crate) p_next: Option<&'a mut dyn vk::ExtendsVideoBeginCodingInfoKHR>,
}

impl<'a> Recorder<'a> {
    pub(super) fn new(
        command_buffer: vk::CommandBuffer,
        device: &'a Device,
        pass: &'a mut Pass,
    ) -> Self {
        todo!()
    }

    pub(crate) fn command_buffer(&self) -> vk::CommandBuffer {
        todo!()
    }

    pub(crate) fn device(&self) -> &Device {
        todo!()
    }

    pub(crate) fn layout_of(&self, range: &ImageRange) -> Result<vk::ImageLayout, GraphError> {
        todo!()
    }

    pub(crate) fn view(&self, range: &ImageRange) -> Result<vk::ImageView, GraphError> {
        todo!()
    }

    pub(crate) fn buffer(&self, range: &BufferRange) -> Result<vk::Buffer, GraphError> {
        todo!()
    }

    pub(crate) fn begin_coding(&mut self, info: BeginCoding<'_>) -> Result<(), GraphError> {
        todo!()
    }

    pub(crate) fn control_reset(
        &mut self,
        info: &vk::VideoCodingControlInfoKHR<'_>,
    ) -> Result<(), GraphError> {
        todo!()
    }

    pub(crate) fn reference(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, GraphError> {
        todo!()
    }

    pub(crate) fn setup(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, GraphError> {
        todo!()
    }

    pub(crate) fn picture_resource(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, GraphError> {
        todo!()
    }

    pub(crate) fn decode(&mut self, info: &vk::VideoDecodeInfoKHR<'_>) -> Result<(), GraphError> {
        todo!()
    }

    pub(crate) fn encode(&mut self, info: &vk::VideoEncodeInfoKHR<'_>) -> Result<(), GraphError> {
        todo!()
    }

    pub(crate) fn end_coding(&mut self) -> Result<(), GraphError> {
        todo!()
    }

    fn reference_slots(
        &self,
        active: u32,
        order: Option<&[u32]>,
    ) -> Result<Vec<vk::VideoReferenceSlotInfoKHR<'static>>, GraphError> {
        todo!()
    }
}
