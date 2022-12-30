
use super::{Packet, PacketHeader, PacketBytes, PacketError};
use crate::Result;

use std::{mem, io};

use bytes::{BytesRead, BytesWrite, BytesSeek};

use tokio::io::AsyncReadExt;


/// make frame stream abort safe.
pub struct PacketReceiver<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	bytes: B,
	header: Option<P::Header>,
	read: usize,
	/// if size is zero it's not limited
	body_limit: usize
}

impl<P, B> PacketReceiver<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{

	pub fn new(body_limit: usize) -> Self {
		Self {
			bytes: B::new(P::Header::len()),
			header: None,
			read: 0,
			body_limit
		}
	}

	pub fn set_body_limit(&mut self, body_limit: usize) {
		self.body_limit = body_limit;
	}

	/// May becalled multiple times
	/// 
	/// ## Error
	/// if an error occurs, you should probably
	/// not call this function again.
	pub async fn read_header<R, F>(
		&mut self,
		reader: &mut R,
		mutate: F
	) -> Result<()>
	where
		R: AsyncReadExt + Unpin,
		F: FnOnce(&mut B) -> Result<()>
	{
		if self.header.is_some() {
			return Ok(())
		}

		// lets read all header bytes
		let mut header_bytes = self.bytes.full_header_mut();
		header_bytes.seek(self.read);

		loop {
			let r = reader.read(header_bytes.remaining_mut()).await?;
			header_bytes.advance(r);
			self.read += r;

			if header_bytes.remaining().is_empty() {
				break
			}

			if r == 0 {
				return Err(eof().into())
			}
		}

		// now mutate the header if needed
		mutate(&mut self.bytes)?;

		// convert the header bytes to a header
		let header = P::Header::from_bytes(self.bytes.header())?;
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
	) -> Result<P>
	where
		R: AsyncReadExt + Unpin,
		F: FnOnce(&mut B) -> Result<()>
	{
		let len = self.header.as_ref()
			.expect("read the header first")
			.body_len();

		if len == 0 {
			return self.take_message()
		}

		// if we never read prepare the body size
		if self.read == 0 {

			if self.body_limit != 0 && len > self.body_limit {
				return Err(PacketError::BodyLimitReached(len).into())
			}

			let mut body_bytes = self.bytes.body_mut();
			body_bytes.resize(len);
			debug_assert_eq!(body_bytes.as_mut().len(), len);
		}

		// lets read all header bytes
		let mut body_bytes = self.bytes.full_body_mut();
		body_bytes.seek(self.read);

		assert!(body_bytes.len() > 0, "already called read_body");

		loop {
			let r = reader.read(body_bytes.remaining_mut()).await?;
			body_bytes.advance(r);
			self.read += r;

			if body_bytes.remaining().is_empty() {
				break
			}

			if r == 0 {
				return Err(eof().into())
			}
		}

		// now mutate the body if needed
		mutate(&mut self.bytes)?;

		self.take_message()
	}

	fn take_message(&mut self) -> Result<P> {

		let bytes = mem::replace(
			&mut self.bytes,
			B::new(P::Header::len())
		);

		let header = self.header.take().unwrap();

		self.read = 0;

		P::from_bytes_and_header(bytes, header)
			.map_err(|e| e.into())
	}

}


fn eof() -> io::Error {
	io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}