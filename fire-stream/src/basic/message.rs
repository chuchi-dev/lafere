
use crate::packet::{
	self, Flags, Packet, PacketBytes,
	PacketHeader, BodyBytes, BodyBytesMut, PacketError
};

use std::fmt::Debug;
use std::hash::Hash;

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite};

/// the number zero is not allowed to be used
pub trait Action: Debug + Copy + Eq + Hash {
	/// returns an Action that will never be returned
	fn empty() -> Self;
	/// should try to convert an u16 to the action
	/// 
	/// 0 => needs to be converted to Self::empty()
	fn from_u16(num: u16) -> Option<Self>;
	fn as_u16(&self) -> u16;

	fn max_body_size(_header: &Header<Self>) -> Option<u32> {
		None
	}
}


#[derive(Debug)]
pub struct Header<A> {
	body_len: u32,
	flags: Flags,
	id: u32,
	action: A
}

impl<A> Header<A>
where A: Action {

	pub fn empty() -> Self {
		Self {
			body_len: 0,
			flags: Flags::empty(),
			id: 0,
			action: A::empty()
		}
	}

	pub fn to_bytes(&self, mut bytes: BytesMut) {
		bytes.write_u32(self.body_len);
		bytes.write_u8(self.flags.as_u8());
		bytes.write_u32(self.id);
		bytes.write_u16(self.action.as_u16());
	}

	pub fn set_action(&mut self, action: A) {
		self.action = action;
	}

}

impl<A> PacketHeader for Header<A>
where A: Action {
	fn len() -> usize {
		4 + 1 + 4 + 2
	}

	fn from_bytes(mut bytes: Bytes) -> packet::Result<Self> {
		let me = Self {
			body_len: bytes.read_u32(),
			flags: Flags::from_u8(bytes.read_u8())?,
			id: bytes.read_u32(),
			action: A::from_u16(bytes.read_u16())
				.ok_or_else(|| PacketError::Header("Action unknown".into()))?
		};

		if let Some(max) = A::max_body_size(&me) {
			if me.body_len > max {
				return Err(PacketError::Body("body to big".into()))
			}
		}

		Ok(me)
	}

	fn body_len(&self) -> usize {
		self.body_len as usize
	}

	fn flags(&self) -> &Flags {
		&self.flags
	}

	fn set_flags(&mut self, flags: Flags) {
		self.flags = flags;
	}

	fn id(&self) -> u32 {
		self.id
	}

	fn set_id(&mut self, id: u32) {
		self.id = id;
	}
}

#[derive(Debug)]
pub struct Message<A, B> {
	header: Header<A>,
	bytes: B
}

impl<A, B> Message<A, B>
where
	A: Action,
	B: PacketBytes
{
	pub fn new() -> Self {
		Self::empty()
	}

	#[cfg(feature = "json")]
	pub fn serialize<S: ?Sized>(value: &S) -> crate::Result<Self>
	where S: serde::Serialize {
		let mut me = Self::new();
		me.bytes.body_mut().serialize(value)?;
		Ok(me)
	}

	#[cfg(feature = "json")]
	pub fn deserialize<D>(&self) -> crate::Result<D>
	where D: serde::de::DeserializeOwned {
		self.bytes.body().deserialize()
			.map_err(Into::into)
	}

	pub fn body(&self) -> BodyBytes<'_> {
		self.bytes.body()
	}

	pub fn body_mut(&mut self) -> BodyBytesMut<'_> {
		self.bytes.body_mut()
	}
}

impl<A, B> Message<A, B> {
	pub fn action(&self) -> &A {
		&self.header.action
	}
}

impl<A, B> Packet<B> for Message<A, B>
where
	A: Action,
	B: PacketBytes
{
	type Header = Header<A>;

	fn header(&self) -> &Self::Header {
		&self.header
	}

	fn header_mut(&mut self) -> &mut Self::Header {
		&mut self.header
	}

	fn empty() -> Self {
		Self {
			header: Self::Header::empty(),
			bytes: B::new(Self::Header::len())
		}
	}

	fn from_bytes_and_header(bytes: B, header: Self::Header) -> packet::Result<Self> {
		Ok(Self { header, bytes })
	}

	fn into_bytes(mut self) -> B {
		let body_len = self.bytes.body().len();
		self.header.body_len = body_len as u32;
		self.header.to_bytes(self.bytes.header_mut());
		self.bytes
	}
}