use super::{BodyBytes, BodyBytesMut, PacketBytes, PacketError};

use crypto::cipher::{Key, Mac};

use bytes::{Bytes, BytesMut, BytesRead, BytesWrite, BytesSeek};


const OFFSET: usize = Mac::LEN;

/// Encrypted bytes
/// 
/// Data Layout
/// +-----------+-----------+
/// |   Header  |   Body    |
/// +-----+-----+-----+-----+
/// | Mac | Ctn | Mac | Ctn |
/// +-----+-----+-----+-----+
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBytes {
	bytes: Vec<u8>,
	header_len: usize
}

impl PacketBytes for EncryptedBytes {
	fn new(header_len: usize) -> Self {
		Self {
			// 1 offset for header
			// 1 offset for body
			bytes: vec![0; OFFSET * 2 + header_len],
			header_len
		}
	}

	fn header(&self) -> Bytes<'_> {
		self.bytes[OFFSET..][..self.header_len].into()
	}

	fn header_mut(&mut self) -> BytesMut<'_> {
		(&mut self.bytes[OFFSET..][..self.header_len]).into()
	}

	fn full_header_mut(&mut self) -> BytesMut<'_> {
		(&mut self.bytes[..OFFSET + self.header_len]).into()
	}

	fn body(&self) -> BodyBytes<'_> {
		BodyBytes::new(&self.bytes[OFFSET * 2..][self.header_len..])
	}

	fn body_mut(&mut self) -> BodyBytesMut<'_> {
		BodyBytesMut::new(self.header_len + OFFSET * 2, &mut self.bytes)
	}

	fn full_body_mut(&mut self) -> BytesMut<'_> {
		(&mut self.bytes[OFFSET + self.header_len..]).into()
	}
}

impl EncryptedBytes {
	pub(crate) fn has_body(&self) -> bool {
		self.bytes.len() > OFFSET * 2 + self.header_len
	}

	// slice without body mac is returned
	pub(crate) fn as_slice(&self) -> &[u8] {
		if self.has_body() {
			&*self.bytes
		} else {
			// if we don't have any body
			// remove the space saved for it
			&self.bytes[..self.bytes.len() - OFFSET]
		}
	}

	pub(crate) fn encrypt(&mut self, key: &mut Key) {

		// encrypt header
		let mut header: BytesMut = self.full_header_mut().into();
		header.seek(OFFSET);
		let mac = key.encrypt(header.remaining_mut());
		header.seek(0);
		header.write(&mac.into_bytes());

		if !self.has_body() {
			return
		}

		// encrypt body
		let mut body: BytesMut = self.full_body_mut().into();
		body.seek(OFFSET);
		let mac = key.encrypt(body.remaining_mut());
		body.seek(0);
		body.write(&mac.into_bytes());

	}

	pub(crate) fn decrypt_header(
		&mut self,
		key: &mut Key
	) -> Result<(), PacketError> {
		let mut header: BytesMut = self.full_header_mut().into();
		let mac = Mac::from_slice(header.read(OFFSET));
		key.decrypt(header.remaining_mut(), &mac)
			.map_err(|_| PacketError::MacNotEqual)
	}

	pub(crate) fn decrypt_body(
		&mut self,
		key: &mut Key
	) -> Result<(), PacketError> {
		let mut body: BytesMut = self.full_body_mut().into();
		let mac = Mac::from_slice(body.read(OFFSET));
		key.decrypt(body.remaining_mut(), &mac)
			.map_err(|_| PacketError::MacNotEqual)
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::packet::bytes::tests::test_gen_msg;
	use crypto::cipher::{Keypair, Nonce};

	#[test]
	fn encrypted_bytes() {
		test_gen_msg::<EncryptedBytes>();
	}

	fn generate_key() -> Key {
		let alice = Keypair::new();
		let bob = Keypair::new();

		let ssk = alice.diffie_hellman(bob.public());

		let nonce = Nonce::from([1u8; 24]);
		ssk.to_key(nonce)
	}

	#[test]
	fn crypto() {

		let header = [10u8; 30];
		let mut bytes = EncryptedBytes::new(header.len());
		bytes.header_mut().write(&header);

		let body_buf = [20u8; 30];
		let mut body = bytes.body_mut();
		body.write(&body_buf);

		let mut alice_key = generate_key();
		let mut alice_key_2 = alice_key.dublicate();
		let mut bob_key = alice_key.dublicate();

		// should encrypt header
		bytes.encrypt(&mut alice_key);

		// validate header mac
		let mut mut_header = header.clone();
		let mac_r = alice_key_2.encrypt(&mut mut_header);
		assert_eq!(mac_r.into_bytes(), bytes.full_header_mut().as_mut()[..16]);
		assert_eq!(mut_header, bytes.header().as_slice());

		// validate body mac
		let mut mut_body = body_buf.clone();
		let mac_r = alice_key_2.encrypt(&mut mut_body);
		assert_eq!(mac_r.into_bytes()[..], bytes.full_body_mut().as_mut()[..16]);
		assert_eq!(mut_body, bytes.body().as_slice());

		// now decrypt everything
		bytes.decrypt_header(&mut bob_key).unwrap();
		bytes.decrypt_body(&mut bob_key).unwrap();

		assert_eq!(bytes.header().as_slice(), header);
		assert_eq!(bytes.body().as_slice(), body_buf);


	}

	#[test]
	fn empty_msg() {

		let header = [10u8; 30];
		let mut bytes = EncryptedBytes::new(header.len());

		assert!(!bytes.has_body());
		assert_eq!(bytes.body().len(), 0);
		// assert_eq!(bytes.as_slice().len(), 16 + 30);

		bytes.body_mut().write_u8(10);
		assert!(bytes.has_body());
		assert_eq!(bytes.body().len(), 1);
		// assert_eq!(bytes.as_slice().len(), 16 + 30 + 16 + 1);
		assert_eq!(bytes.body().read_u8(), 10);

		bytes.body_mut().resize(0);
		assert!(!bytes.has_body());
		assert_eq!(bytes.body().len(), 0);
		// assert_eq!(bytes.as_slice().len(), 16 + 30);

		bytes.body_mut().resize(1);
		assert!(bytes.has_body());
		assert_eq!(bytes.body().len(), 1);
		// assert_eq!(bytes.as_slice().len(), 16 + 30 + 16 + 1);
		assert_eq!(bytes.body().read_u8(), 0);

	}

}