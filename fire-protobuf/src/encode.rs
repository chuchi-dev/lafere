use crate::WireType;
use crate::varint::Varint;

use std::fmt;
use std::collections::HashMap;

use bytes::{BytesOwned, BytesWrite, BytesRead};


#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
	BufferExausted,
	Other(String)
}

impl fmt::Display for EncodeError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::BufferExausted => write!(f, "the buffer was to small"),
			Self::Other(s) => write!(f, "encode error: {s}")
		}
	}
}

impl std::error::Error for EncodeError {}

#[derive(Debug)]
pub struct MessageEncoder<B> {
	inner: B
}

impl<B> MessageEncoder<B> {
	pub fn new(inner: B) -> Self {
		Self {
			inner
		}
	}

	pub fn inner(&self) -> &B {
		&self.inner
	}

	pub fn finish(self) -> B {
		self.inner
	}
}

impl MessageEncoder<BytesOwned> {
	pub fn new_owned() -> Self {
		Self {
			inner: BytesOwned::new()
		}
	}
}


impl<B> MessageEncoder<B>
where B: BytesWrite {
	pub fn write_tag(
		&mut self,
		fieldnum: u64,
		wtype: WireType
	) -> Result<(), EncodeError> {
		let mut tag = Varint(fieldnum << 3);
		tag.0 |= wtype.as_num() as u64;

		tag.write(&mut self.inner).map_err(|_| EncodeError::BufferExausted)
	}

	pub fn write_len(&mut self, len: u64) -> Result<(), EncodeError> {
		Varint(len).write(&mut self.inner)
			.map_err(|_| EncodeError::BufferExausted)
	}

	pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
		self.inner.try_write(bytes).map_err(|_| EncodeError::BufferExausted)
	}

	pub fn written_len(&self) -> u64 {
		self.inner.as_bytes().len() as u64
	}

	pub fn write_varint(&mut self, val: u64) -> Result<(), EncodeError> {
		Varint(val).write(&mut self.inner)
			.map_err(|_| EncodeError::BufferExausted)
	}

	pub fn write_i32(&mut self, val: u32) -> Result<(), EncodeError> {
		self.inner.try_write_le_u32(val)
			.map_err(|_| EncodeError::BufferExausted)
	}

	pub fn write_i64(&mut self, val: u64) -> Result<(), EncodeError> {
		self.inner.try_write_le_u64(val)
			.map_err(|_| EncodeError::BufferExausted)
	}

	pub fn write_empty_field(
		&mut self,
		fieldnum: u64
	) -> Result<(), EncodeError> {
		self.write_tag(fieldnum, WireType::Len)?;
		self.write_len(0)
	}
}

impl From<MessageEncoder<BytesOwned>> for Vec<u8> {
	fn from(w: MessageEncoder<BytesOwned>) -> Self {
		w.inner.into()
	}
}

#[derive(Debug)]
pub struct SizeBuilder {
	inner: u64
}

impl SizeBuilder {
	pub fn new() -> Self {
		Self {
			inner: 0
		}
	}

	pub fn write_tag(
		&mut self,
		fieldnum: u64,
		_wtype: WireType
	) {
		self.inner += Varint(fieldnum << 3).size();
	}

	pub fn write_len(&mut self, len: u64) {
		// need to get the varint len
		self.write_varint(len);
	}

	pub fn write_varint(&mut self, val: u64) {
		self.inner += Varint(val).size();
	}

	pub fn write_i32(&mut self, _val: u32) {
		self.inner += 4;
	}

	pub fn write_i64(&mut self, _val: u64) {
		self.inner += 8;
	}

	pub fn write_bytes(&mut self, len: u64) {
		self.inner += len;
	}

	pub fn write_empty_field(&mut self, fieldnum: u64) {
		self.write_tag(fieldnum, WireType::Len);
		self.write_len(0);
	}

