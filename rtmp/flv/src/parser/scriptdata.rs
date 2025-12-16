use bytes::Bytes;

use crate::ScriptDataTag;

impl ScriptDataTag {
    pub(super) fn parse(payload: &[u8]) -> Self {
        Self {
            payload: Bytes::copy_from_slice(payload),
        }
    }
}
