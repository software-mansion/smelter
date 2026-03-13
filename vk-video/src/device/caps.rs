use std::ffi::c_void;
use std::ptr::null_mut;

use ash::vk;

use crate::VulkanDecoderError;
use crate::VulkanInitError;
use crate::codec::Codec;
use crate::codec::CodecSpecificEncoderQualityLevelProperties as _;
use crate::codec::h264::H264Codec;
use crate::parameters::H264Profile;
use crate::wrappers::*;

pub(crate) fn query_video_format_properties<'a>(
    device: vk::PhysicalDevice,
    video_queue_instance_ext: &ash::khr::video_queue::Instance,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    image_usage: vk::ImageUsageFlags,
) -> Result<Vec<vk::VideoFormatPropertiesKHR<'a>>, VulkanInitError> {
    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));

    let format_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
        .image_usage(image_usage)
        .push_next(&mut profile_list_info);

    let mut format_info_length = 0;

    unsafe {
        (video_queue_instance_ext
            .fp()
            .get_physical_device_video_format_properties_khr)(
            device,
            &format_info,
            &mut format_info_length,
            std::ptr::null_mut(),
        )
        .result()?;
    }

    let mut format_properties =
        vec![vk::VideoFormatPropertiesKHR::default(); format_info_length as usize];

    unsafe {
        (video_queue_instance_ext
            .fp()
            .get_physical_device_video_format_properties_khr)(
            device,
            &format_info,
            &mut format_info_length,
            format_properties.as_mut_ptr(),
        )
        .result()?;
    }

    Ok(format_properties)
}

/// The device capabilities for encoding
#[derive(Debug, Clone, Copy)]
pub struct EncodeCapabilities {
    pub h264: Option<EncodeH264Capabilities>,
}

/// The device capabilities for H264 encoding.
///
/// See [`H264Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct EncodeH264Capabilities {
    pub baseline_profile: Option<EncodeH264ProfileCapabilities>,
    pub main_profile: Option<EncodeH264ProfileCapabilities>,
    pub high_profile: Option<EncodeH264ProfileCapabilities>,
}

/// The device capabilities for H264 encoding in a specific profile
#[derive(Debug, Clone, Copy)]
pub struct EncodeH264ProfileCapabilities {
    /// The minimum width of the coded image
    pub min_width: u32,
    /// The maximum width of the coded image
    pub max_width: u32,
    /// The minimum height of the coded image
    pub min_height: u32,
    /// The maximum height of the coded image
    pub max_height: u32,
    /// The supported rate control modes in bitflag form
    pub supported_rate_control: vk::VideoEncodeRateControlModeFlagsKHR,
    /// Maximum number of back references a P-frame can have
    pub max_references: u32,
    /// The count of [Vulkan Video encode quality levels](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#encode-quality-level)
    pub quality_levels: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeEncodeCapabilities {
    pub(crate) h264: Option<NativeEncodeH264Capabilities>,
}

impl NativeEncodeCapabilities {
    pub(crate) fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        supported_operations: vk::VideoCodecOperationFlagsKHR,
    ) -> Self {
        let h264 = match supported_operations.contains(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
        {
            true => Some(NativeEncodeH264Capabilities::query(instance, device)),
            false => None,
        };

        Self { h264 }
    }

    pub(crate) fn user_facing(&self) -> EncodeCapabilities {
        EncodeCapabilities {
            h264: self
                .h264
                .as_ref()
                .map(NativeEncodeH264Capabilities::user_facing),
        }
    }
}

// #[derive(Debug, Clone)]
// pub(crate) struct NativeEncodeH265Capabilities {
//     pub(crate) main: Option<NativeEncodeH264ProfileCapabilities>,
// }
//
// pub(crate) struct NativeEncodeH265ProfileCapabilities {}

#[derive(Debug, Clone)]
pub(crate) struct NativeEncodeH264Capabilities {
    pub(crate) baseline: Option<NativeEncodeProfileCapabilities<H264Codec>>,
    pub(crate) main: Option<NativeEncodeProfileCapabilities<H264Codec>>,
    pub(crate) high: Option<NativeEncodeProfileCapabilities<H264Codec>>,
}

impl NativeEncodeH264Capabilities {
    pub(crate) fn user_facing(&self) -> EncodeH264Capabilities {
        EncodeH264Capabilities {
            baseline_profile: self
                .baseline
                .as_ref()
                .map(NativeEncodeProfileCapabilities::user_facing),
            main_profile: self
                .main
                .as_ref()
                .map(NativeEncodeProfileCapabilities::user_facing),
            high_profile: self
                .high
                .as_ref()
                .map(NativeEncodeProfileCapabilities::user_facing),
        }
    }

