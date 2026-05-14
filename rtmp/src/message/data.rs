use std::collections::HashMap;

use crate::amf0::AmfValue;

#[derive(Debug, Clone)]
pub(crate) enum DataMessage {
    OnMetaData(HashMap<String, AmfValue>),
    Unknown(Vec<AmfValue>),
}

impl DataMessage {
    pub fn from_amf_values(values: Vec<AmfValue>) -> Self {
        // onMetaData can appear as:
        //   ["@setDataFrame", "onMetaData", {properties}]
        //   ["onMetaData", {properties}]
        // Some publishers additionally prefix the data message names with "@".
        // Accept both forms.
        let mut iter = values.into_iter();
        match (iter.next(), iter.next(), iter.next()) {
            (
                Some(AmfValue::String(s1)),
                Some(AmfValue::String(s2)),
                Some(AmfValue::Object(map) | AmfValue::EcmaArray(map)),
            ) if matches!(s1.as_str(), "setDataFrame" | "@setDataFrame")
                && matches!(s2.as_str(), "onMetaData" | "@onMetaData") =>
            {
                Self::OnMetaData(map)
            }
            (
                Some(AmfValue::String(s1)),
                Some(AmfValue::Object(map) | AmfValue::EcmaArray(map)),
                _,
            ) if matches!(s1.as_str(), "onMetaData" | "@onMetaData") => Self::OnMetaData(map),
            (v1, v2, v3) => {
                let first_3_values = [v1, v2, v3].into_iter().flatten();
                Self::Unknown(first_3_values.chain(iter).collect())
            }
        }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::DataMessage;
    use crate::AmfValue;

    #[test]
    fn serializes_on_metadata_as_ecma_array() {
        let mut metadata = HashMap::new();
        metadata.insert("width".to_string(), AmfValue::Number(1920.0));

        let values = DataMessage::OnMetaData(metadata.clone()).into_amf_values();

        assert_eq!(
            values,
            vec![
                AmfValue::String("@setDataFrame".to_string()),
                AmfValue::String("onMetaData".to_string()),
                AmfValue::EcmaArray(metadata),
            ]
        );
    }
}
