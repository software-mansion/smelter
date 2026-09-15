use std::{ffi::c_void, sync::Arc};

use ash::vk;
use tracing::{error, info, trace, warn};

use crate::backends::vulkan::VulkanInstanceInitError;

use super::Instance;

pub(crate) struct DebugMessenger {
    messenger: vk::DebugUtilsMessengerEXT,
    instance: Arc<Instance>,
}

impl DebugMessenger {
    pub(crate) fn new(instance: Arc<Instance>) -> Result<Self, VulkanInstanceInitError> {
        let Some(debug_utils) = &instance.debug_utils_instance_ext else {
            return Err(VulkanInstanceInitError::MissingExtension(
                vk::EXT_DEBUG_UTILS_NAME.to_string_lossy().to_string(),
            ));
        };

        let debug_messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                    | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_messenger_callback));

        let messenger = unsafe {
            debug_utils.create_debug_utils_messenger(&debug_messenger_create_info, None)?
        };

        Ok(Self {
            instance,
            messenger,
        })
    }
}

impl Drop for DebugMessenger {
    fn drop(&mut self) {
        let Some(debug_utils) = &self.instance.debug_utils_instance_ext else {
            return;
        };

        unsafe { debug_utils.destroy_debug_utils_messenger(self.messenger, None) };
    }
}

unsafe extern "system" fn debug_messenger_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let callback_data = unsafe { *p_callback_data };

    // FIXME: This is a bug in wgpu:
    // https://github.com/gfx-rs/wgpu/issues/7696
    // Until it's fixed upstream, let's silence `VUID-StandaloneSpirv-None-10684`.
    if callback_data.message_id_number == 0xb210f7c2u32 as i32 {
        return vk::FALSE;
    }

    // This is an error about creating an image for video coding that has usage flags not
    // advertised as supported by the GPU. We use this extensively on Nvidia and it works fine.
    // Thread on Nvidia developer forum: https://forums.developer.nvidia.com/t/vkimagecreateflags-and-vulkan-encode/284369
    // The VUID for this message is `VUID-VkImageCreateInfo-pNext-06811`.
    if callback_data.message_id_number == 0x30f4ac70u32 as i32 {
        return vk::FALSE;
    }

    let message_id = unsafe {
        callback_data
            .message_id_name_as_c_str()
            .unwrap_or(c"")
            .to_string_lossy()
    };

    let message = unsafe {
        callback_data
            .message_as_c_str()
            .unwrap_or(c"")
            .to_string_lossy()
    };

    let t = format!("{message_types:?}");
    match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
            trace!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
            info!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
            warn!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
            error!("[{t}][{message_id}] {message}");
        }
        _ => {}
    }

    vk::FALSE
}
