use super::{Packet, PacketHeader, PacketBytes, PacketError};

use std::{mem, io};

use bytes::{BytesRead, BytesWrite, BytesSeek};

use tokio::io::AsyncReadExt;


/// make frame stream abort safe.
pub struct PacketReceiver<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	/// bytes is always >= P::Header::LEN
	bytes: B,
	header: Option<P::Header>,
	read: usize,
	/// if size is zero it's not limited
	body_limit: u32
}

impl<P, B> PacketReceiver<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	pub fn new(body_limit: u32) -> Self {
		Self {
			bytes: B::new(P::Header::LEN as usize),
			header: None,
			read: 0,
			body_limit
		}
	}

	pub fn set_body_limit(&mut self, body_limit: u32) {
		self.body_limit = body_limit;
	}

	/// May be called multiple times
	/// 
	/// ## Error
	/// if an error occurs, you should probably
	/// not call this function again.
	pub async fn read_header<R, F>(
		&mut self,
		reader: &mut R,
		mutate: F
	) -> Result<(), PacketReceiverError<P::Header>>
	where
		R: AsyncReadExt + Unpin,
		F: FnOnce(&mut B) -> Result<(), PacketError>
	{
		if self.header.is_some() {
			return Ok(())
		}

		// lets read all header bytes
		let mut header_bytes = self.bytes.full_header_mut();
		header_bytes.seek(self.read);

		loop {
			let r = reader.read(header_bytes.remaining_mut()).await
				.map_err(Error::Io)?;
			header_bytes.advance(r);
			self.read += r;

			if header_bytes.remaining().is_empty() {
				break
			}

			if r == 0 {
				return Err(PacketReceiverError::Io(eof()))
			}
		}

		// now mutate the header if needed
		mutate(&mut self.bytes)
			.map_err(Error::Hard)?;

		// convert the header bytes to a header
		let header = P::Header::from_bytes(self.bytes.header())
			.map_err(Error::Hard)?;
		self.header = Some(header);
		self.read = 0;

		Ok(())
	}

	// abort safe
	/// Reads the body and when everything is read
	/// returns the finished message.
	/// 
	/// ## Note
	/// mutate only gets called if there is a body.
	pub async fn read_body<R, F>(
		&mut self,
		reader: &mut R,
		mutate: F
	) -> Result<P, PacketReceiverError<P::Header>>
	where
		R: AsyncReadExt + Unpin,
		F: FnOnce(&mut B) -> Result<(), PacketError>
	{
		let len = self.header.as_ref()
			.expect("read the header first")
			.body_len();

		if len == 0 {
			return self.take_message()
		}

		// if we never read, prepare the body size
		if self.read == 0 {
			if self.body_limit != 0 && len > self.body_limit {
				return Err(Error::Hard(PacketError::BodyLimitReached(len)))
			}

			let mut body_bytes = self.bytes.body_mut();
			body_bytes.resize(len as usize);
			debug_assert_eq!(body_bytes.as_mut().len(), len as usize);
		}

		// lets read all header bytes
		let mut body_bytes = self.bytes.full_body_mut();
		body_bytes.seek(self.read);

		assert!(body_bytes.len() > 0, "a body should never be empty");

		loop {
			let r = reader.read(body_bytes.remaining_mut()).await
				.map_err(PacketReceiverError::Io)?;
			body_bytes.advance(r);
			self.read += r;

			if body_bytes.remaining().is_empty() {
				break
			}

			if r == 0 {
				return Err(Error::Io(eof()))
			}
		}

		// now mutate the body if needed
		mutate(&mut self.bytes)
			.map_err(|e| self.soft_error(e))?;

		self.take_message()
	}

	fn take_message(&mut self) -> Result<P, PacketReceiverError<P::Header>> {
		let bytes = mem::replace(
			&mut self.bytes,
			B::new(P::Header::LEN as usize)
		);

		let header = self.header.take().unwrap();
		self.read = 0;

		// this is soft since we know the 
		P::from_bytes_and_header(bytes, header.clone())
			.map_err(|e| Error::Soft(header, e))
	}

	/// ## Panics
	/// If there is no header
	fn soft_error(&mut self, e: PacketError) -> PacketReceiverError<P::Header> {
		// reset all fields
		self.bytes = B::new(P::Header::LEN as usize);
		let header = self.header.take().unwrap();
		self.read = 0;

		Error::Soft(header, e)
	}
}

pub enum PacketReceiverError<H> {
	Io(io::Error),
	/// If this error is returned the packet state is broken and the connection
	/// needs to be opened again
	Hard(PacketError),
	/// If this error is returned this means the packet is malformed but
	/// the header is intact and the connection can be kept open
	Soft(H, PacketError)
}

type Error<H> = PacketReceiverError<H>;

fn eof() -> io::Error {
	io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}