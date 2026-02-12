use bytes::{BufMut, Bytes};

use crate::{Amf3EncodingError, AmfEncodingError, amf3::*};

const U28_MAX: u32 = (1 << 28) - 1;

const I29_MAX: i32 = (1 << 28) - 1;
const I29_MIN: i32 = -(1 << 28);

const MAX_SEALED_COUNT: u32 = (1 << 25) - 1;

pub(crate) struct Amf3EncoderState<T> {
    pub(super) buf: T,
}

impl<T> Amf3EncoderState<T>
where
    T: BufMut,
{
    pub(crate) fn new(buf: T) -> Self {
        Self { buf }
    }

    pub(crate) fn put_value(&mut self, amf3_value: &Amf3Value) -> Result<(), AmfEncodingError> {
        match amf3_value {
            Amf3Value::Undefined => self.put_undefined(),
            Amf3Value::Null => self.put_null(),
            Amf3Value::Boolean(b) => self.put_boolean(*b),
            Amf3Value::Integer(i) => self.put_integer(*i)?,
            Amf3Value::Double(d) => self.put_double(*d),
            Amf3Value::String(s) => self.put_string(s)?,
            Amf3Value::XmlDoc(xd) => self.put_xml_doc(xd)?,
            Amf3Value::Date(d) => self.put_date(*d)?,
            Amf3Value::Array { associative, dense } => self.put_array(associative, dense)?,
            Amf3Value::Object {
                class_name,
                sealed_count,
                values,
            } => self.put_object(class_name.as_ref(), *sealed_count, values)?,
            Amf3Value::Xml(x) => self.put_xml(x)?,
            Amf3Value::ByteArray(ba) => self.put_byte_array(ba)?,
            Amf3Value::VectorInt {
                fixed_length,
                values,
            } => self.put_vector_int(*fixed_length, values)?,
            Amf3Value::VectorUInt {
                fixed_length,
                values,
            } => self.put_vector_uint(*fixed_length, values)?,
            Amf3Value::VectorDouble {
                fixed_length,
                values,
            } => self.put_vector_double(*fixed_length, values)?,
            Amf3Value::VectorObject {
                fixed_length,
                class_name,
                values,
            } => self.put_vector_object(*fixed_length, class_name.as_ref(), values)?,
            Amf3Value::Dictionary {
                weak_references,
                entries,
            } => self.put_dictionary(*weak_references, entries)?,
        }
        Ok(())
    }

    fn put_marker(&mut self, marker: u8) {
        self.buf.put_u8(marker);
    }

    fn put_undefined(&mut self) {
        self.put_marker(UNDEFINED);
    }

    fn put_null(&mut self) {
        self.put_marker(NULL);
    }

    fn put_boolean(&mut self, b: bool) {
        match b {
            false => self.put_marker(FALSE),
            true => self.put_marker(TRUE),
        }
    }

    fn put_integer(&mut self, i29: i32) -> Result<(), AmfEncodingError> {
        if !(I29_MIN..=I29_MAX).contains(&i29) {
            return Err(Amf3EncodingError::OutOfRangeInteger.into());
        }

        self.put_marker(INTEGER);
        if i29 >= 0 {
            self.buf.put_slice(&self.encode_u29(i29 as u32)?);
        } else {
            let u29 = ((i29 as u32) & 0x0F_FF_FF_FF) | 0x10_00_00_00;
            self.buf.put_slice(&self.encode_u29(u29)?);
        }
        Ok(())
    }

    fn put_double(&mut self, d: f64) {
        self.put_marker(DOUBLE);
        self.buf.put_f64(d);
    }

    fn put_string(&mut self, s: &str) -> Result<(), AmfEncodingError> {
        self.put_marker(STRING);
        self.put_string_raw(s)
    }

    fn put_string_raw(&mut self, s: &str) -> Result<(), AmfEncodingError> {
        if s.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::StringTooLong(s.len()).into());
        }
        let u29s = self.encode_u29(((s.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29s);
        self.buf.put_slice(s.as_bytes());
        Ok(())
    }

    fn put_xml_doc(&mut self, xd: &str) -> Result<(), AmfEncodingError> {
        if xd.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::StringTooLong(xd.len()).into());
        }
        self.put_marker(XML_DOC);
        let u29x = self.encode_u29(((xd.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29x);
        self.buf.put_slice(xd.as_bytes());
        Ok(())
    }

    fn put_date(&mut self, d: f64) -> Result<(), AmfEncodingError> {
        self.put_marker(DATE);

        // For date the only necessary information is if it is a value (`U29D` set to 1). Remaining
        // bits are insignificant, they are set to 0 so the whole value is encoded in 1 byte
        // only.
        self.buf.put_slice(&self.encode_u29(1)?);
        self.buf.put_f64(d);
        Ok(())
    }

    fn put_array(
        &mut self,
        associative: &HashMap<String, Amf3Value>,
        dense: &Vec<Amf3Value>,
    ) -> Result<(), AmfEncodingError> {
        if dense.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::ArrayTooLong(dense.len()).into());
        }

        self.put_marker(ARRAY);
        let u29a = self.encode_u29(((dense.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29a);
        for (k, v) in associative {
            self.put_string_raw(k)?;
            self.put_value(v)?;
        }
        self.buf.put_u8(0x01);
        for val in dense {
            self.put_value(val)?;
        }
        Ok(())
    }

    fn put_object(
        &mut self,
        class_name: Option<&String>,
        sealed_count: usize,
        values: &[(String, Amf3Value)],
    ) -> Result<(), AmfEncodingError> {
        if sealed_count > MAX_SEALED_COUNT as usize {
            return Err(Amf3EncodingError::SealedMembersCountTooLarge(sealed_count).into());
        }
        if sealed_count > values.len() {
            return Err(Amf3EncodingError::SealedCountTooLarge {
                sealed_count,
                actual_members: values.len(),
            }
            .into());
        }

        let mut u29o = ((sealed_count as u32) << 4) | 0b0011;
        if sealed_count < values.len() {
            u29o |= 0b1000;
        }

        self.put_marker(OBJECT);
        self.buf.put_slice(&self.encode_u29(u29o)?);
        match class_name {
            Some(s) => self.put_string_raw(s)?,
            None => self.put_string_raw("")?,
        }

        let (sealed, dynamic) = if sealed_count < values.len() {
            let (s, d) = values.split_at(sealed_count);
            (s, Some(d))
        } else {
            (values, None)
        };

        let (sealed_keys, sealed_values): (Vec<&str>, Vec<&Amf3Value>) =
            sealed.iter().map(|(k, v)| (k.as_str(), v)).unzip();
        for k in sealed_keys {
            self.put_string_raw(k)?;
        }
        for v in sealed_values {
            self.put_value(v)?;
        }

        if let Some(d) = dynamic {
            for (k, v) in d {
                self.put_string_raw(k)?;
                self.put_value(v)?;
            }
            self.buf.put_u8(0x01);
        }

        Ok(())
    }

    fn put_xml(&mut self, x: &str) -> Result<(), AmfEncodingError> {
        if x.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::StringTooLong(x.len()).into());
        }
        self.put_marker(XML);
        let u29x = self.encode_u29(((x.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29x);
        self.buf.put_slice(x.as_bytes());
        Ok(())
    }

    fn put_byte_array(&mut self, ba: &Bytes) -> Result<(), AmfEncodingError> {
        if ba.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::ArrayTooLong(ba.len()).into());
        }

        self.put_marker(BYTE_ARRAY);
        let u29b = self.encode_u29(((ba.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29b);
        self.buf.put_slice(ba);
        Ok(())
    }

    fn put_vector_int(
        &mut self,
        fixed_length: bool,
        values: &[i32],
    ) -> Result<(), AmfEncodingError> {
        if values.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::VectorTooLong(values.len()).into());
        }

        self.put_marker(VECTOR_INT);
        let u29v = self.encode_u29(((values.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29v);
        self.buf.put_u8(fixed_length.into());
        for int in values {
            self.buf.put_i32(*int);
        }
        Ok(())
    }

    fn put_vector_uint(
        &mut self,
        fixed_length: bool,
        values: &[u32],
    ) -> Result<(), AmfEncodingError> {
        if values.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::VectorTooLong(values.len()).into());
        }

        self.put_marker(VECTOR_UINT);
        let u29v = self.encode_u29(((values.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29v);
        self.buf.put_u8(fixed_length.into());
        for uint in values {
            self.buf.put_u32(*uint);
        }
        Ok(())
    }

    fn put_vector_double(
        &mut self,
        fixed_length: bool,
        values: &[f64],
    ) -> Result<(), AmfEncodingError> {
        if values.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::VectorTooLong(values.len()).into());
        }

        self.put_marker(VECTOR_DOUBLE);
        let u29v = self.encode_u29(((values.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29v);
        self.buf.put_u8(fixed_length.into());
        for double in values {
            self.buf.put_f64(*double);
        }
        Ok(())
    }

    fn put_vector_object(
        &mut self,
        fixed_length: bool,
        class_name: Option<&String>,
        values: &[Amf3Value],
    ) -> Result<(), AmfEncodingError> {
        if values.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::VectorTooLong(values.len()).into());
        }

        self.put_marker(VECTOR_OBJECT);
        let u29v = self.encode_u29(((values.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29v);
        self.buf.put_u8(fixed_length.into());
        match class_name {
            Some(name) => self.put_string_raw(name)?,
            None => self.put_string_raw("*")?,
        }
        for obj in values {
            self.put_value(obj)?;
        }
        Ok(())
    }

    fn put_dictionary(
        &mut self,
        weak_references: bool,
        entries: &[(Amf3Value, Amf3Value)],
    ) -> Result<(), AmfEncodingError> {
        if entries.len() > U28_MAX as usize {
            return Err(Amf3EncodingError::DictionaryTooLong(entries.len()).into());
        }

        self.put_marker(DICTIONARY);
        let u29dict = self.encode_u29(((entries.len() as u32) << 1) | 0b1)?;
        self.buf.put_slice(&u29dict);
        self.buf.put_u8(weak_references.into());
        for (key, value) in entries {
            self.put_value(key)?;
            self.put_value(value)?;
        }
        Ok(())
    }

    fn encode_u29(&self, mut u29: u32) -> Result<Bytes, AmfEncodingError> {
        const ONE_BYTE_MAX: u32 = 2u32.pow(7) - 1;
        const TWO_BYTE_MAX: u32 = 2u32.pow(14) - 1;
        const THREE_BYTE_MAX: u32 = 2u32.pow(21) - 1;
        const FOUR_BYTE_MAX: u32 = 2u32.pow(29) - 1;

        let n_bytes: usize = match u29 {
            n if n <= ONE_BYTE_MAX => 1,
            n if n <= TWO_BYTE_MAX => 2,
            n if n <= THREE_BYTE_MAX => 3,
            n if n <= FOUR_BYTE_MAX => 4,
            _ => {
                return Err(Amf3EncodingError::OutOfRangeU29.into());
            }
        };

        match n_bytes {
            1 => {
                let first = (u29 & 0x7F) as u8;
                Ok(Bytes::from_iter([first]))
            }
            2 => {
                let second = (u29 & 0x7F) as u8;
                u29 >>= 7;
                let first = 0x80 | (u29 & 0x7F) as u8;
                Ok(Bytes::from_iter([first, second]))
            }
            3 => {
                let third = (u29 & 0x7F) as u8;
                u29 >>= 7;
                let second = 0x80 | (u29 & 0x7F) as u8;
                u29 >>= 7;
                let first = 0x80 | (u29 & 0x7F) as u8;

                Ok(Bytes::from_iter([first, second, third]))
            }
            4 => {
                let fourth = (u29 & 0xFF) as u8;
                u29 >>= 8;
                let third = 0x80 | (u29 & 0x7F) as u8;
                u29 >>= 7;
                let second = 0x80 | (u29 & 0x7F) as u8;
                u29 >>= 7;
                let first = 0x80 | (u29 & 0x7F) as u8;
                Ok(Bytes::from_iter([first, second, third, fourth]))
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod encode_test {
    use bytes::{Bytes, BytesMut};

    use crate::amf3::INTEGER;
    use crate::amf3::encoding::Amf3EncoderState;

    #[test]
    fn encode_u29_test() {
        let encoder = Amf3EncoderState::new(BytesMut::new());

        let one_byte = 105;
        let expected = Bytes::from_iter([0b01101001]);
        let actual = encoder.encode_u29(one_byte).unwrap();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual, expected);

        let two_byte = 2137;
        let expected = Bytes::from_iter([0b10010000, 0b01011001]);
        let actual = encoder.encode_u29(two_byte).unwrap();
        assert_eq!(actual.len(), 2);
        assert_eq!(actual, expected);

        let three_byte = 1_002_137;
        let expected = Bytes::from_iter([0b10111101, 0b10010101, 0b00011001]);
        let actual = encoder.encode_u29(three_byte).unwrap();
        assert_eq!(actual.len(), 3);
        assert_eq!(actual, expected);

        let four_byte = 21_372_137;
        let expected = Bytes::from_iter([0b10000101, 0b10001100, 0b10011100, 0b11101001]);
        let actual = encoder.encode_u29(four_byte).unwrap();
        assert_eq!(actual.len(), 4);
        assert_eq!(actual, expected);
    }

    #[test]
    fn encode_integer_test() {
        let mut encoder = Amf3EncoderState::new(BytesMut::new());
        encoder.put_integer(-2137).unwrap();
        let expected = Bytes::from_iter([INTEGER, 0b11111111, 0b11111111, 0b11110111, 0b10100111]);
        let actual = encoder.buf.freeze();
        assert_eq!(actual, expected);

        let mut encoder = Amf3EncoderState::new(BytesMut::new());
        encoder.put_integer(-(1 << 28)).unwrap();
        let expected = Bytes::from_iter([INTEGER, 0b11000000, 0b10000000, 0b10000000, 0b00000000]);
        let actual = encoder.buf.freeze();
        assert_eq!(actual, expected);
    }

    #[test]
    #[should_panic(expected = "OutOfRangeInteger")]
    fn encode_integer_out_of_bounds_test() {
        let mut encoder = Amf3EncoderState::new(BytesMut::new());

        let too_large = (1 << 28) + 3;
        encoder.put_integer(too_large).unwrap();
    }
}
