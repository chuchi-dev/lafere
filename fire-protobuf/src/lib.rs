#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

mod varint;
pub mod decode;
pub mod encode;
#[cfg(test)]
pub mod test_util;
#[cfg(test)]
pub mod tests;

use decode::{DecodeMessage, DecodeError};
use encode::{MessageEncoder, EncodeMessage, EncodeError};

pub use bytes;
pub use codegen::*;


pub fn from_slice<'a, T>(slice: &'a [u8]) -> Result<T, DecodeError>
where T: DecodeMessage<'a> {
	T::parse_from_bytes(slice)
}

pub fn to_vec<T>(msg: &mut T) -> Result<Vec<u8>, EncodeError>
where T: EncodeMessage {
	msg.write_to_bytes()
}

pub fn to_bytes_writer<T, W>(msg: &mut T, w: &mut W) -> Result<(), EncodeError>
where
	T: EncodeMessage,
	W: bytes::BytesWrite
{
	let mut encoder = MessageEncoder::new(w);

	msg.encode(None, &mut encoder)
}

// todo move this into a module
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
	Varint,
	I32,
	I64,
	Len
}

impl WireType {
	pub(crate) fn from_tag(tag: u64) -> Result<Self, DecodeError> {
		let b = tag as u8 & 0b0000_0111;

		match b {
			0 => Ok(Self::Varint),
			1 => Ok(Self::I64),
			2 => Ok(Self::Len),
			5 => Ok(Self::I32),
			b => Err(DecodeError::InvalidWireType(b))
		}
	}

	pub(crate) fn as_num(&self) -> u8 {
		match self {
			Self::Varint => 0,
			Self::I64 => 1,
			Self::Len => 2,
			Self::I32 => 5
		}
	}

	pub(crate) const fn can_be_packed(&self) -> bool {
		matches!(self, Self::Varint | Self::I32 | Self::I64)
	}

	pub fn is_len(&self) -> bool {
		matches!(self, Self::Len)
	}
}