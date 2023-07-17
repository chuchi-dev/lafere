use crate::WireType;
use crate::varint::Varint;

use std::fmt;

use bytes::{Bytes, BytesRead, BytesReadRef};


#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
	UnexpectedEof,
	ExpectedEof,
	InvalidVarint,
	InvalidWireType(u8),
	WireTypeMismatch,
	ExpectedVarintWireType,
	ExpectedI32WireType,
	ExpectedI64WireType,
	ExpectedLenWireType,
	ExpectedUtf8,
	ExpectedArrayLen(usize),
	Other(String)
}

impl fmt::Display for DecodeError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::UnexpectedEof => write!(f, "unexpected end of file"),
			Self::ExpectedEof => write!(f, "expected end of file"),
			Self::InvalidVarint => write!(f, "varint is invalid"),
			Self::InvalidWireType(t) => {
				write!(f, "the wiretype {t} is invalid")
			},
			Self::WireTypeMismatch => write!(f, "wire types don't match"),
			Self::ExpectedVarintWireType => {
				write!(f, "expected a varint wire type")
			},
			Self::ExpectedI32WireType => write!(f, "expected a i32 wire type"),
			Self::ExpectedI64WireType => write!(f, "expected a i64 wire type"),
			Self::ExpectedLenWireType => {
				write!(f, "expected the len wire type")
			},
			Self::ExpectedUtf8 => write!(f, "expected a valid utf8 string"),
			Self::ExpectedArrayLen(n) => {
				write!(f, "expected an array length of {n}")
			},
			Self::Other(s) => write!(f, "decode error: {s}")
		}
	}
}

impl std::error::Error for DecodeError {}

#[derive(Debug)]
pub struct MessageDecoder<'a> {
	inner: Bytes<'a>
}

impl<'a> MessageDecoder<'a> {
	pub fn new(bytes: &'a [u8]) -> Self {
		Self {
			inner: Bytes::from(bytes)
		}
	}

	pub fn try_from_kind(kind: FieldKind<'a>) -> Result<Self, DecodeError> {
		kind.try_unwrap_len().map(Self::new)
	}

	pub(crate) fn next_varint(&mut self) -> Result<u64, DecodeError> {
		Varint::read(&mut self.inner)
			.map(|v| v.0)
			.map_err(|_| DecodeError::InvalidVarint)
	}

	fn next_kind(
		&mut self,
		ty: WireType
	) -> Result<FieldKind<'a>, DecodeError> {
		let kind = match ty {
			WireType::Varint => FieldKind::Varint(self.next_varint()?),
			WireType::I64 => FieldKind::I64(
				self.inner.try_read_le_u64()
					.map_err(|_| DecodeError::UnexpectedEof)?
			),
			WireType::I32 => FieldKind::I32(
				self.inner.try_read_le_u32()
					.map_err(|_| DecodeError::UnexpectedEof)?
			),
			WireType::Len => {
				let len = self.next_varint()?;
				let bytes = self.inner.try_read_ref(len as usize)
					.map_err(|_| DecodeError::UnexpectedEof)?;

				FieldKind::Len(bytes)
			}
		};

