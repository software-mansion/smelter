use std::ptr::null_mut;

use crate::DeckLinkError;

use super::ffi::{self, HResult};

pub struct ProfileAttributes(pub(super) *mut ffi::IDeckLinkProfileAttributes);

impl ProfileAttributes {
    pub fn get_flag(
        &self,
        id: ffi::FlagAttributeId,
    ) -> Result<Option<bool>, DeckLinkError> {
        let mut value = false;
        match unsafe { ffi::profile_attributes_flag(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkProfileAttributes::GetFlag")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_integer(
        &self,
        id: ffi::IntegerAttributeId,
    ) -> Result<Option<i64>, DeckLinkError> {
        let mut value: i64 = 0;
        match unsafe { ffi::profile_attributes_integer(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkProfileAttributes::GetInt")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_float(
        &self,
        id: ffi::FloatAttributeId,
    ) -> Result<Option<f64>, DeckLinkError> {
        let mut value: f64 = 0.0;
        match unsafe { ffi::profile_attributes_float(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkProfileAttributes::GetFloat")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_string(
        &self,
        id: ffi::StringAttributeId,
    ) -> Result<Option<String>, DeckLinkError> {
        // List of attributes that should not be freed
        const STATIC_STRING_ATTRIBUTES: [ffi::StringAttributeId; 1] =
            [ffi::StringAttributeId::VendorName];

        let is_static = STATIC_STRING_ATTRIBUTES.iter().any(|static_id| static_id == &id);
        let mut value = String::new();
        match unsafe {
            ffi::profile_attributes_string(self.0, id, &mut value, is_static)?
        } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkProfileAttributes::GetString")?;
                Ok(Some(value))
            }
        }
    }
}

impl Drop for ProfileAttributes {
    fn drop(&mut self) {
        unsafe { ffi::profile_attributes_release(self.0) };
    }
}

pub struct ProfileManager(pub(super) *mut ffi::IDeckLinkProfileManager);

impl ProfileManager {
    pub fn profiles(&self) -> Result<Vec<Profile>, DeckLinkError> {
        let mut profiles = vec![];
        unsafe { ffi::profile_manager_profiles(self.0, &mut profiles) }
            .into_result("IDeckLinkProfileManager::GetProfiles")?;
        Ok(profiles.into_iter().map(|wrapper| Profile(wrapper.ptr)).collect())
    }
}

impl Drop for ProfileManager {
    fn drop(&mut self) {
        unsafe { ffi::profile_manager_release(self.0) };
    }
}

pub struct Profile(*mut ffi::IDeckLinkProfile);

impl Profile {
    pub fn attributes(&self) -> Result<ProfileAttributes, DeckLinkError> {
        let mut attributes = null_mut();
        unsafe { ffi::profile_profile_attributes(self.0, &mut attributes) }.into_result(
            "IDeckLinkProfile::QueryInterface(IID_IDeckLinkProfileAttributes, _)",
        )?;
        Ok(ProfileAttributes(attributes))
    }

    pub fn is_active(&self) -> Result<bool, DeckLinkError> {
        unsafe {
            let mut is_active = false;
            ffi::profile_is_active(self.0, &mut is_active)
                .into_result("IDeckLinkProfile::IsActive")?;
            Ok(is_active)
        }
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        unsafe { ffi::profile_release(self.0) };
    }
}
