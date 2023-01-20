use crate::encode::{
	EncodeMessage, MessageEncoder, EncodeError, FieldOpt, SizeBuilder
};
use crate::decode::{DecodeMessage, MessageDecoder, FieldKind, DecodeError};
use crate::bytes::{BytesOwned, BytesRead, BytesWrite};
pub use crate::WireType;

use std::fmt::Debug;


#[derive(Debug, Default, PartialEq, Eq)]
pub struct Wrapper<T>(pub T);

impl<T> EncodeMessage for Wrapper<T>
where T: EncodeMessage {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool { false }

	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		let mut size = SizeBuilder::new();
		self.0.encoded_size(Some(FieldOpt::new(1)), &mut size)?;
		let size = size.finish();

		if let Some(field) = field {
			builder.write_tag(field.num, WireType::Len);
			builder.write_len(size);
		}

		builder.write_bytes(size);

		Ok(())
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		if let Some(field) = field {
			encoder.write_tag(field.num, WireType::Len)?;

			let mut size = SizeBuilder::new();
			self.0.encoded_size(Some(FieldOpt::new(1)), &mut size)?;
			let size = size.finish();

			encoder.write_len(size)?;
		}

		self.0.encode(Some(FieldOpt::new(1)), encoder)
	}
}

impl<'m, T> DecodeMessage<'m> for Wrapper<T>
where T: DecodeMessage<'m> {
	const WIRE_TYPE: WireType = WireType::Len;

	fn decode_default() -> Self {
		Self(T::decode_default())
	}

	fn merge(
		&mut self,
		kind: FieldKind<'m>,
		_is_field: bool
	) -> Result<(), DecodeError> {
		let mut decoder = MessageDecoder::try_from_kind(kind)?;

		while let Some(field) = decoder.next()? {
			if field.number == 1 {
				self.0.merge(field.kind, true)?;
			}
		}

		decoder.finish()
	}
}

pub struct TestWriter {
	inner: MessageEncoder<BytesOwned>
}

impl TestWriter {
	pub fn new() -> Self {
		Self {
			inner: MessageEncoder::new_owned()
		}
	}

	pub fn write_tag(mut self, fieldnum: u64, wtype: WireType) -> Self {
		self.inner.write_tag(fieldnum, wtype).unwrap();
		self
	}

	pub fn write_len(mut self, len: u64) -> Self {
		self.inner.write_len(len).unwrap();
		self
	}

	pub fn write_bytes(mut self, bytes: &[u8]) -> Self {
		self.inner.write_bytes(bytes).unwrap();
		self
	}

	pub fn write_varint(mut self, val: u64) -> Self {
		self.inner.write_varint(val).unwrap();
		self
	}

	#[track_caller]
	pub fn cmp<T>(self, mut other: T) -> Self
	where T: EncodeMessage + for<'a> DecodeMessage<'a> + Debug + Eq {
		let n_bytes = other.write_to_bytes().unwrap();
		assert_eq!(self.inner.inner().as_slice(), n_bytes);

		let n_v = T::parse_from_bytes(self.inner.inner().as_slice())
			.unwrap();
		assert_eq!(other, n_v);

		self
	}
}