		Ok(kind)
	}

	/// should only be used for reading packed values
	pub(crate) fn maybe_next_kind(
		&mut self,
		ty: WireType
	) -> Result<Option<FieldKind<'a>>, DecodeError> {
		if self.inner.remaining().is_empty() {
			return Ok(None)
		}

		self.next_kind(ty).map(Some)
	}

	/// If this returns Ok(None), this means there will never be any
	/// more fields
	pub fn next(&mut self) -> Result<Option<Field<'a>>, DecodeError> {
		if self.inner.remaining().is_empty() {
			return Ok(None)
		}

		let tag = self.next_varint()?;
		let wtype = WireType::from_tag(tag)?;
		let number = tag >> 3;

		let kind = self.next_kind(wtype)?;

		Ok(Some(Field { number, kind }))
	}

	pub fn finish(self) -> Result<(), DecodeError> {
		if self.inner.remaining().is_empty() {
			Ok(())
		} else {
			Err(DecodeError::ExpectedEof)
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field<'a> {
	pub number: u64,
	pub kind: FieldKind<'a>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind<'a> {
	// backwards compatible:
	// - int32, uint32, int64, uint64, bool
	// - sint32, sint64
	Varint(u64),
	// backwards compatible:
	// - fixed32, sfixed32
	I32(u32),
	// backwards compatible:
	// - fixed64, sfixed64
	I64(u64),
	
	Len(&'a [u8])
}

impl<'a> FieldKind<'a> {
	pub fn is_len(&self) -> bool {
		matches!(self, Self::Len(_))
	}

	pub fn wire_type(&self) -> WireType {
		match self {
			Self::Varint(_) => WireType::Varint,
			Self::I32(_) => WireType::I32,
			Self::I64(_) => WireType::I64,
			Self::Len(_) => WireType::Len
		}
	}

	pub fn try_unwrap_varint(&self) -> Result<u64, DecodeError> {
		match self {
			Self::Varint(n) => Ok(*n),
			_ => Err(DecodeError::ExpectedVarintWireType)
		}
	}

	pub fn try_unwrap_i32(&self) -> Result<u32, DecodeError> {
		match self {
			Self::I32(n) => Ok(*n),
			_ => Err(DecodeError::ExpectedI32WireType)
		}
	}

	pub fn try_unwrap_i64(&self) -> Result<u64, DecodeError> {
		match self {
			Self::I64(n) => Ok(*n),
			_ => Err(DecodeError::ExpectedI64WireType)
		}
	}

	/// Returns ExpectedLenWireType if the kind is not Len
	pub fn try_unwrap_len(&self) -> Result<&'a [u8], DecodeError> {
		match self {
			Self::Len(b) => Ok(b),
			_ => Err(DecodeError::ExpectedLenWireType)
		}
	}
}



pub trait DecodeMessage<'m> {
	/// This field is just a hint, merge might accept another type
	/// 
	/// mostly this is used for detecting if we can pack a message
	const WIRE_TYPE: WireType;

	fn parse_from_bytes(b: &'m [u8]) -> Result<Self, DecodeError>
	where Self: Sized {
		let mut this = Self::decode_default();

		this.merge(FieldKind::Len(b), false)?;

		Ok(this)
	}

	fn decode_default() -> Self;

	/// kind does not need to be the same as Self::WIRE_TYPE
	/// 
	/// is_field is true if this message is a field of a struct or enum
	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		is_field: bool
	) -> Result<(), DecodeError>;
}

pub trait DecodeMessageOwned: for<'m> DecodeMessage<'m> {}

impl<T> DecodeMessageOwned for T
where T: for<'m> DecodeMessage<'m> {}

// a vec is represented as repeated ty = 1;
impl<'m, V> DecodeMessage<'m> for Vec<V>
where V: DecodeMessage<'m> {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		Self::new()
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		is_field: bool
	) -> Result<(), DecodeError> {
		// if this is not a field
		// we need to create a struct / message
		// which contains one field which is repeatable
		if !is_field {
			let mut parser = MessageDecoder::try_from_kind(kind)?;

			while let Some(field) = parser.next()? {
				if field.number != 1 {
					continue
				}

				// were now in a field of our virtual message/struct
				self.merge(field.kind, true)?;
			}

			return parser.finish();
		}

		// the data could be packet
		if kind.is_len() && V::WIRE_TYPE.can_be_packed() {
			let mut parser = MessageDecoder::try_from_kind(kind)?;
			while let Some(k) = parser.maybe_next_kind(V::WIRE_TYPE)? {
				let mut v = V::decode_default();
				v.merge(k, false)?;

				self.push(v);
			}

			return parser.finish()
		}


		let mut v = V::decode_default();
		v.merge(kind, false)?;

		self.push(v);

		Ok(())
	}
}

impl<'m> DecodeMessage<'m> for Vec<u8> {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		Self::new()
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		_is_field: bool
	) -> Result<(), DecodeError> {
		let bytes = kind.try_unwrap_len()?;
		self.clear();
		self.extend_from_slice(bytes);

		Ok(())
	}
}

impl<'m, const S: usize> DecodeMessage<'m> for [u8; S] {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		[0; S]
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		_is_field: bool
	) -> Result<(), DecodeError> {
		let bytes = kind.try_unwrap_len()?;

		if bytes.len() != S {
			return Err(DecodeError::ExpectedArrayLen(S))
		}

		self.copy_from_slice(bytes);

		Ok(())
	}
}

impl<'m> DecodeMessage<'m> for String {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		Self::new()
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		_is_field: bool
	) -> Result<(), DecodeError> {
		let bytes = kind.try_unwrap_len()?;
		self.clear();
		let s = std::str::from_utf8(bytes)
			.map_err(|_| DecodeError::ExpectedUtf8)?;
		self.push_str(s);

		Ok(())
	}
}

impl<'m, V> DecodeMessage<'m> for Option<V>
where V: DecodeMessage<'m> {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		None
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		is_field: bool
	) -> Result<(), DecodeError> {
		// if this is not a field
		// we need to create a struct / message
		// which contains one field which represent V
		if !is_field {
			let mut parser = MessageDecoder::try_from_kind(kind)?;

			while let Some(field) = parser.next()? {
				if field.number != 1 {
					continue
				}

				// were now in a field of our virtual message/struct
				self.merge(field.kind, true)?;
			}

			return parser.finish();
		}

		match self {
			Some(v) => {
				v.merge(kind, false)?;
			}
			None => {
				let mut v = V::decode_default();
				v.merge(kind, false)?;
				*self = Some(v);
			}
		}

		Ok(())
	}
}

impl<'m> DecodeMessage<'m> for bool {
	const WIRE_TYPE: WireType = WireType::Varint;

	fn decode_default() -> Self {
		false
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		_is_field: bool
	) -> Result<(), DecodeError> {
		let num = kind.try_unwrap_varint()?;
		*self = num != 0;

		Ok(())
	}
}

