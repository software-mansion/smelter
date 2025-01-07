use std::sync::Arc;

use ash::Entry;

mod command;
mod debug;
mod mem;
mod parameter_sets;
mod sync;
mod video;
mod vk_extensions;

pub(crate) use command::*;
pub(crate) use debug::*;
pub(crate) use mem::*;
pub(crate) use parameter_sets::*;
pub(crate) use sync::*;
pub(crate) use video::*;
pub(crate) use vk_extensions::*;

pub(crate) struct Instance {
    pub(crate) instance: ash::Instance,
    pub(crate) _entry: Arc<Entry>,
    pub(crate) video_queue_instance_ext: ash::khr::video_queue::Instance,
    pub(crate) video_encode_queue_instance_ext: ash::khr::video_encode_queue::Instance,
    pub(crate) debug_utils_instance_ext: ash::ext::debug_utils::Instance,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { self.destroy_instance(None) };
    }
}

impl std::ops::Deref for Instance {
    type Target = ash::Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

pub(crate) struct Device {
    pub(crate) device: ash::Device,
    pub(crate) video_queue_ext: ash::khr::video_queue::Device,
    pub(crate) video_decode_queue_ext: ash::khr::video_decode_queue::Device,
    pub(crate) _instance: Arc<Instance>,
}

impl std::ops::Deref for Device {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { self.destroy_device(None) };
    }
}
