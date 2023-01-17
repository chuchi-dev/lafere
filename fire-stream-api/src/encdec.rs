#[cfg(feature = "json")]
pub mod json {
	use crate::error::MessageError;
	use crate::message::{Message, Action, PacketBytes};
	use bytes::BytesRead;

	use serde::{Serialize, de::DeserializeOwned};


	pub fn encode<T, A, B>(value: T) -> Result<Message<A, B>, MessageError>
	where
		T: Serialize,
		A: Action,
		B: PacketBytes
	{
		let mut msg = Message::new();
		serde_json::to_writer(msg.body_mut(), &value)
			.map_err(MessageError::Json)?;

		Ok(msg)
	}

	pub fn decode<A, B, T>(msg: Message<A, B>) -> Result<T, MessageError>
	where
		A: Action,
		B: PacketBytes,
		T: DeserializeOwned
	{
		serde_json::from_slice(msg.body().as_slice())
			.map_err(MessageError::Json)
	}
}

#[cfg(feature = "protobuf")]
pub mod protobuf {
	use crate::error::MessageError;
	use crate::message::{Message, Action, PacketBytes};
	use bytes::BytesRead;

	use fire_protobuf::encode::{EncodeMessage, MessageEncoder};
	use fire_protobuf::decode::DecodeMessage;


	pub fn encode<T, A, B>(mut value: T) -> Result<Message<A, B>, MessageError>
	where
		T: EncodeMessage,
		A: Action,
		B: PacketBytes
	{
		let mut msg = Message::new();
		let mut encoder = MessageEncoder::new(msg.body_mut());
		value.encode(None, &mut encoder)
			.map_err(MessageError::EncodeError)?;

		Ok(msg)
	}

	pub fn decode<A, B, T>(msg: Message<A, B>) -> Result<T, MessageError>
	where
		A: Action,
		B: PacketBytes,
		T: for<'a> DecodeMessage<'a>
	{
		T::parse_from_bytes(msg.body().as_slice())
			.map_err(MessageError::DecodeError)
	}
}