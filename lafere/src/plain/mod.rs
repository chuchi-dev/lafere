use crate::client::{Config as ClientConfig, Connection as Client, ReconStrat};
use crate::handler::TaskHandle;
use crate::handler::handler::{
	Handler, PacketStream, bg_stream, bg_stream_reconnect,
};
use crate::packet::builder::{PacketReceiver, PacketReceiverError};
use crate::packet::{Packet, PlainBytes};
use crate::server::{Config as ServerConfig, Connection as Server};
use crate::util::{ByteStream, TimeoutReader};

use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::Duration;

use std::io;

/// Creates a new client from a stream, without using any encryption.
pub(crate) fn client<S, P>(
	byte_stream: S,
	cfg: ClientConfig,
	recon_strat: Option<ReconStrat<S>>,
) -> Client<P>
where
	S: ByteStream,
	P: Packet<PlainBytes> + Send + 'static,
	P::Header: Send,
{
	let (sender, _, mut cfg_rx, mut bg_handler) = Handler::new(cfg, false);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		bg_stream_reconnect(
			byte_stream,
			&mut bg_handler,
			&mut cfg_rx,
			&mut rx_close,
			|stream: &mut PlainPacketStream<_, _>, cfg| {
				stream.stream.set_timeout(cfg.timeout);
				stream.builder.set_body_limit(cfg.body_limit);
			},
			recon_strat,
			|stream, cfg| async move {
				Ok(PlainPacketStream::new(stream, cfg.timeout, cfg.body_limit))
			},
		)
		.await
	});

	let task = TaskHandle {
		close: tx_close,
		task,
	};

	Client::new_raw(sender, task)
}

/// Creates a new server from a stream, without using any encryption.
pub(crate) fn server<S, P>(stream: S, cfg: ServerConfig) -> Server<P>
where
	S: ByteStream,
	P: Packet<PlainBytes> + Send + 'static,
	P::Header: Send,
{
	let stream = PlainPacketStream::new(stream, cfg.timeout, cfg.body_limit);
	let (_, receiver, mut cfg_rx, mut bg_handler) = Handler::new(cfg, true);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		let r = bg_stream(
			stream,
			&mut bg_handler,
			&mut cfg_rx,
			&mut rx_close,
			|stream, cfg| {
				stream.stream.set_timeout(cfg.timeout);
				stream.builder.set_body_limit(cfg.body_limit);
			},
		)
		.await;

		if let Err(e) = &r {
			tracing::error!("server_bg_stream error {:?}", e)
		}

		r
	});

	let task = TaskHandle {
		close: tx_close,
		task,
	};

	Server::new_raw(receiver, task)
}

/// inner manages a stream
struct PlainPacketStream<S, P>
where
	S: ByteStream,
	P: Packet<PlainBytes>,
{
	stream: TimeoutReader<S>,
	// buffer to receive a message
	builder: PacketReceiver<P, PlainBytes>,
}

impl<S, P> PlainPacketStream<S, P>
where
	S: ByteStream,
	P: Packet<PlainBytes>,
{
	fn new(stream: S, timeout: Duration, body_limit: u32) -> Self {
		Self {
			stream: TimeoutReader::new(stream, timeout),
			builder: PacketReceiver::new(body_limit),
		}
	}
}