	pub fn finish(self) -> u64 {
		self.inner
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldOpt {
	pub num: u64,
	pub is_nested: bool
}

impl FieldOpt {
	pub const fn new(num: u64) -> Self {
		Self {
			num,
			is_nested: false
		}
	}
}

/// ## Ignoring fields
/// if your call tells you a field number you need to write it even if you
/// have the default value
/// 
/// You can only ignore writing fields if you wan't
pub trait EncodeMessage {
	fn write_to_bytes(&mut self) -> Result<Vec<u8>, EncodeError> {
		// we need to call
		let mut encoder = MessageEncoder::new_owned();

		self.encode(None, &mut encoder)?;

		Ok(encoder.into())
	}

	/// at the moment only used to check if this message can be packed
	const WIRE_TYPE: WireType;

	fn is_default(&self) -> bool;

	/// how big will the size be after encoding
	/// 
	/// The that get's returned here needs to be the same as called in write
	///
	/// if fieldnum is set this means you *need* to write the tag
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError>;

	/// In most cases before this is called encoded_size get's called
	/// 
	/// The size that get's computed in encoded_size must be the same as we get
	/// here
	/// 
	/// if fieldnum is set this means you *need* to write the tag to
	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite;
}

impl<V: EncodeMessage> EncodeMessage for &mut V {
	const WIRE_TYPE: WireType = V::WIRE_TYPE;

	fn is_default(&self) -> bool {
		(**self).is_default()
	}

	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		(**self).encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		(**self).encode(field, encoder)
	}
}

macro_rules! impl_from_ref {
	($ty:ty) => (
		impl EncodeMessage for $ty {
			const WIRE_TYPE: WireType = <&$ty>::WIRE_TYPE;

			fn is_default(&self) -> bool {
				(&&*self).is_default()
			}

			fn encoded_size(
				&mut self,
				field: Option<FieldOpt>,
				builder: &mut SizeBuilder
			) -> Result<(), EncodeError> {
				(&mut &*self).encoded_size(field, builder)
			}

			fn encode<B>(
				&mut self,
				field: Option<FieldOpt>,
				encoder: &mut MessageEncoder<B>
			) -> Result<(), EncodeError>
			where B: BytesWrite {
				(&mut &*self).encode(field, encoder)
			}
		}
	)
}


impl<V> EncodeMessage for Vec<V>
where V: EncodeMessage {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_empty()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_mut_slice().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_mut_slice().encode(field, encoder)
	}
}

impl<V, const S: usize> EncodeMessage for [V; S]
where V: EncodeMessage {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_empty()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_mut_slice().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_mut_slice().encode(field, encoder)
	}
}


impl<V> EncodeMessage for [V]
where V: EncodeMessage {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_empty()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		// if we don't have a fieldnum we need to simulate a custom field
		let field = field.unwrap_or(FieldOpt::new(1));

		// if this fieldnumber cannot be repeated we need to simulate another
		// field
		if field.is_nested {
			builder.write_tag(field.num, WireType::Len);

			let mut size = SizeBuilder::new();
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			builder.write_len(size);
			builder.write_bytes(size);
			return Ok(())
		}

		// if we are packed
		if V::WIRE_TYPE.can_be_packed() && self.len() > 1 {
			builder.write_tag(field.num, WireType::Len);

			let mut packed_builder = SizeBuilder::new();
			for v in self.iter_mut() {
				v.encoded_size(None, &mut packed_builder)?;
			}
			// we now know how big a packed field is
			let packed_size = packed_builder.finish();

			builder.write_len(packed_size);
			builder.write_bytes(packed_size);
			return Ok(())
		}

		// we need to create a field for every entry
		for v in self.iter_mut() {
			let field = FieldOpt {
				num: field.num,
				is_nested: true
			};
			v.encoded_size(Some(field), builder)?;
		}

		Ok(())
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		// if we don't have a fieldnum we need to simulate a custom field
		let field = field.unwrap_or(FieldOpt::new(1));

		// if this fieldnumber cannot be repeated we need to simulate another
		// field
		if field.is_nested {
			encoder.write_tag(field.num, WireType::Len)?;

			let mut size = SizeBuilder::new();
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			encoder.write_len(size)?;

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			self.encode(None, encoder)?;

			#[cfg(debug_assertions)]
			{
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(size, added_len as u64,
					"size does not match real size");
			}

			return Ok(())
		}

		// if we are packed
		if V::WIRE_TYPE.can_be_packed() && self.len() > 1 {
			encoder.write_tag(field.num, WireType::Len)?;

			let mut packed_builder = SizeBuilder::new();
			for v in self.iter_mut() {
				v.encoded_size(None, &mut packed_builder)?;
			}
			// we now know how big a packed field is
			let packed_size = packed_builder.finish();

			encoder.write_len(packed_size)?;

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			for v in self.iter_mut() {
				v.encode(None, encoder)?;
			}

			#[cfg(debug_assertions)]
			{
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(packed_size, added_len as u64,
					"size does not match real size");
			}

			return Ok(())
		}

		// we need to create a field for every entry
		for v in self.iter_mut() {
			let field = FieldOpt {
				num: field.num,
				is_nested: true
			};
			v.encode(Some(field), encoder)?;
		}

		Ok(())
	}
}

