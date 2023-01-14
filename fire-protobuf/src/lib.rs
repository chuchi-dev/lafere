mod varint;
pub mod decode;
pub mod encode;
#[cfg(test)]
pub mod test_util;
#[cfg(test)]
pub mod tests;

use decode::DecodeError;

pub use bytes;
pub use codegen::*;


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