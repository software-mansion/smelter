use bytes::{BufMut, Bytes};

use crate::{AmfEncodingError, amf3::Amf3Value};

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

    pub(crate) fn encode_value(&mut self, amf3_value: &Amf3Value) -> Result<(), AmfEncodingError> {
        match amf3_value {
            Amf3Value::Undefined => self.puf_undefined(),
            Amf3Value::Null => self.put_null(),
            Amf3Value::Boolean(b) => self.put_boolean(*b),
            _ => todo!(),
        }
        Ok(())
    }

    fn puf_undefined(&mut self) {
        self.buf.put_u8(0x00);
    }

    fn put_null(&mut self) {
        self.buf.put_u8(0x01);
    }

    fn put_boolean(&mut self, b: bool) {
        match b {
            false => self.buf.put_u8(0x02),
            true => self.buf.put_u8(0x03),
        }
    }

    fn put_integer(&mut self, i29: i32) -> Result<(), AmfEncodingError> {
        if i29 >= 2i32.pow(28) || i29 < -(2i32.pow(28)) {
            return Err(AmfEncodingError::OutOfRangeInteger);
        }

        self.buf.put(self.encode_i29(i29)?);
        Ok(())
    }

    fn encode_i29(&self, i29: i32) -> Result<Bytes, AmfEncodingError> {
        if i29 >= 2i32.pow(28) || i29 < -(2i32.pow(28)) {
            return Err(AmfEncodingError::OutOfRangeI29);
        }

        let u29: u32 = if i29 >= 0 {
            i29 as u32
        } else {
            match i29 {
                n if n >= -(2i32.pow(6)) => 0x40 | (n as u32 & 0x3F),
                _ => unreachable!(),
            }
        };
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
}
