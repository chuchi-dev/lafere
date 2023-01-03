
use super::handshake::{server_handshake, client_handshake};
use crate::timeout::TimeoutReader;
use crate::packet::{Packet, EncryptedBytes};
use crate::packet::builder::PacketReceiver;
use crate::error::StreamError;
use crate::traits::ByteStream;
use crate::handler::{client, server, TaskHandle, SendBack};
use crate::client::{Connection as Client, Config as ClientConfig, ReconStrat};
use crate::server::{Connection as Server, Config as ServerConfig};
use crate::watch;

use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::{interval, Duration, MissedTickBehavior};

use crypto::cipher::Key;
use crypto::signature as sign;

type Result<T> = std::result::Result<T, StreamError>;

pub fn client<S, P>(
	stream: S,
	cfg: ClientConfig,
	mut recon_strat: Option<ReconStrat<S>>,
	sign: sign::PublicKey
) -> Client<P>
where
	S: ByteStream,
	P: Packet<EncryptedBytes> + Send + 'static,
	P::Header: Send
{
	let (sender, mut cfg_rx, mut bg_handler) = client::Handler::new(cfg);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		client_bg_reconnect!(
			client_bg_stream(
				stream,
				bg_handler,
				cfg_rx,
				rx_close,
				recon_strat,
				|stream, cfg| {
					PacketStream::client(stream, sign.clone(), cfg).await
				}
			)
		);
	});

	let task = TaskHandle { close: tx_close, task };

	Client::new_raw(sender, task)
}

pub fn server<S, P>(
	stream: S,
	cfg: ServerConfig,
	sign: sign::Keypair
) -> Server<P>
where
	S: ByteStream,
	P: Packet<EncryptedBytes> + Send + 'static,
	P::Header: Send
{
	let (receiver, mut cfg_rx, mut bg_handler) = server::Handler::new(cfg);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		let stream = PacketStream::server(stream, sign, cfg_rx.newest()).await?;
		let r = server_bg_stream(
			stream,
			&mut bg_handler,
			&mut cfg_rx,
			&mut rx_close
		).await;

		if let Err(e) = &r {
			eprintln!("bg_stream closed with error {:?}", e);
		}

		r
	});

	let task = TaskHandle { close: tx_close, task };

	Server::new_raw(receiver, task)
}

/// inner manages a stream
struct PacketStream<S, P>
where
	S: ByteStream,
	P: Packet<EncryptedBytes>
{
	stream: TimeoutReader<S>,
	send_key: Key,
	recv_key: Key,
	// buffer to receive a message
	builder: PacketReceiver<P, EncryptedBytes>
}

impl<S, P> PacketStream<S, P>
where
	S: ByteStream,
	P: Packet<EncryptedBytes>
{

	fn timeout(&self) -> Duration {
		self.stream.timeout()
	}

	async fn client(
		stream: S,
		sign: sign::PublicKey,
		cfg: ClientConfig
	) -> Result<Self> {
		let mut stream = TimeoutReader::new(stream, cfg.timeout);
		let handshake = client_handshake(&sign, &mut stream).await?;
		Ok(Self {
			stream,
			send_key: handshake.send_key,
			recv_key: handshake.recv_key,
			builder: PacketReceiver::new(cfg.body_limit)
		})
	}

	async fn server(
		stream: S,
		sign: sign::Keypair,
		cfg: ServerConfig
	) -> Result<Self> {
		let mut stream = TimeoutReader::new(stream, cfg.timeout);
		let handshake = server_handshake(&sign, &mut stream).await?;
		Ok(Self {
			stream,
			send_key: handshake.send_key,
			recv_key: handshake.recv_key,
			builder: PacketReceiver::new(cfg.body_limit)
		})
	}

	async fn send(&mut self, packet: P) -> Result<()> {
		let mut bytes = packet.into_bytes();
		bytes.encrypt(&mut self.send_key);
		let slice = bytes.as_slice();
		self.stream.write_all(slice).await?;
		self.stream.flush().await?;
		Ok(())
	}

	/// this function is abort safe
	async fn receive(&mut self) -> Result<P> {
		let recv_key = &mut self.recv_key;
		self.builder.read_header(&mut self.stream, |bytes| {
			bytes.decrypt_header(recv_key)
				.map_err(|e| e.into())
		}).await?;
		self.builder.read_body(&mut self.stream, |bytes| {
			bytes.decrypt_body(recv_key)
				.map_err(|e| e.into())
		}).await
	}

	async fn shutdown(&mut self) -> Result<()> {
		self.stream.shutdown().await.map_err(Into::into)
	}
}

