use std::sync::Arc;

use ash::vk;

use crate::backends::vulkan::wrappers::{Device, VideoSession, VideoSessionParameters};

use super::{BufferRange, ImageRange, Step, TaskError};

pub(crate) struct Recorder<'a> {
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) device: &'a Device,
    pub(super) step: &'a mut Step,
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
        step: &'a mut Step,
    ) -> Self {
        todo!()
    }

    pub(crate) fn command_buffer(&self) -> vk::CommandBuffer {
        todo!()
    }

    pub(crate) fn device(&self) -> &Device {
        todo!()
    }

    pub(crate) fn layout_of(&self, range: &ImageRange) -> Result<vk::ImageLayout, TaskError> {
        todo!()
    }

    pub(crate) fn view(&self, range: &ImageRange) -> Result<vk::ImageView, TaskError> {
        todo!()
    }

    pub(crate) fn buffer(&self, range: &BufferRange) -> Result<vk::Buffer, TaskError> {
        todo!()
    }

    pub(crate) fn begin_coding(&mut self, info: BeginCoding<'_>) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn control_reset(
        &mut self,
        info: &vk::VideoCodingControlInfoKHR<'_>,
    ) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn reference(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn setup(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn picture_resource(
        &self,
        range: &ImageRange,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn decode(&mut self, info: &vk::VideoDecodeInfoKHR<'_>) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn encode(&mut self, info: &vk::VideoEncodeInfoKHR<'_>) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn end_coding(&mut self) -> Result<(), TaskError> {
        todo!()
    }

    fn reference_slots(
        &self,
        active: u32,
        order: Option<&[u32]>,
    ) -> Result<Vec<vk::VideoReferenceSlotInfoKHR<'static>>, TaskError> {
        todo!()
    }
}