impl<K, V> EncodeMessage for HashMap<K, V>
where
	for<'a> &'a K: EncodeMessage,
	V: EncodeMessage
{
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_empty()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		// if we don't have a fieldnum we need to simulate a custom field
		let field = field.unwrap_or(FieldOpt::new(1));

		// if this fieldnumber cannot be repeated we need to simulate another
		// field
		if field.is_nested {
			builder.write_tag(field.num, WireType::Len);

			let mut size = SizeBuilder::new();
			// this works since FieldOpt::new(1) will never be nested
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			builder.write_len(size);
			builder.write_bytes(size);
			return Ok(())
		}

		// we need to create a field for every entry
		for (k, v) in self.iter_mut() {
			let field = FieldOpt {
				num: field.num,
				is_nested: true
			};
			(k, v).encoded_size(Some(field), builder)?;
		}

		Ok(())
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		// if we don't have a fieldnum we need to simulate a custom field
		let field = field.unwrap_or(FieldOpt::new(1));

		// if this fieldnumber cannot be repeated we need to simulate another
		// field
		if field.is_nested {
			encoder.write_tag(field.num, WireType::Len)?;

			let mut size = SizeBuilder::new();
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			encoder.write_len(size)?;

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			self.encode(None, encoder)?;

			#[cfg(debug_assertions)]
			{
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(size, added_len as u64,
					"size does not match real size");
			}

			return Ok(())
		}

		// we need to create a field for every entry
		for (k, v) in self.iter_mut() {
			let field = FieldOpt {
				num: field.num,
				is_nested: true
			};
			(k, v).encode(Some(field), encoder)?;
		}

		Ok(())
	}
}

impl EncodeMessage for Vec<u8> {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.as_slice().is_default()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_slice().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_slice().encode(field, encoder)
	}
}

impl<const S: usize> EncodeMessage for [u8; S] {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.as_slice().is_default()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_slice().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_slice().encode(field, encoder)
	}
}

