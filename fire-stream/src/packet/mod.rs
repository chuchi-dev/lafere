
mod bytes;
pub use self::bytes::*;

mod body_bytes;
pub use body_bytes::*;

mod packet;
pub use packet::*;

#[cfg(feature = "encrypted")]
mod encrypted_bytes;
#[cfg(feature = "encrypted")]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
pub use encrypted_bytes::*;

pub(crate) mod builder;

pub type Result<T> = std::result::Result<T, PacketError>;

use std::fmt;


#[derive(Debug)]
pub enum PacketError {
	Header(String),
	Body(String),
	#[cfg(feature = "json")]
	#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
	Json(serde_json::Error),
	#[cfg(feature = "fs")]
	#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
	Io(std::io::Error),
	/// Returns the size that should have been sent
	BodyLimitReached(usize)
}

impl fmt::Display for PacketError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Header(s) => write!(f, "PacketError::Header: {}", s),
			Self::Body(s) => write!(f, "PacketError::Body: {}", s),
			#[cfg(feature = "json")]
			Self::Json(s) => write!(f, "PacketError::Json: {}", s),
			#[cfg(feature = "fs")]
			Self::Io(s) => write!(f, "PacketError::Io: {}", s),
			Self::BodyLimitReached(s) => {
				write!(f, "PacketError::BodyLimitReached: {}", s)
			}
		}
	}
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for PacketError {
	fn from(e: serde_json::Error) -> Self {
		Self::Json(e)
	}
}

#[cfg(feature = "fs")]
impl From<std::io::Error> for PacketError {
	fn from(e: std::io::Error) -> Self {
		Self::Io(e)
	}
}