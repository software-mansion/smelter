use crate::DeckLinkError;

use super::ffi::{self, HResult};

pub struct DeckLinkConfiguration(pub(super) *mut ffi::IDeckLinkConfiguration);

impl DeckLinkConfiguration {
    pub fn get_flag(
        &self,
        id: ffi::FlagConfigurationId,
    ) -> Result<Option<bool>, DeckLinkError> {
        let mut value = false;
        match unsafe { ffi::configuration_flag(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkConfiguration::GetFlag")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_integer(
        &self,
        id: ffi::IntegerConfigurationId,
    ) -> Result<Option<i64>, DeckLinkError> {
        let mut value: i64 = 0;
        match unsafe { ffi::configuration_integer(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkConfiguration::GetInt")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_float(
        &self,
        id: ffi::FloatConfigurationId,
    ) -> Result<Option<f64>, DeckLinkError> {
        let mut value: f64 = 0.0;
        match unsafe { ffi::configuration_float(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkConfiguration::GetFloat")?;
                Ok(Some(value))
            }
        }
    }

    pub fn get_string(
        &self,
        id: ffi::StringConfigurationId,
    ) -> Result<Option<String>, DeckLinkError> {
        let mut value = String::new();
        match unsafe { ffi::configuration_string(self.0, id, &mut value)? } {
            HResult::NotImplementedError => Ok(None),
            hresult => {
                hresult.into_result("IDeckLinkConfiguration::GetString")?;
                Ok(Some(value))
            }
        }
    }

    pub fn set_flag(
        &mut self,
        id: ffi::FlagConfigurationId,
        value: bool,
    ) -> Result<(), DeckLinkError> {
        let hresult = unsafe { ffi::configuration_set_flag(self.0, id, value)? };
        hresult.into_result("IDeckLinkConfiguration::SetFlag")
    }

    pub fn set_integer(
        &mut self,
        id: ffi::IntegerConfigurationId,
        value: i64,
    ) -> Result<(), DeckLinkError> {
        let hresult = unsafe { ffi::configuration_set_integer(self.0, id, value)? };
        hresult.into_result("IDeckLinkConfiguration::SetInt")
    }

    pub fn set_float(
        &mut self,
        id: ffi::FloatConfigurationId,
        value: f64,
    ) -> Result<(), DeckLinkError> {
        let hresult = unsafe { ffi::configuration_set_float(self.0, id, value)? };
        hresult.into_result("IDeckLinkConfiguration::SetFloat")
    }

    pub fn set_string(
        &mut self,
        id: ffi::StringConfigurationId,
        value: String,
    ) -> Result<(), DeckLinkError> {
        let hresult = unsafe { ffi::configuration_set_string(self.0, id, value)? };
        hresult.into_result("IDeckLinkConfiguration::SetString")
    }
}

impl Drop for DeckLinkConfiguration {
    fn drop(&mut self) {
        unsafe { ffi::configuration_release(self.0) };
    }
}
