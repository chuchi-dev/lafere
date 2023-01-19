use crate::packet::PacketError;

use std::{io, fmt};
use std::error::Error;


#[derive(Debug)]
#[non_exhaustive]
pub enum TaskError {
	#[cfg(feature = "connection")]
	Join(tokio::task::JoinError),
	Io(io::Error),
	/// Reading a packet failed
	Packet(PacketError),
	/// gets returned if a packet with an id was received which is unknown
	/// This means a hard error and the connection should be closed
	UnknownId(u32),
	ExistingId(u32),
	WrongPacketKind(&'static str),
	/// Incorrect signature
	#[cfg(feature = "encrypted")]
	#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
	IncorrectSignature
}

impl fmt::Display for TaskError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl Error for TaskError {}


#[derive(Debug)]
#[non_exhaustive]
pub enum RequestError {
	/// If the connection was already closed when you called request
	/// 
	/// Depending on ReconnectStrat you might wan't to call request again
	ConnectionAlreadyClosed,
	/// Only get's returned if something wen't wrong and we won't be able
	/// to get an better error, probably means the connection closed
	TaskFailed,
	/// The other side responded with no response. This means the other side
	/// didn't bother to send a response.
	NoResponse,
	/// If the request you sent could not be parsed successfully by the server
	MalformedRequest,
	/// The error that originated while parsing the response
	ResponsePacket(PacketError)
}

impl fmt::Display for RequestError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl Error for RequestError {}


#[derive(Debug)]
#[non_exhaustive]
pub enum ResponseError {
	ConnectionClosed
}

impl fmt::Display for ResponseError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl Error for ResponseError {}


/// Returned from a StreamSender or StreamReceiver
#[derive(Debug)]
#[non_exhaustive]
pub enum StreamError {
	StreamAlreadyClosed
}

impl fmt::Display for StreamError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl Error for StreamError {}