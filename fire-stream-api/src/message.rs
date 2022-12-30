
use crate::error::ApiError;

use stream::packet::{
	self, Flags, Packet,
	PacketHeader, BodyBytes, BodyBytesMut, PacketError
};
pub use stream::packet::{PlainBytes, PacketBytes};
#[cfg(feature = "encrypted")]
pub use stream::packet::EncryptedBytes;

use std::fmt::Debug;
use std::hash::Hash;

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite};

/// the number zero is not allowed to be used
pub trait Action: Debug + Copy + Eq + Hash {
	/// returns an Action that will never be returned
	fn empty() -> Self;
	/// should try to convert an u16 to the action
	/// 
	/// 0 will never be passed as num
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
	success: bool,
	id: u32,
	action: A
}

impl<A> Header<A>
where A: Action {

	pub fn empty() -> Self {
		Self {
			body_len: 0,
			flags: Flags::empty(),
			success: true,
			id: 0,
			action: A::empty()
		}
	}

	pub fn to_bytes(&self, mut bytes: BytesMut) {
		bytes.write_u32(self.body_len);
		bytes.write_u8(self.flags.as_u8());
		bytes.write_u8(self.success as u8);
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
		4 + 1 + 1 + 4 + 2
	}

	fn from_bytes(mut bytes: Bytes) -> packet::Result<Self> {
		let me = Self {
			body_len: bytes.read_u32(),
			flags: Flags::from_u8(bytes.read_u8())?,
			success: bytes.read_u8() == 1,
			id: bytes.read_u32(),
			action: {
				let action_num = bytes.read_u16();
				// if the action is empty create an empty action
				// instead of rellying on the Action implementation
				if action_num == 0 {
					A::empty()
				} else {
					A::from_u16(action_num)
						.ok_or_else(|| PacketError::Header("Action unknown".into()))?
				}
			}
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
}

impl<A, B> Message<A, B>
where B: PacketBytes {

	// #[cfg(feature = "json")]
	// pub fn serialize<S: ?Sized>(value: &S) -> Result<Self, E>
	// where S: serde::Serialize {
	// 	let mut me = Self::new();
	// 	me.bytes.body_mut().serialize(value)?;
	// 	Ok(me)
	// }

	// #[cfg(feature = "json")]
	// pub fn deserialize<D>(&self) -> crate::Result<D>
	// where D: serde::de::DeserializeOwned {
	// 	self.bytes.body().deserialize()
	// 		.map_err(Into::into)
	// }

	pub fn set_success(&mut self, success: bool) {
		self.header.success = success;
	}

	pub fn is_success(&self) -> bool {
		self.header.success
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

	fn from_bytes_and_header(
		bytes: B,
		header: Self::Header
	) -> packet::Result<Self> {
		Ok(Self { header, bytes })
	}

	fn into_bytes(mut self) -> B {
		let body_len = self.bytes.body().len();
		self.header.body_len = body_len as u32;
		self.header.to_bytes(self.bytes.header_mut());
		self.bytes
	}
}

/// Means SerializeDeserialize Message
pub trait SerdeMessage<A, B, E: ApiError>: Sized {

	/// You should never return a Message which has a status of !success
	/// 
	/// ## Note
	/// The action and the success flag/states will automatically be set
	fn into_message(self) -> Result<Message<A, B>, E>
	where B: PacketBytes;

	/// you will never receive a Message with a status of !success
	/// 
	/// ## Note
	/// The action will always match
	fn from_message(msg: Message<A, B>) -> Result<Self, E>
	where B: PacketBytes;

}

/// Helps to implement SerdeMessage with the default serde traits.
/// 
/// ```
/// # use fire_stream_api as stream_api;
/// use stream_api::derive_serde_message;
/// 
/// use serde::{Serialize, Deserialize};
/// 
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct Request {
/// 	name: String
/// }
/// 
/// derive_serde_message!(Request);
/// ```
#[macro_export]
macro_rules! derive_serde_message {
	($type:ty) => (
		impl<A, B, E> $crate::message::SerdeMessage<A, B, E> for $type
		where
			A: $crate::message::Action,
			E: $crate::error::ApiError
		{

			fn into_message(
				self
			) -> std::result::Result<$crate::message::Message<A, B>, E>
			where B: $crate::message::PacketBytes {
				let mut msg = $crate::message::Message::new();
				msg.body_mut()
					.serialize(&self)
					.map_err(|e| E::request(
						format!("malformed request: {}", e)
					))?;
				Ok(msg)
			}

			fn from_message(
				msg: $crate::message::Message<A, B>
			) -> std::result::Result<Self, E>
			where B: $crate::message::PacketBytes {
				msg.body().deserialize()
					.map_err(|e| E::response(
						format!("malformed response: {}", e)
					))
			}

		}
	)
}

derive_serde_message!(());

#[cfg(test)]
mod tests {
	use crate::derive_serde_message;

	use serde::{Serialize, Deserialize};

	#[derive(Debug, Clone, Serialize, Deserialize)]
	struct Request {
		name: String
	}

	derive_serde_message!(Request);


}