bg_stream!(
	client_bg_stream,
	client::Handler<P, EncryptedBytes>,
	EncryptedBytes,
	ClientConfig
);
bg_stream!(
	server_bg_stream,
	server::Handler<P, EncryptedBytes>,
	EncryptedBytes,
	ServerConfig
);

#[cfg(test)]
mod tests {

	use super::*;
	use crate::packet::test::{TestPacket};
	use crate::handler::Message;
	use crypto::signature::Keypair;

	use tokio::net::{TcpStream, TcpListener};

	async fn tcp_streams() -> (TcpStream, TcpStream) {

		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

		let addr = listener.local_addr().unwrap();

		let connect = TcpStream::connect(addr);

		let accept = listener.accept();

		let (connect, accept) = tokio::join!(connect, accept);

		(
			connect.unwrap(),
			accept.unwrap().0
		)
	}

	#[tokio::test]
	async fn test_encrypted_stream() {

		let timeout = Duration::from_secs(1);
		let key = Keypair::new();
		let public_key = key.public().clone();

		let (alice, bob) = tcp_streams().await;

		let alice: Client<TestPacket<_>> = client(alice, ClientConfig {
			timeout,
			body_limit: 200
		}, None, public_key);

		let bob_task = tokio::spawn(async move {

			let mut bob: Server<TestPacket<_>> = server(bob, ServerConfig {
			timeout,
			body_limit: 200
		}, key);

			// let's receive a request message
			let req = bob.receive().await.unwrap();
			match req {
				Message::Request(req, resp) => {
					assert_eq!(req.num1, 1);
					assert_eq!(req.num2, 2);

					// send response
					let res = TestPacket::new(3, 4);
					resp.send(res).unwrap();
				},
				_ => panic!("expected request")
			};

			let req = bob.receive().await.unwrap();
			match req {
				Message::OpenStream(req, stream) => {
					assert_eq!(req.num1, 5);
					assert_eq!(req.num2, 6);

					// send response
					let res = TestPacket::new(7, 8);
					stream.send(res).await.unwrap();

					let res = TestPacket::new(9, 10);
					stream.send(res).await.unwrap();
				},
				_ => panic!("expected stream")
			};

			let req = bob.receive().await.unwrap();
			match req {
				Message::CreateStream(req, mut stream) => {
					assert_eq!(req.num1, 11);
					assert_eq!(req.num2, 12);

					// send response
					let res = stream.receive().await.unwrap();
					assert_eq!(res.num1, 13);
					assert_eq!(res.num2, 14);

					let res = stream.receive().await.unwrap();
					assert_eq!(res.num1, 15);
					assert_eq!(res.num2, 16);
				},
				_ => panic!("expected stream")
			};

			bob.wait().await.unwrap();

		});

		// let's make a request
		let req = TestPacket::new(1, 2);
		let res = alice.request(req).await.unwrap();
		assert_eq!(res.num1, 3);
		assert_eq!(res.num2, 4);

		// let's create a stream to listen
		let req = TestPacket::new(5, 6);
		let mut stream = alice.open_stream(req).await.unwrap();

		let res = stream.receive().await.unwrap();
		assert_eq!(res.num1, 7);
		assert_eq!(res.num2, 8);

		let res = stream.receive().await.unwrap();
		assert_eq!(res.num1, 9);
		assert_eq!(res.num2, 10);
		drop(stream);

		// now request a stream.sender
		let req = TestPacket::new(11, 12);
		let stream = alice.create_stream(req).await.unwrap();

		let req = TestPacket::new(13, 14);
		stream.send(req).await.unwrap();

		let req = TestPacket::new(15, 16);
		stream.send(req).await.unwrap();
		drop(stream);

		alice.close().await.unwrap();

		// wait until bob's task finishes
		bob_task.await.unwrap();
	}

}