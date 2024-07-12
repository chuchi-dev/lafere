use bytes::{BytesRead, BytesWrite, ReadError, WriteError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Varint(pub u64);

impl Varint {
	pub fn read<R: BytesRead>(reader: &mut R) -> Result<Self, ReadError> {
		let mut val: u64 = 0;

		let mut msb = false;

		for i in 0..10 {
			let b = reader.try_read_u8()?;

			val |= ((b & 0b0111_1111) as u64) << (i * 7);

			// has most significant bit
			msb = b & 0b1000_0000 > 0;
			if !msb {
				break;
			}
		}

		if msb {
			// we should not get an msb on the last byte
			return Err(ReadError);
		}

		Ok(Self(val))
	}

	pub fn write<W: BytesWrite>(
		&self,
		writer: &mut W,
	) -> Result<(), WriteError> {
		let mut val: u64 = self.0;

		for _ in 1..=10 {
			// get the first 7bytes
			let mut b = val as u8 & 0b0111_1111;
			val >>= 7;

			let msb = val > 0;
			if msb {
				b |= 0b1000_0000;
			}

			writer.try_write_u8(b)?;

			if !msb {
				break;
			}
		}

		Ok(())
	}

	pub fn size(&self) -> u64 {
		let bits = 64 - self.0.leading_zeros();
		let bytes = (bits as f32 / 7f32).ceil() as u64;

		bytes.max(1)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use bytes::{BytesOwned, BytesSeek};

	#[test]
	fn varints() {
		assert_eq!(Varint(0).size(), 1);
		let mut bytes = BytesOwned::new();
		Varint(0).write(&mut bytes).unwrap();
		assert_eq!(bytes.as_slice(), &[0]);

		let mut r_bytes = BytesOwned::from(vec![0b000_0001]);
		let varint = Varint::read(&mut r_bytes).unwrap();
		assert_eq!(varint.0, 1);
		assert_eq!(varint.size(), 1);
		let mut w_bytes = BytesOwned::new();
		varint.write(&mut w_bytes).unwrap();
		assert_eq!(w_bytes.as_slice(), r_bytes.as_slice());

		let mut r_bytes = BytesOwned::from(vec![0b1001_0110, 0b0000_0001]);
		let varint = Varint::read(&mut r_bytes).unwrap();
		assert_eq!(varint.size(), 2);
		assert_eq!(varint.0, 150);
		let mut w_bytes = BytesOwned::new();
		varint.write(&mut w_bytes).unwrap();
		assert_eq!(w_bytes.as_slice(), r_bytes.as_slice());

		let mut bytes = BytesOwned::new();
		Varint(u64::MAX).write(&mut bytes).unwrap();
		bytes.seek(0);
		let varint = Varint::read(&mut bytes).unwrap();
		assert_eq!(varint.0, u64::MAX);
		assert_eq!(varint.size(), 10);
	}
}
