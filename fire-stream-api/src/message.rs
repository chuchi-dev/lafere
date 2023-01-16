use crate::error::MessageError;

use std::fmt::Debug;
use std::hash::Hash;

use stream::packet::{
	self, Flags, Packet,
	PacketHeader, BodyBytes, BodyBytesMut, PacketError
};
pub use stream::packet::PacketBytes;

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite};


/// ## Important
/// the number *zero* is not allowed to be used
pub trait Action: Debug + Copy + Eq + Hash {
	/// should try to convert an u16 to the action
	/// 
	/// 0 will never be passed as num
	fn from_u16(num: u16) -> Option<Self>;

	fn as_u16(&self) -> u16;

	fn max_body_size(_header: &Header<Self>) -> Option<u32> {
		None
	}
}

pub trait IntoMessage<A, B> {
	fn into_message(self) -> Result<Message<A, B>, MessageError>;
}

pub trait FromMessage<A, B>: Sized {
	fn from_message(msg: Message<A, B>) -> Result<Self, MessageError>;
}


#[derive(Debug, Clone, PartialEq, Eq)]
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
}

impl<A, B> Message<A, B>
where B: PacketBytes {
	pub fn set_success(&mut self, success: bool) {
		self.header.msg_flags.set_success(success);
	}

	pub fn is_success(&self) -> bool {
		self.header.msg_flags.is_success()
	}

	pub fn body(&self) -> BodyBytes<'_> {
		self.bytes.body()
	}

	pub fn body_mut(&mut self) -> BodyBytesMut<'_> {
		self.bytes.body_mut()
	}
}

impl<A, B> Message<A, B> {
	pub fn action(&self) -> Option<&A> {
		match &self.header.action {
			MaybeAction::Action(a) => Some(a),
			_ => None
		}
	}
}

impl<A, B> IntoMessage<A, B> for Message<A, B> {
	fn into_message(self) -> Result<Self, MessageError> {
		Ok(self)
	}
}

impl<A, B> FromMessage<A, B> for Message<A, B> {
	fn from_message(me: Self) -> Result<Self, MessageError> {
		Ok(me)
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
			bytes: B::new(Self::Header::LEN as usize)
		}
	}

	fn from_bytes_and_header(
		bytes: B,
		header: Self::Header
	) -> packet::Result<Self> {
		// todo probably check if the action is correct
		Ok(Self { header, bytes })
	}

	fn into_bytes(mut self) -> B {
		let body_len = self.bytes.body().len();
		self.header.body_len = body_len as u32;
		self.header.to_bytes(self.bytes.header_mut());
		self.bytes
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header<A> {
	body_len: u32,
	flags: Flags,
	msg_flags: MessageFlags,
	id: u32,
	action: MaybeAction<A>
}

impl<A> Header<A>
where A: Action {
	pub fn empty() -> Self {
		Self {
			body_len: 0,
			flags: Flags::empty(),
			msg_flags: MessageFlags::new(true),
			id: 0,
			action: MaybeAction::None
		}
	}

	pub fn to_bytes(&self, mut bytes: BytesMut) {
		bytes.write_u32(self.body_len);
		bytes.write_u8(self.flags.as_u8());
		bytes.write_u8(self.msg_flags.as_u8());
		bytes.write_u32(self.id);
		bytes.write_u16(self.action.as_u16());
	}

	pub fn set_action(&mut self, action: A) {
		self.action = MaybeAction::Action(action);
	}
}

impl<A> PacketHeader for Header<A>
where A: Action {
	const LEN: u32 = 4 + 1 + 1 + 4 + 2;

	fn from_bytes(mut bytes: Bytes) -> packet::Result<Self> {
		let me = Self {
			body_len: bytes.read_u32(),
			flags: Flags::from_u8(bytes.read_u8())?,
			msg_flags: MessageFlags::from_u8(bytes.read_u8()),
			id: bytes.read_u32(),
			action: {
				let action_num = bytes.read_u16();
				if action_num == 0 {
					MaybeAction::None
				} else if let Some(action) = A::from_u16(action_num) {
					// we don't return an error here
					// since we wan't to get the package
					// and can still discard it later
					// this allows the connection to continue to exist
					// without closing it
					MaybeAction::Action(action)
				} else {
					MaybeAction::Unknown(action_num)
				}
			}
		};

		if let Some(max) = A::max_body_size(&me) {
			if me.body_len > max {
				return Err(PacketError::BodyLimitReached(max))
			}
		}

		Ok(me)
	}

	fn body_len(&self) -> u32 {
		self.body_len
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

/// In bits
/// +----------+---------+
/// | reserved | success |
/// +----------+---------+
/// |    7     |    1    |
/// +----------+---------+
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MessageFlags {
	inner: u8
}

impl MessageFlags {
	pub fn new(success: bool) -> Self {
		let mut me = Self { inner: 0 };
		me.set_success(success);

		me
	}

	pub fn from_u8(inner: u8) -> Self {
		Self { inner }
	}

	pub fn is_success(&self) -> bool {
		self.inner & 0b0000_0001 != 0
	}

	pub fn set_success(&mut self, success: bool) {
		self.inner |= success as u8;
	}

	pub fn as_u8(&self) -> u8 {
		self.inner
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MaybeAction<A> {
	Action(A),
	/// zero will never be here
	Unknown(u16),
	/// Represented as 0
	None
}

impl<A> MaybeAction<A>
where A: Action {
	pub fn as_u16(&self) -> u16 {
		match self {
			Self::Action(a) => a.as_u16(),
			Self::Unknown(u) => *u,
			Self::None => 0
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use stream::packet::PlainBytes;

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	enum SomeAction {
		One,
		Two
	}

	impl Action for SomeAction {
		fn from_u16(num: u16) -> Option<Self> {
			match num {
				1 => Some(Self::One),
				2 => Some(Self::Two),
				_ => None
			}
		}

		fn as_u16(&self) -> u16 {
			match self {
				Self::One => 1,
				Self::Two => 2
			}
		}
	}

	#[test]
	fn msg_from_to_bytes() {
		let mut msg = Message::<_, PlainBytes>::new();
		msg.header_mut().set_action(SomeAction::Two);
		msg.body_mut().write_u16(u16::MAX);

		let bytes = msg.clone().into_bytes();
		let header = Header::from_bytes(bytes.header()).unwrap();
		let n_msg = Message::from_bytes_and_header(bytes, header).unwrap();
		assert_eq!(msg.action(), n_msg.action());
		assert_eq!(msg.header().flags(), n_msg.header().flags());

		assert_eq!(n_msg.body().read_u16(), u16::MAX);
	}
}