/// a tuple behaves the same way as a struct
macro_rules! impl_tuple {
	($($gen:ident, $idx:tt),*) => (
		impl<$($gen),*> EncodeMessage for ($($gen),*)
		where
			$($gen: EncodeMessage),*
		{
			const WIRE_TYPE: WireType = WireType::Len;

			fn is_default(&self) -> bool {
				false
			}

			/// how big will the size be after writing
			fn encoded_size(
				&mut self,
				field: Option<FieldOpt>,
				builder: &mut SizeBuilder
			) -> Result<(), EncodeError> {
				let mut size = SizeBuilder::new();
				$(
					if !self.$idx.is_default() {
						self.$idx.encoded_size(
							Some(FieldOpt::new($idx)),
							&mut size
						)?;
					}
				)*
				let fields_size = size.finish();

				if let Some(field) = field {
					builder.write_tag(field.num, Self::WIRE_TYPE);
					builder.write_len(fields_size);
				}

				builder.write_bytes(fields_size);

				Ok(())
			}

			fn encode<Bytes>(
				&mut self,
				field: Option<FieldOpt>,
				encoder: &mut MessageEncoder<Bytes>
			) -> Result<(), EncodeError>
			where Bytes: BytesWrite {
				#[cfg(debug_assertions)]
				let mut dbg_fields_size = None;

				// we don't need to get the size if we don't need to write
				// the size
				if let Some(field) = field {
					encoder.write_tag(field.num, Self::WIRE_TYPE)?;

					let mut size = SizeBuilder::new();
					$(
						if !self.$idx.is_default() {
							self.$idx.encoded_size(
								Some(FieldOpt::new($idx)),
								&mut size
							)?;
						}
					)*
					let fields_size = size.finish();

					encoder.write_len(fields_size)?;

					#[cfg(debug_assertions)]
					{
						dbg_fields_size = Some(fields_size);
					}
				}

				#[cfg(debug_assertions)]
				let prev_len = encoder.written_len();

				$(
					if !self.$idx.is_default() {
						self.$idx.encode(
							Some(FieldOpt::new($idx)),
							encoder
						)?;
					}
				)*

				#[cfg(debug_assertions)]
				if let Some(fields_size) = dbg_fields_size {
					let added_len = encoder.written_len() - prev_len;
					assert_eq!(fields_size, added_len as u64,
						"encoded size does not match actual size");
				}

				Ok(())
			}
		}
	)
}

// impl_tuple![
// 	A, 0
// ];
impl_tuple![
	A, 0,
	B, 1
];
impl_tuple![
	A, 0,
	B, 1,
	C, 2
];
impl_tuple![
	A, 0,
	B, 1,
	C, 2,
	D, 3
];
impl_tuple![
	A, 0,
	B, 1,
	C, 2,
	D, 3,
	E, 4
];
impl_tuple![
	A, 0,
	B, 1,
	C, 2,
	D, 3,
	E, 4,
	F, 5
];

impl_from_ref!([u8]);

impl EncodeMessage for &[u8] {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_empty()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		writer: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		if let Some(field) = field {
			writer.write_tag(field.num, Self::WIRE_TYPE);
			writer.write_len(self.len() as u64);
		}

		writer.write_bytes(self.len() as u64);

		Ok(())
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		writer: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		if let Some(field) = field {
			writer.write_tag(field.num, Self::WIRE_TYPE)?;
			writer.write_len(self.len() as u64)?;
		}

		writer.write_bytes(self)
	}
}

impl_from_ref!(String);

impl EncodeMessage for &String {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.as_bytes().is_default()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_bytes().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_bytes().encode(field, encoder)
	}
}

impl EncodeMessage for &str {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.as_bytes().is_default()
	}

	/// how big will the size be after writing
	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		self.as_bytes().encoded_size(field, builder)
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		self.as_bytes().encode(field, encoder)
	}
}

