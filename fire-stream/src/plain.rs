
use crate::client::{Connection as Client, Config as ClientConfig, ReconStrat};
use crate::server::{Connection as Server, Config as ServerConfig};
use crate::timeout::TimeoutReader;
use crate::packet::{Packet, PlainBytes};
use crate::packet::builder::PacketReceiver;
use crate::error::StreamError;
use crate::traits::ByteStream;
use crate::watch;
use crate::handler::{client, server, TaskHandle, SendBack};

use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::{interval, Duration, MissedTickBehavior};

type Result<T> = std::result::Result<T, StreamError>;

macro_rules! client_bg_reconnect {
	(
		$fn:ident(
			$stream:ident,
			$bg_handler:ident,
			$cfg_rx:ident,
			$rx_close:ident,
			$recon_strat:ident,
			|$n_stream:ident, $cfg:ident| $block:block
		)
	) => (

		let mut stream = Some($stream);

		loop {

			// reconnect if 
			let stream = match (stream.take(), &mut $recon_strat) {
				(Some(s), _) => s,
				// no recon and no stream
				// close connection
				(None, None) => unreachable!(),
				(None, Some(recon)) => {
					let mut err_counter = 0;

					loop {

						let stream = (recon.inner)(err_counter).await;
						match stream {
							Ok(s) => break s,
							Err(e) => {
								eprintln!(
									"reconnect failed attempt {} {:?}",
									err_counter,
									e
								);
								err_counter += 1;
							}
						}

					}
				}
			};

			let cfg = $cfg_rx.newest();
			let stream = {
				let $n_stream = stream;
				let $cfg = cfg;

				$block
			};
			let stream = match stream {
				Ok(s) => s,
				Err(e) => {
					eprintln!("creating packetstream failed {:?}", e);
					// close since we can't reconnect
					if $recon_strat.is_none() {
						return Err(e)
					}

					continue
				}
			};

			let r = $fn(
				stream,
				&mut $bg_handler,
				&mut $cfg_rx,
				&mut $rx_close
			).await;

			match r {
				Ok(o) => return Ok(o),
				Err(e) => {
					eprintln!("client_bg_stream closed with error {:?}", e);
					if $recon_strat.is_none() {
						// close since we can't reconnect
						return Err(e)
					}
				}
			}

			// close all started requests
			$bg_handler.close_all_started();

		}
	)
}

/// Creates a new client from a stream, without using any encryption.
pub(crate) fn client<S, P>(
	byte_stream: S,
	cfg: ClientConfig,
	mut recon_strat: Option<ReconStrat<S>>
) -> Client<P>
where
	S: ByteStream,
	P: Packet<PlainBytes> + Send + 'static,
	P::Header: Send
{
	let (sender, mut cfg_rx, mut bg_handler) = client::Handler::new(cfg);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		client_bg_reconnect!(
			client_bg_stream(
				byte_stream,
				bg_handler,
				cfg_rx,
				rx_close,
				recon_strat,
				|stream, cfg| {
					Ok(PacketStream::new(stream, cfg.timeout, cfg.body_limit))
				}
			)
		);
	});

	let task = TaskHandle { close: tx_close, task };

	Client::new_raw(sender, task)
}

