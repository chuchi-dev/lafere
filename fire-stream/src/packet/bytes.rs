use super::{BodyBytes, BodyBytesMut};

use bytes::{Bytes, BytesMut};


/// A trait that allows efficient allocation if encryption is
/// used or not.
pub trait PacketBytes: std::fmt::Debug {
	/// Creates a new Bytes instance.
	///
	/// It must always have header len available and initialized
	fn new(header_len: usize) -> Self;

	/// Returns the header Bytes.
	fn header(&self) -> Bytes<'_>;

	/// Returns the header mutably.
	fn header_mut(&mut self) -> BytesMut<'_>;

	/// Returns the full header mutably.
	/// 
	/// ## Note
	/// This should only be used to fill the buffer from a reader, in any other
	/// case you should use `header_mut`.
	fn full_header_mut(&mut self) -> BytesMut<'_>;

	/// Returns the body.
	fn body(&self) -> BodyBytes<'_>;

	/// Returns the body mutably.
	fn body_mut(&mut self) -> BodyBytesMut<'_>;

	/// Returns the full body mutably.
	/// 
	/// ## Note
	/// This should only be used to fill the buffer from a reader, in any other
	/// case you should use `body_mut`.
	fn full_body_mut(&mut self) -> BytesMut<'_>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainBytes {
	bytes: Vec<u8>,
	header_len: usize
}

impl PacketBytes for PlainBytes {
	fn new(header_len: usize) -> Self {
		Self {
			bytes: vec![0; header_len],
			header_len
		}
	}

	fn header(&self) -> Bytes<'_> {
		self.bytes[..self.header_len].into()
	}

	fn header_mut(&mut self) -> BytesMut<'_> {
		(&mut self.bytes[..self.header_len]).into()
	}

	fn full_header_mut(&mut self) -> BytesMut<'_> {
		self.header_mut()
	}

	fn body(&self) -> BodyBytes<'_> {
		BodyBytes::new(&self.bytes[self.header_len..])
	}

	fn body_mut(&mut self) -> BodyBytesMut<'_> {
		BodyBytesMut::new(self.header_len, &mut self.bytes)
	}

	fn full_body_mut(&mut self) -> BytesMut<'_> {
		(&mut self.bytes[self.header_len..]).into()
	}
}

impl PlainBytes {
	pub(crate) fn as_slice(&self) -> &[u8] {
		&*self.bytes
	}
}


#[cfg(test)]
pub(super) mod tests {
	use super::*;
	use bytes::{BytesOwned, BytesRead, BytesWrite};

	pub fn test_gen_msg<B: PacketBytes>() {
		let header = [10u8; 30];
		let mut bytes = B::new(header.len());
		bytes.header_mut().write(&header);
		assert_eq!(bytes.body().len(), 0);

		{
			let mut body = bytes.body_mut();

			// now i can't use bytes
			// but body

			body.write_u32(10u32);
			body.write_u32(20u32);
		}

		// assert_eq!(bytes.as_slice().len(), 16 + 30 + 16 + 4 + 4);

		assert_eq!(bytes.header().as_slice(), header);

		let mut body = BytesOwned::new();
		body.write_u32(10);
		body.write_u32(20);
		assert_eq!(bytes.body().as_slice(), body.as_slice());
		assert_eq!(bytes.body().len(), body.len());
	}

	#[test]
	fn plain_bytes() {
		test_gen_msg::<PlainBytes>();
	}
}