use std::sync::Arc;

use ash::vk;

use crate::VulkanCommonError;

use super::Device;

pub(crate) struct DescriptorSetLayout {
    device: Arc<Device>,
    pub(crate) set_layout: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorSetLayoutCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let set_layout = unsafe { device.create_descriptor_set_layout(create_info, None)? };

        Ok(Self { device, set_layout })
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.set_layout, None)
        };
    }
}

pub(crate) struct DescriptorPool {
    device: Arc<Device>,
    pub(crate) pool: vk::DescriptorPool,
}

impl DescriptorPool {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorPoolCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let pool = unsafe { device.create_descriptor_pool(create_info, None)? };
        Ok(Self { device, pool })
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}

pub(crate) struct PipelineLayout {
    layout: vk::PipelineLayout,
    device: Arc<Device>,
    descriptor_set_layouts: Vec<Arc<DescriptorSetLayout>>,
}

impl PipelineLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::PipelineLayoutCreateInfo,
        descriptor_set_layouts: Vec<Arc<DescriptorSetLayout>>,
    ) -> Result<Self, VulkanCommonError> {
        let layout = unsafe { device.create_pipeline_layout(create_info, None)? };

        Ok(Self {
            layout,
            device,
            descriptor_set_layouts,
        })
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline_layout(self.layout, None) };
    }
}
