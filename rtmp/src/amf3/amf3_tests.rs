use std::collections::HashMap;

use bytes::BytesMut;

use super::Amf3Value;
use crate::amf3::{Amf3DecoderState, Amf3EncoderState};

#[test]
fn test_string() {
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let sample_string = Amf3Value::String("kremówki".to_string());

    encoder.put_value(&sample_string).unwrap();
    let encoded_string = encoder.buf.freeze();

    let mut decoder = Amf3DecoderState::new(encoded_string);
    let decoded_string = decoder.decode_value().unwrap();

    assert_eq!(decoded_string, sample_string);
}

#[test]
fn test_integer() {
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let sample_pos = Amf3Value::Integer(2137);
    let sample_neg = Amf3Value::Integer(-2137);

    encoder.put_value(&sample_pos).unwrap();
    encoder.put_value(&sample_neg).unwrap();
    let amf3_bytes = encoder.buf.freeze();

    let mut decoder = Amf3DecoderState::new(amf3_bytes);
    let decoded_pos = decoder.decode_value().unwrap();
    let decoded_neg = decoder.decode_value().unwrap();

    assert_eq!(decoded_pos, sample_pos);
    assert_eq!(decoded_neg, sample_neg);
}

#[test]
fn test_array() {
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let associative = HashMap::from([
        ("Integer".to_string(), Amf3Value::Integer(2137)),
        (
            "String".to_string(),
            Amf3Value::String("kremówki".to_string()),
        ),
    ]);
    let dense = vec![Amf3Value::Xml("Sample XML".to_string())];
    let amf_array = Amf3Value::Array { associative, dense };
    encoder.put_value(&amf_array).unwrap();
    let amf3_values = encoder.buf.freeze();

    let mut decoder = Amf3DecoderState::new(amf3_values);
    let decoded_array = decoder.decode_value().unwrap();

    assert_eq!(decoded_array, amf_array);
}

#[test]
fn test_xml_and_xml_doc() {
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let xml = Amf3Value::Xml("Sample XML".to_string());
    let xml_doc = Amf3Value::XmlDoc("Sample XML doc".to_string());

    encoder.put_value(&xml).unwrap();
    encoder.put_value(&xml_doc).unwrap();
    let amf3_values = encoder.buf.freeze();

    let mut decoder = Amf3DecoderState::new(amf3_values);
    let decoded_xml = decoder.decode_value().unwrap();
    let decoded_xml_doc = decoder.decode_value().unwrap();

    assert_eq!(decoded_xml, xml);
    assert_eq!(decoded_xml_doc, xml_doc);
}

#[test]
fn test_object() {
    // Case with non-empty class name
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let values = vec![
        ("Val1".to_string(), Amf3Value::Null),
        ("Val2".to_string(), Amf3Value::Undefined),
        (
            "Val3".to_string(),
            Amf3Value::String("kremówki".to_string()),
        ),
        ("Val4".to_string(), Amf3Value::Integer(2137)),
    ];
    let amf_object = Amf3Value::Object {
        class_name: Some("Test name".to_string()),
        sealed_count: 2,
        values,
    };
    encoder.put_value(&amf_object).unwrap();
    let amf3_values = encoder.buf.freeze();
    let mut decoder = Amf3DecoderState::new(amf3_values);
    let decoded_object = decoder.decode_value().unwrap();
    assert_eq!(decoded_object, amf_object);

    // Case with empty class name
    let mut encoder = Amf3EncoderState::new(BytesMut::new());
    let values = vec![
        (
            "Val1".to_string(),
            Amf3Value::String("kremówki".to_string()),
        ),
        ("Val2".to_string(), Amf3Value::Integer(2137)),
    ];
    let amf_object = Amf3Value::Object {
        class_name: None,
        sealed_count: 2,
        values,
    };
    encoder.put_value(&amf_object).unwrap();
    let amf3_values = encoder.buf.freeze();
    let mut decoder = Amf3DecoderState::new(amf3_values);
    let decoded_object = decoder.decode_value().unwrap();
    assert_eq!(decoded_object, amf_object);
}
