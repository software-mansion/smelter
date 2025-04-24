use std::sync::Arc;

use ash::vk;

use crate::{wrappers::ImageView, VulkanCommonError};

use super::Device;

pub(crate) struct ShaderModule {
    device: Arc<Device>,
    pub(crate) module: vk::ShaderModule,
}

impl ShaderModule {
    pub(crate) fn new(device: Arc<Device>, code: &[u32]) -> Result<Self, VulkanCommonError> {
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
    ) -> Result<Self, VulkanCommonError> {
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
    ) -> Result<Self, VulkanCommonError> {
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
    ) -> Result<Self, VulkanCommonError> {
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

pub(crate) struct Pipeline {
    pub(crate) pipeline: vk::Pipeline,
    _render_pass: Arc<RenderPass>,
    _layout: Arc<PipelineLayout>,
    device: Arc<Device>,
}

impl Pipeline {
    pub(crate) fn new(
        device: Arc<Device>,
        render_pass: Arc<RenderPass>,
        layout: Arc<PipelineLayout>,
        create_info: &vk::GraphicsPipelineCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let pipeline = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(create_info),
                    None,
                )
                .map_err(|(_, err)| err)?
        }[0];

        Ok(Self {
            pipeline,
            _render_pass: render_pass,
            _layout: layout,
            device,
        })
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

pub(crate) struct Framebuffer {
    pub(crate) framebuffer: vk::Framebuffer,
    _attachments: Vec<Arc<ImageView>>,
    _render_pass: Arc<RenderPass>,
    device: Arc<Device>,
}

impl Framebuffer {
    pub(crate) fn new(
        device: Arc<Device>,
        render_pass: Arc<RenderPass>,
        attachments: Vec<Arc<ImageView>>,
        create_info: &vk::FramebufferCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let framebuffer = unsafe { device.create_framebuffer(create_info, None)? };

        Ok(Self {
            framebuffer,
            _attachments: attachments,
            _render_pass: render_pass,
            device,
        })
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_framebuffer(self.framebuffer, None);
        }
    }
}

pub(crate) struct Sampler {
    pub(crate) sampler: vk::Sampler,
    device: Arc<Device>,
}

impl Sampler {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::SamplerCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let sampler = unsafe { device.create_sampler(create_info, None)? };

        Ok(Self { sampler, device })
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

pub(crate) struct DescriptorPool {
    pub(crate) pool: vk::DescriptorPool,
    device: Arc<Device>,
}

impl DescriptorPool {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorPoolCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let pool = unsafe { device.create_descriptor_pool(create_info, None)? };

        Ok(Self { pool, device })
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}
