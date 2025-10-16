use super::packet::*;
use super::{PacketBytes, PlainBytes, Result};

use std::marker::PhantomData;

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPacket<B> {
	header: TestPacketHeader,
	pub num1: u32,
	pub num2: u64,
	marker: PhantomData<B>,
}

impl<B> TestPacket<B> {
	pub fn new(num1: u32, num2: u64) -> Self {
		Self {
			header: TestPacketHeader::empty(),
			num1,
			num2,
			marker: PhantomData,
		}
	}
}

impl<B> Packet<B> for TestPacket<B>
where
	B: PacketBytes,
{
	// gets created if header and body are ready
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
			marker: PhantomData,
		}
	}

	fn from_bytes_and_header(bytes: B, header: Self::Header) -> Result<Self> {
		let mut body = bytes.body();
		assert_eq!(header.body_len(), body.len() as u32);

		if header.body_len() == 0 {
			let mut me = Self::empty();
			me.header = header;
			Ok(me)
		} else {
			Ok(Self {
				header,
				num1: body.read_u32(),
				num2: body.read_u64(),
				marker: PhantomData,
			})
		}
	}

	fn into_bytes(mut self) -> B {
		let mut bytes = B::new(Self::Header::LEN as usize);
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
	id: u32,
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
			id: 0,
		}
	}
}

impl PacketHeader for TestPacketHeader {
	const LEN: u32 = 4 + 4 + 1;

	fn from_bytes(mut bytes: Bytes) -> Result<Self> {
		assert_eq!(bytes.len(), Self::LEN as usize);
		Ok(Self {
			length: bytes.read_u32(),
			flags: Flags::from_u8(bytes.read_u8())?,
			id: bytes.read_u32(),
		})
	}

	fn body_len(&self) -> u32 {
		self.length
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
	let n_msg =
		TestPacket::from_bytes_and_header(bytes.clone(), msg_header).unwrap();

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
