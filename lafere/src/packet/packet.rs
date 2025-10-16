use super::{PacketBytes, Result};
use bytes::Bytes;

pub trait Packet<B>: Sized
where
	B: PacketBytes,
{
	// gets created if header and body are ready
	type Header: PacketHeader;

	fn header(&self) -> &Self::Header;

	fn header_mut(&mut self) -> &mut Self::Header;

	/// Returns an empty header
	/// this is used internally by this crate.
	///
	/// ## Important
	/// `body_len` needs to return 0
	fn empty() -> Self;

	/// The `bytes.body()` length should always be the same value has
	/// `header.body_len()`
	fn from_bytes_and_header(bytes: B, header: Self::Header) -> Result<Self>;

	fn into_bytes(self) -> B;
}

// Header should be as small as possible
pub trait PacketHeader: Clone + Sized {
	/// how long is the header written to bytes
	const LEN: u32;

	/// bytes.len() == Self::LEN
	///
	/// ## Implementor
	/// If an Error get's returned this means that the stream from where the
	/// bytes are read will be terminated.
	fn from_bytes(bytes: Bytes) -> Result<Self>;

	/// Returns the length of the body
	fn body_len(&self) -> u32;

	/// Returns the internal flags.
	///
	/// ## Note
	/// This is returned as a number so that outside of this crate
	/// nobody can rely on the information contained within.
	fn flags(&self) -> &Flags;

	fn set_flags(&mut self, flags: Flags);

	fn id(&self) -> u32;

	// /// Sets if this message is push.
	fn set_id(&mut self, id: u32);
}

// we should have a ping && pong
// a close
// and a

// if IS_PUSH && IS_REQ (we have an internal message)

macro_rules! kind {
	($($variant:ident = $val:expr),*) => {
		#[derive(Debug, Clone, Copy, PartialEq, Eq)]
		pub(crate) enum Kind {
			$($variant),*,
			Unknown
		}

		impl Kind {
			fn from_num(num: u8) -> Self {
				debug_assert!(num <= MAX_KIND);
				match num {
					$($val => Self::$variant),*,
					_ => Self::Unknown
				}
			}

			fn as_num(&self) -> u8 {
				match self {
					$(Self::$variant => $val),*,
					Self::Unknown => MAX_KIND
				}
			}

			#[cfg_attr(not(feature = "connection"), allow(dead_code))]
			pub(crate) fn to_str(&self) -> &'static str {
				match self {
					$(Self::$variant => stringify!($variant)),*,
					Self::Unknown => "Unknown"
				}
			}
		}
	}
}

// max is 2^4 = 16
kind! {
	Request = 1,
	Response = 2,
	NoResponse = 3,
	Stream = 4,
	RequestReceiver = 5,
	RequestSender = 6,
	StreamClosed = 7,
	Close = 8,
	Ping = 9,
	MalformedRequest = 10,
	EnableServerRequests = 11
}

// equivalent to expect response
const KIND_OFFSET: u8 = 4;
const MAX_KIND: u8 = u8::MAX >> KIND_OFFSET;
const KIND_MASK: u8 = 0b1111 << KIND_OFFSET;
// const IS_STREAM: u8 = 1 << 3;
// const IS_LAST: u8 = 1 << 2;

///  Flags
/// ```dont_run
/// +------+----------+
/// | Kind | Reserved |
/// +------+----------+
/// |  4   |     4    |
/// +------+----------+
///  In bits
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
	inner: u8,
}

impl Flags {
	#[cfg_attr(not(feature = "connection"), allow(dead_code))]
	pub(crate) fn new(kind: Kind) -> Self {
		let mut this = Self { inner: 0 };
		this.set_kind(kind);
		this
	}

	pub fn empty() -> Self {
		Self { inner: 0 }
	}

	pub fn from_u8(num: u8) -> Result<Self> {
		Ok(Self { inner: num })
	}

	pub(crate) fn kind(&self) -> Kind {
		let num = self.inner >> KIND_OFFSET;
		Kind::from_num(num)
	}

	pub(crate) fn set_kind(&mut self, kind: Kind) {
		let num = kind.as_num();
		debug_assert!(num < MAX_KIND);
		// clear mask
		self.inner &= !KIND_MASK;
		self.inner |= num << KIND_OFFSET;
	}

	pub fn as_u8(&self) -> u8 {
		self.inner
	}

	pub fn is_request(&self) -> bool {
		matches!(self.kind(), Kind::Request)
	}

	pub fn is_response(&self) -> bool {
		matches!(self.kind(), Kind::Response)
	}
}
