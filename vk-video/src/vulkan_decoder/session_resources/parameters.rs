use std::{collections::HashMap, sync::Arc};

use ash::vk;
use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};

use crate::{
    vulkan_decoder::{
        Device, VideoSessionParameters, VkPictureParameterSet, VkSequenceParameterSet,
    },
    VulkanDecoderError, VulkanDevice,
};

/// Since `VideoSessionParameters` can only add sps and pps values (inserting sps or pps with an
/// existing id is prohibited), this is an abstraction which provides the capability to replace an
/// existing sps or pps.
pub(crate) struct VideoSessionParametersManager {
    pub(crate) parameters: VideoSessionParameters,
    sps: HashMap<u8, VkSequenceParameterSet>,
    pps: HashMap<(u8, u8), VkPictureParameterSet>,
    device: Arc<Device>,
    session: vk::VideoSessionKHR,
}

impl VideoSessionParametersManager {
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        session: vk::VideoSessionKHR,
    ) -> Result<Self, VulkanDecoderError> {
        Ok(Self {
            parameters: VideoSessionParameters::new(
                vulkan_ctx.device.clone(),
                session,
                &[],
                &[],
                None,
                None,
            )?,
            sps: HashMap::new(),
            pps: HashMap::new(),
            device: vulkan_ctx.device.clone(),
            session,
        })
    }

    pub(crate) fn parameters(&self) -> vk::VideoSessionParametersKHR {
        self.parameters.parameters
    }

    pub(crate) fn change_session(
        &mut self,
        session: vk::VideoSessionKHR,
    ) -> Result<(), VulkanDecoderError> {
        if self.session == session {
            return Ok(());
        }
        self.session = session;

        let sps = self.sps.values().map(|sps| sps.sps).collect::<Vec<_>>();
        let pps = self.pps.values().map(|pps| pps.pps).collect::<Vec<_>>();

        self.parameters =
            VideoSessionParameters::new(self.device.clone(), session, &sps, &pps, None, None)?;

        Ok(())
    }

    // it is probably not optimal to insert sps and pps searately. this could be optimized, so that
    // the insertion happens lazily when the parameters are bound to a session.
    pub(crate) fn put_sps(&mut self, sps: &SeqParameterSet) -> Result<(), VulkanDecoderError> {
        let key = sps.seq_parameter_set_id.id();
        match self.sps.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(sps.into());

                self.parameters = VideoSessionParameters::new(
                    self.device.clone(),
                    self.session,
                    &[self.sps[&key].sps],
                    &[],
                    Some(&self.parameters),
                    None,
                )?
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(sps.into());

                self.parameters.add(&[self.sps[&key].sps], &[])?;
            }
        }

        Ok(())
    }

    pub(crate) fn put_pps(&mut self, pps: &PicParameterSet) -> Result<(), VulkanDecoderError> {
        let key = (pps.seq_parameter_set_id.id(), pps.pic_parameter_set_id.id());
        match self.pps.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(pps.into());

                self.parameters = VideoSessionParameters::new(
                    self.device.clone(),
                    self.session,
                    &[],
                    &[self.pps[&key].pps],
                    Some(&self.parameters),
                    None,
                )?;
            }

            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(pps.into());

                self.parameters.add(&[], &[self.pps[&key].pps])?;
            }
        }

        Ok(())
    }
}
