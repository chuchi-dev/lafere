#[cfg(feature = "json")]
use super::Result;

use std::io;
use std::result::Result as StdResult;

#[cfg(feature = "fs")]
use std::path::Path;

use bytes::{
	Bytes, BytesRead, BytesReadRef, BytesSeek, BytesWrite, Cursor, Offset,
};

#[cfg(feature = "json")]
use serde::{Serialize, de::DeserializeOwned};

#[cfg(feature = "fs")]
use tokio::fs::{self, File};
#[cfg(feature = "fs")]
use tokio::io::AsyncReadExt;

/// Read more from a body.
pub struct BodyBytes<'a> {
	inner: Bytes<'a>,
}

impl<'a> BodyBytes<'a> {
	/// Creates a new body bytes.
	pub fn new(slice: &'a [u8]) -> Self {
		Self {
			inner: slice.into(),
		}
	}

	/// Returns the length of this body.
	pub fn len(&self) -> usize {
		self.inner.len()
	}

	/// returns the inner slice
	pub fn inner(&self) -> &'a [u8] {
		self.inner.inner()
	}

	#[cfg(feature = "json")]
	#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
	pub fn deserialize<D>(&self) -> Result<D>
	where
		D: DeserializeOwned,
	{
		serde_json::from_slice(self.inner.as_slice()).map_err(|e| e.into())
	}

	#[cfg(feature = "fs")]
	#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
	pub async fn to_file<P>(&mut self, path: P) -> Result<()>
	where
		P: AsRef<Path>,
	{
		fs::write(path, self.as_slice()).await.map_err(|e| e.into())
	}
}

impl BytesRead for BodyBytes<'_> {
	// returns the full slice
	#[inline]
	fn as_slice(&self) -> &[u8] {
		self.inner.as_slice()
	}

	#[inline]
	fn remaining(&self) -> &[u8] {
		self.inner.remaining()
	}

	#[inline]
	fn try_read(&mut self, len: usize) -> StdResult<&[u8], bytes::ReadError> {
		self.inner.try_read(len)
	}

	#[inline]
	fn peek(&self, len: usize) -> Option<&[u8]> {
		self.inner.peek(len)
	}
}

impl<'a> BytesReadRef<'a> for BodyBytes<'a> {
	#[inline]
	fn as_slice_ref(&self) -> &'a [u8] {
		self.inner.as_slice_ref()
	}

	#[inline]
	fn remaining_ref(&self) -> &'a [u8] {
		self.inner.remaining_ref()
	}

	#[inline]
	fn try_read_ref(
		&mut self,
		len: usize,
	) -> StdResult<&'a [u8], bytes::ReadError> {
		self.inner.try_read_ref(len)
	}

	#[inline]
	fn peek_ref(&self, len: usize) -> Option<&'a [u8]> {
		self.inner.peek_ref(len)
	}
}

impl BytesSeek for BodyBytes<'_> {
	fn position(&self) -> usize {
		self.inner.position()
	}

	fn try_seek(&mut self, pos: usize) -> StdResult<(), bytes::SeekError> {
		self.inner.try_seek(pos)
	}
}

/// Write easely more to a body.
pub struct BodyBytesMut<'a> {
	inner: Offset<Cursor<&'a mut Vec<u8>>>,
}

impl<'a> BodyBytesMut<'a> {
	/// Creates a new body bytes.
	///
	/// This should only be used if you implement your own MessageBytes.
	pub fn new(offset: usize, buffer: &'a mut Vec<u8>) -> Self {
		Self {
			inner: Offset::new(Cursor::new(buffer), offset),
		}
	}

	/// Shrinks and grows the internal bytes.
	pub fn resize(&mut self, len: usize) {
		let offset = self.inner.offset();
		unsafe {
			// safe because the offset is kept
			self.as_mut_vec().resize(offset + len, 0);
		}
	}

	pub fn reserve(&mut self, len: usize) {
		let offset = self.inner.offset();
		unsafe {
			// safe because the offset is kept
			self.as_mut_vec().reserve(offset + len);
		}
	}

	/// Returns the length of this body.
	pub fn len(&self) -> usize {
		self.inner.len()
	}

	#[cfg(feature = "json")]
	pub fn serialize<S: ?Sized>(&mut self, value: &S) -> Result<()>
	where
		S: Serialize,
	{
		serde_json::to_writer(self, value).map_err(|e| e.into())
	}

	#[cfg(feature = "fs")]
	pub async fn from_file<P>(&mut self, path: P) -> Result<()>
	where
		P: AsRef<Path>,
	{
		let mut file = File::open(path).await?;

		// check how big the file is then allocate
		let buf_size = file
			.metadata()
			.await
			.map(|m| m.len() as usize + 1)
			.unwrap_or(0);

		self.reserve(buf_size);

		unsafe {
			// safe because file.read_to_end only appends
			let v = self.as_mut_vec();
			file.read_to_end(v).await?;
		}

		Ok(())
	}

	/// ## Safety
	///
	/// You are not allowed to remove any data.
	#[doc(hidden)]
	pub unsafe fn as_mut_vec(&mut self) -> &mut Vec<u8> {
		self.inner.inner_mut().inner_mut()
	}
}

impl BytesWrite for BodyBytesMut<'_> {
	fn as_mut(&mut self) -> &mut [u8] {
		self.inner.as_mut()
	}

	fn as_bytes(&self) -> Bytes<'_> {
		self.inner.as_bytes()
	}

	fn remaining_mut(&mut self) -> &mut [u8] {
		self.inner.remaining_mut()
	}

	fn try_write(
		&mut self,
		slice: impl AsRef<[u8]>,
	) -> StdResult<(), bytes::WriteError> {
		self.inner.try_write(slice)
	}
}

impl BytesSeek for BodyBytesMut<'_> {
	fn position(&self) -> usize {
		self.inner.position()
	}

	fn try_seek(&mut self, pos: usize) -> StdResult<(), bytes::SeekError> {
		self.inner.try_seek(pos)
	}
}

impl io::Write for BodyBytesMut<'_> {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.inner.write(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}
