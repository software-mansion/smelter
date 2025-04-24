use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{
    wrappers::{CommandBuffer, Image, Semaphore},
    VulkanCtxError, VulkanDevice,
};

use super::H264EncodeProfileInfo;

#[derive(Debug, thiserror::Error)]
pub enum YuvConverterError {
    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),
}

pub(crate) struct Converter {
    device: Arc<VulkanDevice>,
    image: Arc<Mutex<Image>>,
}

impl Converter {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        width: u32,
        height: u32,
        profile: &H264EncodeProfileInfo,
        command_buffer: &CommandBuffer,
    ) -> Result<Self, YuvConverterError> {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let queue_indices =
            [device.queues.h264_encode.idx, device.queues.wgpu.idx].map(|i| i as u32);

        let create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR,
            )
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(&queue_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut profile_list_info);

        let mut image = Image::new(device.allocator.clone(), &create_info)?;

        image.transition_layout_single_layer(
            command_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE..vk::AccessFlags2::NONE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            0,
        )?;

        let vk_image = image.image;
        let image = Arc::new(Mutex::new(image));

        let module =
            wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/rgba_to_yuv.wgsl"))
                .unwrap();
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );

        validator
            .subgroup_stages(wgpu::naga::valid::ShaderStages::all())
            .subgroup_operations(wgpu::naga::valid::SubgroupOperationSet::all());

        let module_info = validator.validate(&module).unwrap();

        let compiled_vertex = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "vs_main".into(),
                shader_stage: wgpu::naga::ShaderStage::Vertex,
            }),
        ).unwrap();

        let compiled_fragment = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "fs_main".into(),
                shader_stage: wgpu::naga::ShaderStage::Fragment,
            }),
        ).unwrap();

        let create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::empty())
            .bindings(bindings);

        device
            .device
            .create_descriptor_set_layout(create_info, None);

        let create_info = vk::PipelineLayoutCreateInfo::default()
            .flags(vk::PipelineLayoutCreateFlags::empty())
            .set_layouts(set_layouts);
        let layout = device.device.create_pipeline_layout(create_info, None)?;

        Ok(Self { device, image })
    }

    /// The returned image is NV12 with encoding layout
    pub(crate) fn convert(
        &self,
        texture: wgpu::Texture,
        signal_semaphores: &[&Semaphore],
        profile: &H264EncodeProfileInfo,
    ) -> Result<Image, YuvConverterError> {
        todo!()
    }
}
