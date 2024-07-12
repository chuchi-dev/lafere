use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

pub use lafere::error::RequestError;

/// The error that is sent if something goes wrong while responding
/// to a request.
pub trait ApiError: StdError {
	fn from_request_error(e: RequestError) -> Self;

	fn from_message_error(e: MessageError) -> Self;
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
	MessageError(MessageError),
}

impl From<MessageError> for Error {
	fn from(e: MessageError) -> Self {
		Self::MessageError(e)
	}
}

impl fmt::Display for Error {
	fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, fmt)
	}
}

impl StdError for Error {}

#[derive(Debug)]
#[non_exhaustive]
pub enum MessageError {
	#[cfg(feature = "json")]
	Json(serde_json::Error),
	// Protobuf
	#[cfg(feature = "protobuf")]
	EncodeError(protopuffer::encode::EncodeError),
	#[cfg(feature = "protobuf")]
	DecodeError(protopuffer::decode::DecodeError),
	Other(Cow<'static, str>),
}

impl fmt::Display for MessageError {
	fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, fmt)
	}
}

impl StdError for MessageError {}