    pub(crate) fn query(instance: &Instance, device: vk::PhysicalDevice) -> Self {
        let profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

        let mut baseline = vk::VideoEncodeH264ProfileInfoKHR::default().std_profile_idc(
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE,
        );
        let profile_baseline = profile.push_next(&mut baseline);
        let baseline =
            NativeEncodeProfileCapabilities::query(instance, device, &profile_baseline).ok();

        let mut main = vk::VideoEncodeH264ProfileInfoKHR::default()
            .std_profile_idc(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN);
        let profile_main = profile.push_next(&mut main);
        let main = NativeEncodeProfileCapabilities::query(instance, device, &profile_main).ok();

        let mut high = vk::VideoEncodeH264ProfileInfoKHR::default()
            .std_profile_idc(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH);
        let profile_high = profile.push_next(&mut high);
        let high = NativeEncodeProfileCapabilities::query(instance, device, &profile_high).ok();

        Self {
            baseline,
            main,
            high,
        }
    }

    pub(crate) fn profile(
        &self,
        profile: H264Profile,
    ) -> Option<&NativeEncodeProfileCapabilities<H264Codec>> {
        match profile {
            H264Profile::Baseline => self.baseline.as_ref(),
            H264Profile::Main => self.main.as_ref(),
            H264Profile::High => self.high.as_ref(),
        }
    }