impl<S, P> PacketStream<P, PlainBytes> for PlainPacketStream<S, P>
where
	S: ByteStream,
	P: Packet<PlainBytes>,
{
	fn timeout(&self) -> Duration {
		self.stream.timeout()
	}

	async fn send(&mut self, packet: P) -> Result<(), io::Error> {
		let bytes = packet.into_bytes();
		let slice = bytes.as_slice();
		self.stream.write_all(slice).await?;
		self.stream.flush().await?;
		Ok(())
	}

	/// this function is abort safe
	async fn receive(&mut self) -> Result<P, PacketReceiverError<P::Header>> {
		self.builder
			.read_header(&mut self.stream, |_| Ok(()))
			.await?;
		self.builder.read_body(&mut self.stream, |_| Ok(())).await
	}

	async fn shutdown(&mut self) -> Result<(), io::Error> {
		self.stream.shutdown().await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::packet::test::TestPacket;
	use crate::server::Request;
	use crate::util::PinnedFuture;

	use tokio::net::{TcpListener, TcpStream};
	use tokio::time::{Duration, sleep};

	/// create two tcp stream which communicate with each other
	async fn tcp_streams() -> (TcpStream, TcpStream) {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();

		let connect = TcpStream::connect(addr);
		let accept = listener.accept();
		let (connect, accept) = tokio::join!(connect, accept);

		(connect.unwrap(), accept.unwrap().0)
	}

	#[tokio::test]
	async fn test_plain_stream() {
		let timeout = Duration::from_secs(1);

		let (alice, bob) = tcp_streams().await;

		let alice: Client<TestPacket<_>> = client(
			alice,
			ClientConfig {
				timeout,
				body_limit: 200,
			},
			None,
		);

		let bob_task = tokio::spawn(async move {
			let mut bob: Server<TestPacket<_>> = server(
				bob,
				ServerConfig {
					timeout,
					body_limit: 200,
				},
			);

			// let's receive a request message
			let req = bob.receive().await.unwrap();
			match req {
				Request::Request(req, resp) => {
					assert_eq!(req.num1, 1);
					assert_eq!(req.num2, 2);

					// send response
					let res = TestPacket::new(3, 4);
					resp.send(res).unwrap();
				}
				_ => panic!("expected request"),
			};

			let req = bob.receive().await.unwrap();
			match req {
				Request::RequestReceiver(req, stream) => {
					assert_eq!(req.num1, 5);
					assert_eq!(req.num2, 6);

					// send response
					let res = TestPacket::new(7, 8);
					stream.send(res).await.unwrap();

					let res = TestPacket::new(9, 10);
					stream.send(res).await.unwrap();
				}
				_ => panic!("expected stream"),
			};

			let req = bob.receive().await.unwrap();
			match req {
				Request::RequestSender(req, mut stream) => {
					assert_eq!(req.num1, 11);
					assert_eq!(req.num2, 12);

					// send response
					let res = stream.receive().await.unwrap();
					assert_eq!(res.num1, 13);
					assert_eq!(res.num2, 14);

					let res = stream.receive().await.unwrap();
					assert_eq!(res.num1, 15);
					assert_eq!(res.num2, 16);
				}
				_ => panic!("expected stream"),
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
		let mut stream = alice.request_receiver(req).await.unwrap();

		let res = stream.receive().await.unwrap();
		assert_eq!(res.num1, 7);
		assert_eq!(res.num2, 8);

		let res = stream.receive().await.unwrap();
		assert_eq!(res.num1, 9);
		assert_eq!(res.num2, 10);
		drop(stream);

		// now request a stream.sender
		let req = TestPacket::new(11, 12);
		let stream = alice.request_sender(req).await.unwrap();

		let req = TestPacket::new(13, 14);
		stream.send(req).await.unwrap();

		let req = TestPacket::new(15, 16);
		stream.send(req).await.unwrap();
		drop(stream);

		alice.close().await.unwrap();

		// wait until bob's task finishes
		bob_task.await.unwrap();
	}

	#[tokio::test]
	async fn test_plain_stream_reconnect() {
		let timeout = Duration::from_millis(20);

		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();

		let server = tokio::spawn(async move {
			let mut c = 0;
			'main: loop {
				// if i == 0
				// close stream early

				c += 1;

				let accept = listener.accept().await.unwrap().0;

				let mut bob: Server<TestPacket<_>> = server(
					accept,
					ServerConfig {
						timeout,
						body_limit: 200,
					},
				);

				loop {
					// let's receive a request message
					let req = bob.receive().await;
					let req = match req {
						Some(r) => r,
						None => continue 'main,
					};

					match req {
						Request::Request(req, resp) => {
							// send response
							let res = TestPacket::new(req.num1, req.num2);
							resp.send(res).unwrap();

							if req.num1 == 3 {
								break;
							}
						}
						_ => panic!("expected request"),
					};

					if c == 1 {
						// we need to wait so the
						sleep(Duration::from_millis(100)).await;
						bob.abort();
						continue 'main;
					}
				}

				bob.wait().await.expect("bob failed");
				break;
			}
		});

		let alice: Client<TestPacket<_>> = client(
			TcpStream::connect(addr).await.unwrap(),
			ClientConfig {
				timeout,
				body_limit: 200,
			},
			Some(ReconStrat::new(move |err_count| {
				let addr = addr.clone();
				assert!(err_count < 10);
				PinnedFuture::new(async move {
					sleep(Duration::from_millis(10)).await;
					TcpStream::connect(addr).await
				})
			})),
		);

		// first request should succeed
		let req = TestPacket::new(1, 2);
		let res = alice.request(req).await.unwrap();
		assert_eq!(res.num1, 1);
		assert_eq!(res.num2, 2);

		let mut retry_counter = 0;

		// loop until we get a response
		loop {
			assert!(retry_counter < 10);

			let req = TestPacket::new(3, 4);
			let res = alice.request(req).await;
			let res = match res {
				Ok(r) => r,
				Err(_) => {
					retry_counter += 1;
					sleep(Duration::from_millis(100)).await;
					continue;
				}
			};
			assert_eq!(res.num1, 3);
			assert_eq!(res.num2, 4);
			break;
		}

		alice.close().await.unwrap();

		// wait until bob's task finishes
		server.await.unwrap();
	}
}
