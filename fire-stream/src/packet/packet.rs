
use super::{PacketBytes, Result};
use bytes::Bytes;

// TODO: remove std::fmt::Debug bound

pub trait Packet<B>: std::fmt::Debug + Sized
where B: PacketBytes {// gets created if header and body are ready
	type Header: PacketHeader;

	fn header(&self) -> &Self::Header;

	fn header_mut(&mut self) -> &mut Self::Header;

	/// Returns an empty header
	/// this is used internally by this crate.
	///
	/// ## Important
	/// `body_len` needs to return 0
	fn empty() -> Self;

	fn from_bytes_and_header(bytes: B, header: Self::Header) -> Result<Self>;

	fn into_bytes(self) -> B;

}

// Header should be as small as possible
pub trait PacketHeader: std::fmt::Debug + Sized {
	fn len() -> usize;

	/// bytes.len() == Self::len()
	fn from_bytes(bytes: Bytes) -> Result<Self>;

	/// Returns the length of the body
	fn body_len(&self) -> usize;

	fn flags(&self) -> &Flags;

	fn set_flags(&mut self, flags: Flags);

	/// Returns the internal flags.
	/// 
	/// ## Note
	/// This is returned as a number so that outside of this crate
	/// nobody can rely on the information contained within.
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
		}
	}
}

// max is 2^4 = 16
kind!{
	Request = 1,
	Response = 2,
	NoResponse = 3,
	Stream = 4,
	NewStream = 5,
	NewSenderStream = 6,
	StreamClosed = 7,
	Close = 8,
	Ping = 9
}
// todo add ping

// equivalent to expect response
const KIND_OFFSET: u8 = 4;
const MAX_KIND: u8 = u8::MAX << KIND_OFFSET;
const KIND_MASK: u8 = 0b11_111 << KIND_OFFSET;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
	inner: u8
}

impl Flags {

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


// /// How a message kind is represented is not important
// /// the only thing that is important that the sender
// /// and receiver represent it the same.
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum MessageKind {
// 	Request(u32),
// 	Response(u32),
// 	Push
// }

// impl MessageKind {
// 	pub fn as_u8(&self) -> u8 {
// 		match self {
// 			Self::Request => 0,
// 			Self::Response => 1,
// 			Self::Push => 2
// 		}
// 	}

// 	pub fn from_u8(num: u8) -> Self {
// 		match num {
// 			0 => Self::Request,
// 			1 => Self::Response,
// 			_ => Self::Push
// 		}
// 	}

// 	pub fn is_response(&self) -> bool {
// 		matches!(self, Self::Response)
// 	}
// }


#[cfg(test)]
pub(crate) mod test {

	use super::*;
	use crate::packet::PlainBytes;

	use std::marker::PhantomData;

	use bytes::{BytesMut, BytesRead, BytesWrite};


	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct TestPacket<B> {
		header: TestPacketHeader,
		pub num1: u32,
		pub num2: u64,
		marker: PhantomData<B>
	}

	impl<B> TestPacket<B> {
		pub fn new(num1: u32, num2: u64) -> Self {
			Self {
				header: TestPacketHeader::empty(),
				num1, num2,
				marker: PhantomData
			}
		}
	}


	impl<B> Packet<B> for TestPacket<B>
	where B: PacketBytes {// gets created if header and body are ready
		type Header = TestPacketHeader;

		fn header(&self) -> &Self::Header {
			&self.header
		}

		fn header_mut(&mut self) -> &mut Self::Header {
			&mut self.header
		}

		fn empty() -> Self {
			Self {
				header: TestPacketHeader::empty(),
				num1: 0,
				num2: 0,
				marker: PhantomData
			}
		}

		fn from_bytes_and_header(bytes: B, header: Self::Header) -> Result<Self> {
			let mut body = bytes.body();
			assert_eq!(header.body_len(), body.len());

			if header.body_len() == 0 {
				let mut me = Self::empty();
				me.header = header;
				Ok(me)
			} else {
				Ok(Self {
					header,
					num1: body.read_u32(),
					num2: body.read_u64(),
					marker: PhantomData
				})
			}
		}

		fn into_bytes(mut self) -> B {
			let mut bytes = B::new(Self::Header::len());
			let mut body = bytes.body_mut();
			body.write_u32(self.num1);
			body.write_u64(self.num2);
			self.header.length = body.len() as u32;
			self.header.to_bytes(bytes.header_mut());
			bytes
		}


	}


	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct TestPacketHeader {
		length: u32,
		flags: Flags,
		id: u32
	}

	impl TestPacketHeader {
		fn to_bytes(&self, mut bytes: BytesMut) {
			bytes.write_u32(self.length);
			bytes.write_u8(self.flags.as_u8());
			bytes.write_u32(self.id);
		}

		fn empty() -> Self {
			Self {
				length: 0,
				flags: Flags::empty(),
				id: 0
			}
		}
	}

	impl PacketHeader for TestPacketHeader {

		fn len() -> usize { 4 + 4 + 1 }

		fn from_bytes(mut bytes: Bytes) -> Result<Self> {
			assert_eq!(bytes.len(), Self::len());
			Ok(Self {
				length: bytes.read_u32(),
				flags: Flags::from_u8(bytes.read_u8())?,
				id: bytes.read_u32()
			})
		}

		fn body_len(&self) -> usize {
			self.length as usize
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




	#[test]
	fn test_message() {

		let mut msg = TestPacket::new(10, 20);
		// doing this manually since body_len() is valid after into_bytes()
		msg.header.length = 4 + 8;
		let bytes: PlainBytes = msg.clone().into_bytes();

		// header
		let header_bytes = bytes.header();
		let msg_header = TestPacketHeader::from_bytes(header_bytes).unwrap();
		let n_msg = TestPacket::from_bytes_and_header(bytes.clone(), msg_header).unwrap();

		assert_eq!(msg, n_msg);
		assert_eq!(bytes, n_msg.into_bytes());

	}

	#[test]
	fn test_flags() {

		let mut flags = Flags::new(Kind::Request);
		assert_eq!(flags.kind(), Kind::Request);
		flags.set_kind(Kind::Close);
		assert_eq!(flags.kind(), Kind::Close);

		// assert!(!flags.is_stream());
		// assert!(!flags.is_last());

		// flags.set_stream();
		// assert!(flags.is_stream());

		// flags.set_last();
		// assert!(flags.is_last());

		// let mut flags = Flags::new();
		// assert!(flags.set_id(MAX_MSG_ID));
		// assert_eq!(flags.id(), MAX_MSG_ID);

		// assert!(!flags.set_id(MAX_MSG_ID + 1));
	}

}