    pub(crate) fn max_profile(&self) -> H264Profile {
        if self.high.is_some() {
            H264Profile::High
        } else if self.main.is_some() {
            H264Profile::Main
        } else {
            H264Profile::Baseline
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct NativeEncodeProfileCapabilities<C: Codec> {
    pub(crate) video_capabilities: vk::VideoCapabilitiesKHR<'static>,
    pub(crate) encode_capabilities: vk::VideoEncodeCapabilitiesKHR<'static>,
    pub(crate) encode_dpb_properties: Vec<vk::VideoFormatPropertiesKHR<'static>>,
    pub(crate) encode_src_properties: Vec<vk::VideoFormatPropertiesKHR<'static>>,
    pub(crate) quality_level_properties: Vec<NativeEncodeQualityLevelProperties<C>>,
    pub(crate) codec_encode_capabilities: C::CodecSpecificEncodeCapabilities<'static>,
}

impl<C: Codec> NativeEncodeProfileCapabilities<C> {
    fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        profile: &vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanInitError> {
        let encode_dpb_properties = query_video_format_properties(
            device,
            &instance.video_queue_instance_ext,
            profile,
            vk::ImageUsageFlags::VIDEO_ENCODE_DPB_KHR,
        )?;

        let encode_src_properties = query_video_format_properties(
            device,
            &instance.video_queue_instance_ext,
            profile,
            vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR,
        )?;

        let mut codec_encode_caps = C::CodecSpecificEncodeCapabilities::default();
        let mut encode_caps = vk::VideoEncodeCapabilitiesKHR::default();
        let mut caps = vk::VideoCapabilitiesKHR::default()
            .push_next(&mut encode_caps)
            .push_next(&mut codec_encode_caps);

        unsafe {
            (instance
                .video_queue_instance_ext
                .fp()
                .get_physical_device_video_capabilities_khr)(device, profile, &mut caps)
            .result()?;
        }

        let video_capabilities = vk::VideoCapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..caps
        };

        let encode_capabilities = vk::VideoEncodeCapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..encode_caps
        };

        let codec_encode_capabilities = C::static_codec_capabilities(&codec_encode_caps);

        let mut quality_level_properties =
            Vec::with_capacity(encode_capabilities.max_quality_levels as usize);

        for i in 0..encode_capabilities.max_quality_levels {
            if let Ok(qlp) = NativeEncodeQualityLevelProperties::query(instance, device, profile, i)
            {
                quality_level_properties.push(qlp);
            }
        }

        Ok(Self {
            video_capabilities,
            encode_capabilities,
            codec_encode_capabilities,
            encode_dpb_properties,
            encode_src_properties,
            quality_level_properties,
        })
    }
}

impl NativeEncodeProfileCapabilities<H264Codec> {
    fn user_facing(&self) -> EncodeH264ProfileCapabilities {
        EncodeH264ProfileCapabilities {
            min_width: self.video_capabilities.min_coded_extent.width,
            max_width: self.video_capabilities.max_coded_extent.width,
            min_height: self.video_capabilities.min_coded_extent.height,
            max_height: self.video_capabilities.max_coded_extent.height,
            supported_rate_control: self.encode_capabilities.rate_control_modes,
            max_references: self
                .codec_encode_capabilities
                .max_p_picture_l0_reference_count,
            quality_levels: self.encode_capabilities.max_quality_levels,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeEncodeQualityLevelProperties<C: Codec> {
    pub(crate) quality_level_properties: vk::VideoEncodeQualityLevelPropertiesKHR<'static>,
    pub(crate) codec_quality_level_properties:
        C::CodecSpecificEncodeQualityLevelProperties<'static>,
}

impl<C: Codec> NativeEncodeQualityLevelProperties<C> {
    fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        profile_info: &vk::VideoProfileInfoKHR<'_>,
        quality_level: u32,
    ) -> Result<Self, VulkanInitError> {
        let quality_level_info = vk::PhysicalDeviceVideoEncodeQualityLevelInfoKHR::default()
            .video_profile(profile_info)
            .quality_level(quality_level);

        let mut codec_qlp = C::CodecSpecificEncodeQualityLevelProperties::default();
        let mut qlp = vk::VideoEncodeQualityLevelPropertiesKHR::default().push_next(&mut codec_qlp);

        unsafe {
            (instance
                .video_encode_queue_instance_ext
                .fp()
                .get_physical_device_video_encode_quality_level_properties_khr)(
                device,
                &quality_level_info,
                &mut qlp,
            )
            .result()?;
        }

        let quality_level_properties = vk::VideoEncodeQualityLevelPropertiesKHR::default()
            .preferred_rate_control_mode(qlp.preferred_rate_control_mode)
            .preferred_rate_control_layer_count(qlp.preferred_rate_control_layer_count);

        let codec_specific_encode_quality_level_properties = C::static_codec_qlp(&codec_qlp);

        Ok(Self {
            quality_level_properties,
            codec_quality_level_properties: codec_specific_encode_quality_level_properties,
        })
    }

    pub(crate) fn zeroed(&self) -> bool {
        // this is hideous
        self.quality_level_properties
            .preferred_rate_control_mode
            .as_raw()
            == 0
            && self
                .quality_level_properties
                .preferred_rate_control_layer_count
                == 0
            && self.codec_quality_level_properties.zeroed()
    }
}

/// The device capabilities for decoding
#[derive(Debug, Clone, Copy)]
pub struct DecodeCapabilities {
    pub h264: Option<DecodeH264Capabilities>,
}

/// The device capabilities for H264 decoding.
///
/// See [`H264Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct DecodeH264Capabilities {
    pub baseline_profile: Option<DecodeH264ProfileCapabilities>,
    pub main_profile: Option<DecodeH264ProfileCapabilities>,
    pub high_profile: Option<DecodeH264ProfileCapabilities>,
}

/// The device capabilities for H264 decoding in a specific profile
#[derive(Debug, Clone, Copy)]
pub struct DecodeH264ProfileCapabilities {
    /// The minimum width of the coded image
    pub min_width: u32,
    /// The maximum width of the coded image
    pub max_width: u32,
    /// The minimum height of the coded image
    pub min_height: u32,
    /// The maximum height of the coded image
    pub max_height: u32,
    /// The maximum H264 level
    pub max_level_idc: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeDecodeCapabilities {
    pub(crate) baseline: Option<NativeDecodeProfileCapabilities>,
    pub(crate) main: Option<NativeDecodeProfileCapabilities>,
    pub(crate) high: Option<NativeDecodeProfileCapabilities>,
}

impl NativeDecodeCapabilities {
    pub(crate) fn user_facing(&self) -> DecodeH264Capabilities {
        DecodeH264Capabilities {
            baseline_profile: self
                .baseline
                .as_ref()
                .and_then(|profile| profile.user_facing().ok()),
            main_profile: self
                .main
                .as_ref()
                .and_then(|profile| profile.user_facing().ok()),
            high_profile: self
                .high
                .as_ref()
                .and_then(|profile| profile.user_facing().ok()),
        }
    }

    pub(crate) fn query(instance: &Instance, device: vk::PhysicalDevice) -> Self {
        let baseline = NativeDecodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE,
        )
        .ok();
        let main = NativeDecodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
        )
        .ok();
        let high = NativeDecodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        )
        .ok();

        Self {
            baseline,
            main,
            high,
        }
    }

    pub(crate) fn profile(&self, profile: H264Profile) -> Option<&NativeDecodeProfileCapabilities> {
        match profile {
            H264Profile::Baseline => self.baseline.as_ref(),
            H264Profile::Main => self.main.as_ref(),
            H264Profile::High => self.high.as_ref(),
        }
    }

