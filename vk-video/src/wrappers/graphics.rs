use std::sync::Arc;

use ash::vk;

use crate::VulkanCtxError;

use super::Device;

pub(crate) struct ShaderModule {
    device: Arc<Device>,
    pub(crate) module: vk::ShaderModule,
}

impl ShaderModule {
    pub(crate) fn new(device: Arc<Device>, code: &[u32]) -> Result<Self, VulkanCtxError> {
        let module = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None)?
        };

        Ok(Self { device, module })
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.destroy_shader_module(self.module, None) };
    }
}

pub(crate) struct DescriptorSetLayout {
    device: Arc<Device>,
    pub(crate) set_layout: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorSetLayoutCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let set_layout = unsafe { device.create_descriptor_set_layout(create_info, None)? };

        Ok(Self { device, set_layout })
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

pub(crate) struct PipelineLayout {
    device: Arc<Device>,
    pub(crate) pipeline_layout: vk::PipelineLayout,
}

impl PipelineLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::PipelineLayoutCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let pipeline_layout = unsafe { device.create_pipeline_layout(create_info, None)? };

        Ok(Self {
            device,
            pipeline_layout,
        })
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

pub(crate) struct RenderPass {
    device: Arc<Device>,
    pub(crate) render_pass: vk::RenderPass,
}

impl RenderPass {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::RenderPassCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let render_pass = unsafe { device.device.create_render_pass(create_info, None)? };

        Ok(Self {
            device,
            render_pass,
        })
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device
                .destroy_render_pass(self.render_pass, None);
        }
    }
}
