
use crypto::cipher::{EphemeralKeypair, Key, Nonce, PublicKey};
use crypto::signature::{self as sign, Signature};
use bytes::{Cursor, BytesRead, BytesWrite, BytesSeek};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

type Result<T> = std::result::Result<T, crate::StreamError>;

const BASE_LEN: usize = PublicKey::LEN + Nonce::LEN * 2;
const TO_SIGN_LEN: usize = BASE_LEN + PublicKey::LEN;
const TO_SEND_LEN: usize = BASE_LEN + Signature::LEN;
const MAX_LEN: usize = const_max(TO_SIGN_LEN, TO_SEND_LEN);

const fn const_max(a: usize, b: usize) -> usize {
	match a > b {
		true => a,
		false => b
	}
}


pub(crate) struct Handshake {
	pub send_key: Key,
	pub recv_key: Key,
	#[allow(dead_code)]
	pub public_key: PublicKey
}

/// 1. receive pk from client
/// 2. send pk + send_nonce + recv_nonce + (signature of pk + nonce + cpk)
/// 
/// ## Abort safe
/// This function is not abort safe.
pub(crate) async fn server_handshake<S>(
	sign_key: &sign::Keypair,
	stream: &mut S
) -> Result<Handshake>
where S: AsyncRead + AsyncWrite + Unpin {

	// - client pk
	let client_pk = {
		let mut buffer = [0u8; PublicKey::LEN];

		// - receive 32 bytes (random client public key)
		stream.read_exact(&mut buffer).await?;

		PublicKey::from(buffer)
	};	

	let server_kp = EphemeralKeypair::new();
	let send_nonce = Nonce::new();
	let recv_nonce = Nonce::new();
	debug_assert_ne!(send_nonce, recv_nonce);


	// - write all to the buffer
	{
		let mut buffer = Cursor::new([0u8; MAX_LEN]);
		buffer.write(server_kp.public());
		buffer.write(&send_nonce);
		buffer.write(&recv_nonce);
		buffer.write(&client_pk);

		// get signature
		buffer.seek(0);
		let sign = sign_key.sign(buffer.read(TO_SIGN_LEN));

		// write signature
		buffer.seek(BASE_LEN);
		buffer.write(sign);

		// now send
		buffer.seek(0);
		stream.write_all(buffer.read(TO_SEND_LEN)).await?;
	}

	// - diffie_hellman
	let shared_secret = server_kp.diffie_hellman(&client_pk);

	// generate keys
	let send_key = shared_secret.to_key(send_nonce);
	let recv_key = shared_secret.to_key(recv_nonce);

	Ok(Handshake {
		send_key,
		recv_key,
		public_key: client_pk
	})
}


/// 1. send pk
/// 2. receive pk + send_nonce + recv_nonce + (signature of pk + nonce + cpk)
/// 
/// ## Abort safe
/// This function is not abort safe.
pub(crate) async fn client_handshake<S>(
	sign_pk: &sign::PublicKey,
	stream: &mut S
) -> Result<Handshake>
where S: AsyncRead + AsyncWrite + Unpin {

	let client_kp = EphemeralKeypair::new();
	// send pk
	stream.write_all(client_kp.public().as_ref()).await?;


	let (server_pk, send_nonce, recv_nonce) = {
		let mut buffer = Cursor::new([0u8; MAX_LEN]);

		stream.read_exact(&mut buffer.as_mut()[..TO_SEND_LEN]).await?;

		// verify signature
		buffer.seek(BASE_LEN);
		let sign = Signature::from_slice(buffer.read(Signature::LEN));

		buffer.seek(BASE_LEN);
		buffer.write(client_kp.public());

		buffer.seek(0);
		if !sign_pk.verify(buffer.read(TO_SIGN_LEN), &sign) {
			todo!("signatured does not match")
		}

		// read message
		buffer.seek(0);
		let server_pk = PublicKey::from_slice(
			buffer.read(PublicKey::LEN)
		);
		let recv_nonce = Nonce::from_slice(
			buffer.read(Nonce::LEN)
		);
		let send_nonce = Nonce::from_slice(
			buffer.read(Nonce::LEN)
		);
		debug_assert_ne!(recv_nonce, send_nonce);

		(server_pk, send_nonce, recv_nonce)
	};


	// - diffie_hellman
	let shared_secret = client_kp.diffie_hellman(&server_pk);

	// generate keys
	let send_key = shared_secret.to_key(send_nonce);
	let recv_key = shared_secret.to_key(recv_nonce);

	Ok(Handshake {
		send_key,
		recv_key,
		public_key: server_pk
	})
}
