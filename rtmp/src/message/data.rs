use std::collections::HashMap;

use crate::amf0::AmfValue;

#[derive(Debug, Clone)]
pub(crate) enum DataMessage {
    OnMetaData(HashMap<String, AmfValue>),
    Unknown(Vec<AmfValue>),
}

impl DataMessage {
    pub fn from_amf_values(mut values: Vec<AmfValue>) -> Self {
        // onMetaData can appear as:
        //   ["@setDataFrame", "onMetaData", {properties}]
        //   ["onMetaData", {properties}]
        let start = match values.first() {
            Some(AmfValue::String(s)) if s == "@setDataFrame" => 1,
            _ => 0,
        };

        let is_on_metadata =
            matches!(values.get(start), Some(AmfValue::String(s)) if s == "onMetaData");
        if !is_on_metadata {
            return DataMessage::Unknown(values);
        }

        let metadata_idx = start + 1;
        let metadata = match values.get(metadata_idx) {
            Some(AmfValue::Object(_) | AmfValue::EcmaArray(_)) => {
                match values.swap_remove(metadata_idx) {
                    AmfValue::Object(map) | AmfValue::EcmaArray(map) => map,
                    _ => unreachable!(),
                }
            }
            _ => HashMap::new(),
        };

        DataMessage::OnMetaData(metadata)
    }

    pub fn into_amf_values(self) -> Vec<AmfValue> {
        match self {
            DataMessage::OnMetaData(metadata) => {
                vec![
                    AmfValue::String("@setDataFrame".to_string()),
                    AmfValue::String("onMetaData".to_string()),
                    AmfValue::EcmaArray(metadata),
                ]
            }
            DataMessage::Unknown(values) => values,
        }
    }
}
