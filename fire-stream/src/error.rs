
use crate::packet::PacketError;

use std::{io, fmt};
use std::error::Error;

#[cfg(feature = "encrypted")]
use crypto::cipher::MacNotEqual;

use tokio::task;

/// ConnectionClosed, RequestDropped, StreamClosed are all errors
/// which you can't say about what really happended those should be treated
/// as the same for the moment.
/// 
/// ## Todo
/// Fix this!!
#[derive(Debug)]
#[non_exhaustive]
pub enum StreamError {
	Io(io::Error),
	Packet(PacketError),
	#[cfg(feature = "encrypted")]
	MacNotEqual(MacNotEqual),
	/// This get's returned if we don't have any other error to return
	ConnectionClosed,
	/// Gets returned if the request or response channel is dropped unexpectedly
	RequestDropped,
	/// Get's returned if a stream is closed
	StreamClosed,
	JoinError(task::JoinError)
}

impl StreamError {

	pub fn io_other<E>(e: E) -> Self
	where E: Into<Box<dyn Error + Send + Sync>> {
		Self::Io(io::Error::new(io::ErrorKind::Other, e))
	}

}

impl From<io::Error> for StreamError {
	fn from(e: io::Error) -> Self {
		Self::Io(e)
	}
}

impl From<PacketError> for StreamError {
	fn from(e: PacketError) -> Self {
		Self::Packet(e)
	}
}

#[cfg(feature = "encrypted")]
impl From<MacNotEqual> for StreamError {
	fn from(e: MacNotEqual) -> Self {
		Self::MacNotEqual(e)
	}
}

impl fmt::Display for StreamError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl Error for StreamError {}