    pub(crate) fn max_profile(&self) -> H264Profile {
        if self.high.is_some() {
            H264Profile::High
        } else if self.main.is_some() {
            H264Profile::Main
        } else {
            H264Profile::Baseline
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeDecodeProfileCapabilities {
    pub(crate) video_capabilities: vk::VideoCapabilitiesKHR<'static>,
    #[allow(dead_code)]
    pub(crate) decode_capabilities: vk::VideoDecodeCapabilitiesKHR<'static>,
    pub(crate) h264_decode_capabilities: vk::VideoDecodeH264CapabilitiesKHR<'static>,
    pub(crate) h264_dpb_format_properties: vk::VideoFormatPropertiesKHR<'static>,
    pub(crate) h264_dst_format_properties: Option<vk::VideoFormatPropertiesKHR<'static>>,
}

impl NativeDecodeProfileCapabilities {
    pub(crate) fn user_facing(&self) -> Result<DecodeH264ProfileCapabilities, VulkanDecoderError> {
        Ok(DecodeH264ProfileCapabilities {
            min_width: self.video_capabilities.min_coded_extent.width,
            max_width: self.video_capabilities.max_coded_extent.width,
            min_height: self.video_capabilities.min_coded_extent.height,
            max_height: self.video_capabilities.max_coded_extent.height,
            max_level_idc: vk_to_h264_level_idc(self.h264_decode_capabilities.max_level_idc)?,
        })
    }

    pub(crate) fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        profile: vk::native::StdVideoH264ProfileIdc,
    ) -> Result<Self, VulkanInitError> {
        let mut h264_decode_profile_info = vk::VideoDecodeH264ProfileInfoKHR::default()
            .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE)
            .std_profile_idc(profile);

        let decode_profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .push_next(&mut h264_decode_profile_info);

        let mut h264_decode_caps = vk::VideoDecodeH264CapabilitiesKHR::default();
        let mut decode_caps = vk::VideoDecodeCapabilitiesKHR {
            p_next: (&mut h264_decode_caps as *mut _) as *mut c_void, // why does this not have `.push_next()`? wtf
            ..Default::default()
        };

        let mut caps = vk::VideoCapabilitiesKHR::default().push_next(&mut decode_caps);

        unsafe {
            (instance
                .video_queue_instance_ext
                .fp()
                .get_physical_device_video_capabilities_khr)(
                device,
                &decode_profile_info,
                &mut caps,
            )
            .result()?
        };

        let video_capabilities = vk::VideoCapabilitiesKHR::default()
            .flags(caps.flags)
            .min_bitstream_buffer_size_alignment(caps.min_bitstream_buffer_size_alignment)
            .min_bitstream_buffer_offset_alignment(caps.min_bitstream_buffer_offset_alignment)
            .picture_access_granularity(caps.picture_access_granularity)
            .min_coded_extent(caps.min_coded_extent)
            .max_coded_extent(caps.max_coded_extent)
            .max_dpb_slots(caps.max_dpb_slots)
            .max_active_reference_pictures(caps.max_active_reference_pictures)
            .std_header_version(caps.std_header_version);

        let decode_capabilities =
            vk::VideoDecodeCapabilitiesKHR::default().flags(decode_caps.flags);

        let h264_decode_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default()
            .max_level_idc(h264_decode_caps.max_level_idc)
            .field_offset_granularity(h264_decode_caps.field_offset_granularity);

        let flags = decode_caps.flags;

        let h264_dpb_format_properties =
            if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
                query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                        | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )?
            } else {
                query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
                )?
            };

        let h264_dst_format_properties =
            if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
                None
            } else {
                Some(query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::TRANSFER_SRC,
                )?)
            };

        let h264_dpb_format_properties = match h264_dpb_format_properties
            .into_iter()
            .find(|f| f.format == vk::Format::G8_B8R8_2PLANE_420_UNORM)
        {
            Some(f) => f,
            None => return Err(VulkanInitError::NoNV12ProfileSupport),
        };

        let h264_dst_format_properties = match h264_dst_format_properties {
            Some(format_properties) => match format_properties
                .into_iter()
                .find(|f| f.format == vk::Format::G8_B8R8_2PLANE_420_UNORM)
            {
                Some(f) => Some(f),
                None => return Err(VulkanInitError::NoNV12ProfileSupport),
            },
            None => None,
        };

        Ok(Self {
            video_capabilities,
            decode_capabilities,
            h264_decode_capabilities,
            h264_dpb_format_properties,
            h264_dst_format_properties,
        })
    }
}