/// Creates a new server from a stream, without using any encryption.
pub(crate) fn server<S, P>(stream: S, cfg: ServerConfig) -> Server<P>
where
	S: ByteStream,
	P: Packet<PlainBytes> + Send + 'static,
	P::Header: Send
{
	let stream = PacketStream::new(stream, cfg.timeout, cfg.body_limit);
	let (receiver, mut cfg_rx, mut bg_handler) = server::Handler::new(cfg);

	let (tx_close, mut rx_close) = oneshot::channel();
	let task = tokio::spawn(async move {
		let r = server_bg_stream(
			stream,
			&mut bg_handler,
			&mut cfg_rx,
			&mut rx_close
		).await;

		if let Err(e) = &r {
			eprintln!("server_bg_stream error {:?}", e)
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
	P: Packet<PlainBytes>
{
	stream: TimeoutReader<S>,
	// buffer to receive a message
	builder: PacketReceiver<P, PlainBytes>
}

impl<S, P> PacketStream<S, P>
where
	S: ByteStream,
	P: Packet<PlainBytes>
{

	fn new(stream: S, timeout: Duration, body_limit: usize) -> Self {
		Self {
			stream: TimeoutReader::new(stream, timeout),
			builder: PacketReceiver::new(body_limit)
		}
	}

	fn timeout(&self) -> Duration {
		self.stream.timeout()
	}

	async fn send(&mut self, packet: P) -> Result<()> {
		let bytes = packet.into_bytes();
		let slice = bytes.as_slice();
		self.stream.write_all(slice).await?;
		self.stream.flush().await?;
		Ok(())
	}

	/// this function is abort safe
	async fn receive(&mut self) -> Result<P> {
		self.builder.read_header(&mut self.stream, |_| Ok(())).await?;
		self.builder.read_body(&mut self.stream, |_| Ok(())).await
	}

}

macro_rules! bg_stream {
	($name:ident, $handler:ty, $bytes:ty, $cfg:ty) => {
		async fn $name<S, P>(
			mut stream: PacketStream<S, P>,
			handler: &mut $handler,
			cfg_rx: &mut watch::Receiver<$cfg>,
			mut close: &mut oneshot::Receiver<()>
		) -> Result<()>
		where
			S: ByteStream,
			P: Packet<$bytes>
		{
			let mut should_close = false;
			let mut close_packet = None;

			let timeout = stream.timeout();
			let diff = match timeout.as_secs() {
				0..=1 => 0,
				0..=10 => 1,
				_ => 5
			};
			let mut interval = interval(stream.timeout() - Duration::from_secs(diff));
			interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

			loop {
				tokio::select!{
					packet = stream.receive(), if !should_close => {
						let packet = packet?;
						let r_packet = handler.send(packet).await?;
						match r_packet {
							SendBack::None => {},
							SendBack::Packet(p) => {
								stream.send(p).await?;
							},
							SendBack::Close => {
								should_close = true;
								let _ = handler.close();
							}
						}
					},
					Some(packet) = handler.to_send() => {
						// Todo make this not block until everything is sent
						// this can stop receiving
						stream.send(packet).await?;
					},
					_ping = interval.tick(), if !should_close => {
						stream.send(handler.ping_packet()).await?;
					},
					_ = &mut close, if !should_close => {
						should_close = true;
						let packet = handler.close();
						close_packet = Some(packet);
					},
					Some(cfg) = cfg_rx.recv(), if !should_close => {
						// should update configuration
						stream.stream.set_timeout(cfg.timeout);
						stream.builder.set_body_limit(cfg.body_limit);
					},
					else => {
						if let Some(packet) = close_packet.take() {
							let _ = stream.send(packet).await;
						}
						return Ok(())
					}
				}

			}
		}
	}
}

bg_stream!(
	client_bg_stream, client::Handler<P, PlainBytes>, PlainBytes, ClientConfig
);
bg_stream!(
	server_bg_stream, server::Handler<P, PlainBytes>, PlainBytes, ServerConfig
);

/*
let r = $fn(
			stream,
			&mut bg_handler,
			&mut cfg_rx,
			&mut rx_close
		).await;

		if let Err(e) = &r {
			eprintln!("client_bg_stream closed with error {:?}", e);
		}

		let mut recon = match recon_strat {
			Some(r) => r,
			None => return r
		};

		if let Ok(o) = r {
			return Ok(o)
		}

		// close all started requests
		bg_handler.close_all_started();

		loop {

			let mut err_counter = 0;

			// connect
			let byte_stream = loop {

				let stream = (recon.inner)(err_counter).await;
				match stream {
					Ok(s) => break s,
					Err(e) => {
						eprintln!(
							"reconnect failed attempt {} {:?}",
							err_counter,
							e
						);
						err_counter += 1;
					}
				}

			};

			let cfg = cfg_rx.newest();
			let stream = PacketStream::new(
				byte_stream,
				cfg.timeout,
				cfg.body_limit
			);
			let r = client_bg_stream(
				stream,
				&mut bg_handler,
				&mut cfg_rx,
				&mut rx_close
			).await;

			match r {
				Ok(o) => return Ok(o),
				Err(e) => {
					eprintln!("client_bg_stream closed with error {:?}", e)
				}
			}

			// close all started requests
			bg_handler.close_all_started();
		}
*/
		



#[cfg(test)]
mod tests {

	use super::*;
	use crate::packet::test::{TestPacket};
	use crate::handler::Message;
	use crate::pinned_future::PinnedFuture;

	use tokio::net::{TcpStream, TcpListener};
	use tokio::time::{sleep, Duration};

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
	async fn test_plain_stream() {

		let timeout = Duration::from_secs(1);

		let (alice, bob) = tcp_streams().await;

		let alice: Client<TestPacket<_>> = client(alice, ClientConfig {
			timeout,
			body_limit: 200
		}, None);

		let bob_task = tokio::spawn(async move {

			let mut bob: Server<TestPacket<_>> = server(bob, ServerConfig {
				timeout,
				body_limit: 200
			});

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

		println!("waiting for alice to close");

		alice.close().await.unwrap();

		// wait until bob's task finishes
		bob_task.await.unwrap();
	}

	#[tokio::test]
	async fn test_plain_stream_reconnect() {

		let timeout = Duration::from_millis(10);

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
						body_limit: 200
					}
				);

				loop {

					// let's receive a request message
					let req = bob.receive().await;
					let req = match req {
						Some(r) => r,
						None => continue 'main
					};

					match req {
						Message::Request(req, resp) => {
							// send response
							let res = TestPacket::new(req.num1, req.num2);
							resp.send(res).unwrap();

							if req.num1 == 3 {
								break
							}
						},
						_ => panic!("expected request")
					};

					if c == 1 {
						// we need to wait so the 
						sleep(Duration::from_millis(100)).await;
						bob.abort();
						continue 'main;
					}

				}

				bob.wait().await.expect("bob failed");
				break
			}
		});

		let alice: Client<TestPacket<_>> = client(
			TcpStream::connect(addr).await.unwrap(),
			ClientConfig {
				timeout,
				body_limit: 200
			},
			Some(ReconStrat::new(move |err_count| {
				let addr = addr.clone();
				assert!(err_count < 10);
				PinnedFuture::new(async move {
					sleep(Duration::from_millis(10)).await;
					TcpStream::connect(addr).await
				})
			}))
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
					continue
				}
			};
			assert_eq!(res.num1, 3);
			assert_eq!(res.num2, 4);
			break

		}

		alice.close().await.unwrap();

		// wait until bob's task finishes
		server.await.unwrap();
	}

}