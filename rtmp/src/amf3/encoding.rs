use bytes::{BufMut, Bytes};

use crate::{AmfEncodingError, amf3::*};

pub(crate) struct Amf3EncoderState<T> {
    buf: T,
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
            } => todo!(),
            Amf3Value::Xml(x) => todo!(),
            Amf3Value::ByteArray(ba) => todo!(),
            _ => todo!(),
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
        if i29 >= 2i32.pow(28) || i29 < -(2i32.pow(28)) {
            return Err(AmfEncodingError::OutOfRangeInteger);
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
        if s.len() > 2usize.pow(28) - 1 {
            return Err(AmfEncodingError::StringTooLong(s.len()));
        }
        self.put_marker(STRING);
        let u29s = self.encode_u29(((s.len() as u32) << 1) | 0b1)?;
        self.buf.put(u29s);
        self.buf.put_slice(s.as_bytes());
        Ok(())
    }

    fn put_xml_doc(&mut self, xd: &str) -> Result<(), AmfEncodingError> {
        if xd.len() > 2usize.pow(28) - 1 {
            return Err(AmfEncodingError::StringTooLong(xd.len()));
        }
        self.put_marker(XML_DOC);
        let u29x = self.encode_u29(((xd.len() as u32) << 1) | 0b1)?;
        self.buf.put(u29x);
        self.buf.put_slice(xd.as_bytes());
        Ok(())
    }

    fn put_date(&mut self, d: f64) -> Result<(), AmfEncodingError> {
        self.put_marker(DATE);
        self.buf.put_slice(&self.encode_u29(1)?);
        self.buf.put_f64(d);
        Ok(())
    }

    fn put_array(
        &mut self,
        associative: &HashMap<String, Amf3Value>,
        dense: &Vec<Amf3Value>,
    ) -> Result<(), AmfEncodingError> {
        if dense.len() > 2usize.pow(28) - 1 {
            return Err(AmfEncodingError::ArrayTooLong(dense.len()));
        }

        self.put_marker(ARRAY);
        let u29a = self.encode_u29(((dense.len() as u32) << 1) | 0b1)?;
        self.put_pairs(associative.into_iter().collect::<Vec<_>>())?;
        self.buf.put_u8(0x01);
        for val in dense {
            self.put_value(val)?;
        }
        Ok(())
    }

    fn put_object(
        &mut self,
        class_name: Option<&str>,
        sealed_count: usize,
        values: &[(String, Amf3Value)],
    ) -> Result<(), AmfEncodingError> {
        todo!()
    }

    fn put_xml(&mut self, x: &str) -> Result<(), AmfEncodingError> {
        if x.len() > 2usize.pow(28) - 1 {
            return Err(AmfEncodingError::StringTooLong(x.len()));
        }
        self.put_marker(XML);
        let u29x = self.encode_u29(((x.len() as u32) << 1) | 0b1)?;
        self.buf.put(u29x);
        self.buf.put_slice(x.as_bytes());
        Ok(())
    }

    fn put_pairs(&mut self, pairs: Vec<(&String, &Amf3Value)>) -> Result<(), AmfEncodingError> {
        for (key, value) in pairs {
            self.put_string(key)?;
            self.put_value(value)?;
        }
        Ok(())
    }

    fn encode_u29(&self, mut u29: u32) -> Result<Bytes, AmfEncodingError> {
        let n_bytes: usize = match u29 {
            n if n <= 2u32.pow(7) - 1 => 1,
            n if n <= 2u32.pow(14) - 1 => 2,
            n if n <= 2u32.pow(21) - 1 => 3,
            n if n <= 2u32.pow(29) - 1 => 4,
            _ => return Err(AmfEncodingError::OutOfRangeU29),
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
        let expected = Bytes::from_iter([0b11111111, 0b11111111, 0b11110111, 0b10100111]);
        let actual = encoder.buf.freeze();
        assert_eq!(actual, expected);

        let mut encoder = Amf3EncoderState::new(BytesMut::new());
        encoder.put_integer(-(1 << 28)).unwrap();
        let expected = Bytes::from_iter([0b11000000, 0b10000000, 0b10000000, 0b00000000]);
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