// impl basic varint
macro_rules! impl_varint {
	($($ty:ty),*) => ($(
		impl<'m> DecodeMessage<'m> for $ty {
			const WIRE_TYPE: WireType = WireType::Varint;

			fn decode_default() -> Self {
				Default::default()
			}

			fn merge(
				&mut self,
				kind: FieldKind<'m>,
				_is_field: bool
			) -> Result<(), DecodeError> {
				let num = kind.try_unwrap_varint()?;
				*self = num as $ty;

				Ok(())
			}
		}
	)*)
}

impl_varint![i32, i64, u32, u64];

macro_rules! impl_floats {
	($($src:ident, $wtype:ident as $ty:ty),*) => ($(
		impl<'m> DecodeMessage<'m> for $ty {
			const WIRE_TYPE: WireType = WireType::$wtype;

			fn decode_default() -> Self {
				Default::default()
			}

			fn merge(
				&mut self,
				kind: FieldKind<'m>,
				_is_field: bool
			) -> Result<(), DecodeError> {
				let num = kind.$src()?;
				*self = num as $ty;

				Ok(())
			}
		}
	)*)
}

impl_floats![
	try_unwrap_i32, I32 as f32,
	try_unwrap_i64, I64 as f64
];

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ZigZag<T>(pub T);

macro_rules! impl_zigzag {
	($($ty:ty),*) => ($(
		impl<'m> DecodeMessage<'m> for ZigZag<$ty> {
			const WIRE_TYPE: WireType = WireType::Varint;

			fn decode_default() -> Self {
				Default::default()
			}

			fn merge(
				&mut self,
				kind: FieldKind<'m>,
				_is_field: bool
			) -> Result<(), DecodeError> {
				let num = kind.try_unwrap_varint()? as $ty;
				let num = (num >> 1) ^ -(num & 1);
				*self = ZigZag(num);

				Ok(())
			}
		}
	)*)
}

impl_zigzag![i32, i64];

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Fixed<T>(pub T);

macro_rules! impl_fixed {
	($($src:ident, $wtype:ident as $ty:ty),*) => ($(
		impl<'m> DecodeMessage<'m> for Fixed<$ty> {
			const WIRE_TYPE: WireType = WireType::$wtype;

			fn decode_default() -> Self {
				Default::default()
			}

			fn merge(
				&mut self,
				kind: FieldKind<'m>,
				_is_field: bool
			) -> Result<(), DecodeError> {
				let num = kind.$src()?;
				*self = Fixed(num as $ty);

				Ok(())
			}
		}
	)*)
}

impl_fixed![
	try_unwrap_i32, I32 as u32, try_unwrap_i32, I32 as i32,
	try_unwrap_i64, I64 as u64, try_unwrap_i64, I64 as i64
];


#[cfg(test)]
mod tests {
	use super::*;

	use hex_literal::hex;

	#[test]
	fn string_and_repeated_test_4() {
		const MSG: &[u8] = &hex!("220568656c6c6f280128022803");

		let mut parser = MessageDecoder::new(MSG);

		let hello_str = Field { number: 4, kind: FieldKind::Len(b"hello") };
		assert_eq!(parser.next().unwrap().unwrap(), hello_str);

		let mut repeated = Field { number: 5, kind: FieldKind::Varint(1) };

		assert_eq!(parser.next().unwrap().unwrap(), repeated);
		repeated.kind = FieldKind::Varint(2);
		assert_eq!(parser.next().unwrap().unwrap(), repeated);
		repeated.kind = FieldKind::Varint(3);
		assert_eq!(parser.next().unwrap().unwrap(), repeated);

		assert!(parser.next().unwrap().is_none());
	}

	#[test]
	fn repeated_packet() {
		const MSG: &[u8] = &hex!("3206038e029ea705");

		let mut parser = MessageDecoder::new(MSG);

		let packed = parser.next().unwrap().unwrap();
		assert_eq!(packed.number, 6);
		let packed = match packed.kind {
			FieldKind::Len(p) => p,
			_ => panic!()
		};

		let mut parser = MessageDecoder::new(packed);
		assert_eq!(parser.next_varint().unwrap(), 3);
		assert_eq!(parser.next_varint().unwrap(), 270);
		assert_eq!(parser.next_varint().unwrap(), 86942);
	}

	#[test]
	fn empty_bytes() {
		const MSG: &[u8] = &[10, 0];

		let mut parser = MessageDecoder::new(MSG);

		let field = parser.next().unwrap().unwrap();
		assert_eq!(field.number, 1);
		assert_eq!(field.kind, FieldKind::Len(&[]));
		assert!(parser.next().unwrap().is_none());
	}

	/*
	message Target {
		oneof target {
			Unknown unknown = 1;
			Unit unit = 2;
			Weapon weapon = 3;
			Static static = 4;
			Scenery scenery = 5;
			Airbase airbase = 6;
			Cargo cargo = 7;
		}
	}
	*/


	// struct Test {
		
	// }

	// impl Message for Test {
	// 	fn parse(r) -> Result<Self, Error> {

	// 	}

	// 	fn merge_field(&self, )
	// }




}