impl<T> EncodeMessage for Option<T>
where T: EncodeMessage {
	const WIRE_TYPE: WireType = WireType::Len;

	fn is_default(&self) -> bool {
		self.is_none()
	}

	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		let field = field.unwrap_or(FieldOpt::new(1));

		if field.is_nested {
			builder.write_tag(field.num, WireType::Len);

			let mut size = SizeBuilder::new();
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			builder.write_len(size);
			builder.write_bytes(size);

			return Ok(())
		}

		match self {
			Some(v) => v.encoded_size(
				Some(FieldOpt {
					num: field.num,
					is_nested: true
				}),
				builder
			),
			None => Ok(())
		}
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		let field = field.unwrap_or(FieldOpt::new(1));

		if field.is_nested {
			encoder.write_tag(field.num, WireType::Len)?;

			let mut size = SizeBuilder::new();
			self.encoded_size(None, &mut size)?;
			let size = size.finish();

			encoder.write_len(size)?;

			#[cfg(debug_assertions)]
			let prev_len = encoder.written_len();

			self.encode(None, encoder)?;

			#[cfg(debug_assertions)]
			{
				let added_len = encoder.written_len() - prev_len;
				assert_eq!(size, added_len as u64,
					"size does not match real size");
			}

			return Ok(())
		}

		match self {
			Some(v) => v.encode(
				Some(FieldOpt {
					num: field.num,
					is_nested: true
				}),
				encoder
			),
			None => Ok(())
		}
	}
}

impl_from_ref!(bool);

impl EncodeMessage for &bool {
	const WIRE_TYPE: WireType = WireType::Varint;

	fn is_default(&self) -> bool {
		**self == false
	}

	fn encoded_size(
		&mut self,
		field: Option<FieldOpt>,
		builder: &mut SizeBuilder
	) -> Result<(), EncodeError> {
		if let Some(field) = field {
			builder.write_tag(field.num, Self::WIRE_TYPE);
		}

		builder.write_varint(**self as u64);

		Ok(())
	}

	fn encode<B>(
		&mut self,
		field: Option<FieldOpt>,
		encoder: &mut MessageEncoder<B>
	) -> Result<(), EncodeError>
	where B: BytesWrite {
		if let Some(field) = field {
			encoder.write_tag(field.num, Self::WIRE_TYPE)?;
		}

		encoder.write_varint(**self as u64)
	}
}


// impl basic varint
macro_rules! impl_varint {
	($($ty:ty),*) => ($(
		impl_from_ref!($ty);

		impl EncodeMessage for &$ty {
			const WIRE_TYPE: WireType = WireType::Varint;

			fn is_default(&self) -> bool {
				**self == 0
			}

			fn encoded_size(
				&mut self,
				field: Option<FieldOpt>,
				builder: &mut SizeBuilder
			) -> Result<(), EncodeError> {
				if let Some(field) = field {
					builder.write_tag(field.num, Self::WIRE_TYPE);
				}

				builder.write_varint(**self as u64);

				Ok(())
			}

			fn encode<B>(
				&mut self,
				field: Option<FieldOpt>,
				encoder: &mut MessageEncoder<B>
			) -> Result<(), EncodeError>
			where B: BytesWrite {
				if let Some(field) = field {
					encoder.write_tag(field.num, Self::WIRE_TYPE)?;
				}

				encoder.write_varint(**self as u64)
			}
		}
	)*)
}

impl_varint![i32, i64, u32, u64];

macro_rules! impl_floats {
	($($src:ident, $wty:ident, $wtype:ident as $ty:ty),*) => ($(
		impl_from_ref!($ty);

		impl EncodeMessage for &$ty {
			const WIRE_TYPE: WireType = WireType::$wtype;

			fn is_default(&self) -> bool {
				**self == 0 as $ty
			}

			fn encoded_size(
				&mut self,
				field: Option<FieldOpt>,
				builder: &mut SizeBuilder
			) -> Result<(), EncodeError> {
				if let Some(field) = field {
					builder.write_tag(field.num, Self::WIRE_TYPE);
				}

				builder.$src(**self as $wty);

				Ok(())
			}

			fn encode<B>(
				&mut self,
				field: Option<FieldOpt>,
				encoder: &mut MessageEncoder<B>
			) -> Result<(), EncodeError>
			where B: BytesWrite {
				if let Some(field) = field {
					encoder.write_tag(field.num, Self::WIRE_TYPE)?;
				}

				encoder.$src(**self as $wty)
			}
		}
	)*)
}

impl_floats![
	write_i32, u32, I32 as f32,
	write_i64, u64, I64 as f64
];