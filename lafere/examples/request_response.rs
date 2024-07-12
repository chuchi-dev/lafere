use std::time::Duration;

use tokio::io;

use lafere::packet::{
	BodyBytes, BodyBytesMut, Flags, Packet, PacketBytes, PacketError,
	PacketHeader,
};
use lafere::server::Message;
use lafere::{client, server};

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite};

#[derive(Debug, Clone)]
struct MyPacket<B> {
	header: MyHeader,
	bytes: B,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct MyHeader {
	length: u32,
	flags: Flags,
	id: u32,
}

impl<B> MyPacket<B>
where
	B: PacketBytes,
{
	pub fn empty() -> Self {
		Self {
			header: MyHeader::default(),
			bytes: B::new(MyHeader::LEN as usize),
		}
	}

	pub fn body(&self) -> BodyBytes<'_> {
		self.bytes.body()
	}

	pub fn body_mut(&mut self) -> BodyBytesMut<'_> {
		self.bytes.body_mut()
	}
}

impl<B> Packet<B> for MyPacket<B>
where
	B: PacketBytes,
{
	type Header = MyHeader;

	fn header(&self) -> &Self::Header {
		&self.header
	}

	fn header_mut(&mut self) -> &mut Self::Header {
		&mut self.header
	}

	fn empty() -> Self {
		Self {
			header: MyHeader::default(),
			bytes: B::new(MyHeader::LEN as usize),
		}
	}

	fn from_bytes_and_header(
		bytes: B,
		header: Self::Header,
	) -> Result<Self, PacketError> {
		Ok(Self { header, bytes })
	}

	fn into_bytes(mut self) -> B {
		self.header.length = self.bytes.body().len() as u32;
		self.header.write_to(self.bytes.header_mut());
		self.bytes
	}
}

impl MyHeader {
	fn write_to(&self, mut bytes: BytesMut) {
		bytes.write_u32(self.length);
		bytes.write_u8(self.flags.as_u8());
		bytes.write_u32(self.id);
	}
}

impl PacketHeader for MyHeader {
	const LEN: u32 = 4 + 4 + 1;

	fn from_bytes(mut bytes: Bytes) -> Result<Self, PacketError> {
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

#[tokio::main]
async fn main() {
	let (client, server) = io::duplex(1024);

	let client = client::Connection::<MyPacket<_>>::new(
		client,
		client::Config {
			timeout: Duration::from_secs(1),
			body_limit: 1024,
		},
		None,
	);

	let mut server = server::Connection::<MyPacket<_>>::new(
		server,
		server::Config {
			timeout: Duration::from_secs(1),
			body_limit: 1024,
		},
	);

	let client_task = tokio::spawn(async move {
		let mut req = MyPacket::empty();
		req.body_mut().write(b"Hello, World!");

		let resp = client.request(req).await.unwrap();
		assert_eq!(resp.body().as_slice(), b"Hello, Back!");
	});

	let server_task = tokio::spawn(async move {
		let msg = server.receive().await.unwrap();
		match msg {
			Message::Request(req, resp_sender) => {
				assert_eq!(req.body().as_slice(), b"Hello, World!");

				let mut resp = MyPacket::empty();
				resp.body_mut().write(b"Hello, Back!");
				resp_sender.send(resp).unwrap();
			}
			_ => unreachable!(),
		}

		assert!(server.receive().await.is_none());
	});

	tokio::try_join!(client_task, server_task).unwrap();
}

#[test]
fn run_main() {
	main();
}
