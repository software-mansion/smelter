use ash::vk;

use crate::backends::vulkan::wrappers::Device;

use super::{BufferRange, ImageLayer, Picture, SessionToken, Step, TaskError, TaskId};

pub(crate) struct Recorder<'a> {
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) device: &'a Device,
    pub(super) task: TaskId,
    pub(super) step: &'a mut Step,
}

pub(crate) struct BeginCoding<'a> {
    pub(crate) active: u32,
    pub(crate) order: Option<&'a [u32]>,
    pub(crate) p_next: Option<&'a mut dyn vk::ExtendsVideoBeginCodingInfoKHR>,
    pub(crate) reset: Option<vk::VideoCodingControlInfoKHR<'a>>,
}

pub(crate) struct CodingScope<'r, 'a> {
    recorder: &'r mut Recorder<'a>,
}

impl<'a> Recorder<'a> {
    pub(super) fn new(
        command_buffer: vk::CommandBuffer,
        device: &'a Device,
        task: TaskId,
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

    pub(crate) fn layout_of(&self, range: &ImageLayer) -> Result<vk::ImageLayout, TaskError> {
        todo!()
    }

    pub(crate) fn view(&self, picture: &Picture) -> Result<vk::ImageView, TaskError> {
        todo!()
    }

    pub(crate) fn buffer(&self, range: &BufferRange) -> Result<vk::Buffer, TaskError> {
        todo!()
    }

    pub(crate) fn coding<T>(
        &mut self,
        session: SessionToken,
        begin: BeginCoding<'_>,
        body: impl FnOnce(&mut CodingScope<'_, 'a>) -> Result<T, TaskError>,
    ) -> Result<T, TaskError> {
        todo!()
    }

    fn check_session_token(&self, session: &SessionToken) -> Result<(), TaskError> {
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

impl CodingScope<'_, '_> {
    pub(crate) fn command_buffer(&self) -> vk::CommandBuffer {
        todo!()
    }

    pub(crate) fn device(&self) -> &Device {
        todo!()
    }

    pub(crate) fn reference(
        &self,
        picture: &Picture,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn setup(
        &self,
        picture: &Picture,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn picture_resource(
        &self,
        picture: &Picture,
    ) -> Result<vk::VideoPictureResourceInfoKHR<'static>, TaskError> {
        todo!()
    }

    pub(crate) fn decode(&mut self, info: &vk::VideoDecodeInfoKHR<'_>) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn encode(&mut self, info: &vk::VideoEncodeInfoKHR<'_>) -> Result<(), TaskError> {
        todo!()
    }
}
