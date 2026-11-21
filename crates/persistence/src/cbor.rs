//! Deterministic CBOR subset used by the versioned save records.

use std::io;

use crate::invalid;

pub(crate) struct Encoder(pub(crate) Vec<u8>);

impl Encoder {
    fn argument(&mut self, kind: u8, value: u64) {
        let prefix = kind << 5;
        match value {
            0..=23 => self.0.push(prefix | value as u8),
            24..=255 => self.0.extend_from_slice(&[prefix | 24, value as u8]),
            256..=65535 => {
                self.0.push(prefix | 25);
                self.0.extend_from_slice(&(value as u16).to_be_bytes());
            }
            65536..=4294967295 => {
                self.0.push(prefix | 26);
                self.0.extend_from_slice(&(value as u32).to_be_bytes());
            }
            _ => {
                self.0.push(prefix | 27);
                self.0.extend_from_slice(&value.to_be_bytes());
            }
        }
    }

    pub(crate) fn unsigned(&mut self, value: u64) {
        self.argument(0, value);
    }

    pub(crate) fn signed(&mut self, value: i64) {
        if value >= 0 {
            self.unsigned(value as u64);
        } else {
            self.argument(1, (!value) as u64);
        }
    }

    pub(crate) fn array(&mut self, length: usize) {
        self.argument(4, length as u64);
    }

    pub(crate) fn text(&mut self, value: &str) {
        self.argument(3, value.len() as u64);
        self.0.extend_from_slice(value.as_bytes());
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.argument(2, bytes.len() as u64);
        self.0.extend_from_slice(bytes);
    }

    pub(crate) fn float_bits(&mut self, value: f64) {
        self.bytes(&value.to_be_bytes());
    }
}

pub(crate) struct Decoder<'a>(pub(crate) &'a [u8]);

impl<'a> Decoder<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let (bytes, remaining) = self
            .0
            .split_at_checked(count)
            .ok_or_else(|| invalid("truncated CBOR"))?;
        self.0 = remaining;
        Ok(bytes)
    }

    fn argument(&mut self, expected: u8) -> io::Result<u64> {
        let header = self.take(1)?[0];
        if header >> 5 != expected {
            return Err(invalid("unexpected CBOR value type"));
        }
        let additional = header & 31;
        let (value, minimum) = match additional {
            0..=23 => return Ok(additional as u64),
            24 => (u64::from(self.take(1)?[0]), 24),
            25 => (
                u64::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap())),
                256,
            ),
            26 => (
                u64::from(u32::from_be_bytes(self.take(4)?.try_into().unwrap())),
                65536,
            ),
            27 => (
                u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
                4294967296,
            ),
            _ => return Err(invalid("indefinite or reserved CBOR argument")),
        };
        if value < minimum {
            return Err(invalid("non-deterministic CBOR argument encoding"));
        }
        Ok(value)
    }

    pub(crate) fn unsigned(&mut self) -> io::Result<u64> {
        self.argument(0)
    }

    pub(crate) fn signed(&mut self) -> io::Result<i64> {
        let negative = self.0.first().is_some_and(|header| header >> 5 == 1);
        let value = i64::try_from(self.argument(u8::from(negative))?)
            .map_err(|_| invalid("CBOR integer out of range"))?;
        Ok(if negative { !value } else { value })
    }

    fn length(&mut self, kind: u8, limit: usize) -> io::Result<usize> {
        let length = self.argument(kind)?;
        usize::try_from(length)
            .ok()
            .filter(|&n| n <= limit)
            .ok_or_else(|| invalid("CBOR field exceeds its size limit"))
    }

    pub(crate) fn array(&mut self, limit: usize) -> io::Result<usize> {
        self.length(4, limit)
    }

    pub(crate) fn fixed(&mut self, count: usize) -> io::Result<()> {
        if self.array(count)? != count {
            return Err(invalid("unexpected CBOR record width"));
        }
        Ok(())
    }

    pub(crate) fn text(&mut self) -> io::Result<String> {
        let length = self.length(3, 1024)?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| invalid("invalid UTF-8 in CBOR"))?;
        Ok(value.into())
    }

    pub(crate) fn bytes(&mut self, limit: usize) -> io::Result<&'a [u8]> {
        let size = self.length(2, limit)?;
        self.take(size)
    }

    pub(crate) fn float_bits(&mut self) -> io::Result<f64> {
        let bytes = self
            .bytes(8)?
            .try_into()
            .map_err(|_| invalid("invalid f64 byte width"))?;
        Ok(f64::from_be_bytes(bytes))
    }

    pub(crate) fn finish(self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(invalid("trailing data in CBOR record"))
        }
    }
}
