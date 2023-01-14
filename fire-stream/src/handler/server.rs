use super::{SendBack, StreamSender, StreamReceiver, Configurator};
use crate::error::{ResponseError, TaskError};
use crate::util::{watch, poll_fn};
use crate::packet::{
	Packet, Kind, Flags, PacketHeader, PacketBytes, PacketError
};
use crate::server::Config;

use std::collections::{HashMap, hash_map::Entry};
use std::future::Future;
use std::task::Poll;
use std::marker::PhantomData;
use std::pin::Pin;

use tokio::sync::{mpsc, oneshot};


/// A receiver that waits on messages from the handler (client)
pub(crate) struct Receiver<P> {
	inner: mpsc::Receiver<Message<P>>,
	cfg: watch::Sender<Config>
}

impl<P> Receiver<P> {
	/// Receive a new message from the client
	pub async fn receive(&mut self) -> Option<Message<P>> {
		self.inner.recv().await
	}

	pub fn update_config(&self, cfg: Config) {
		self.cfg.send(cfg);
	}

	pub fn configurator(&self) -> Configurator<Config> {
		Configurator::new(self.cfg.clone())
	}
}

/// All different kinds of messages.
#[derive(Debug)]
pub enum Message<P> {
	Request(P, ResponseSender<P>),
	// a request to receive a sender stream
	RequestSender(P, StreamReceiver<P>),
	// a request to receive a receiving stream
	RequestReceiver(P, StreamSender<P>)
}

/// A sender used to respond to a request.
#[derive(Debug)]
pub struct ResponseSender<P> {
	pub(crate) inner: oneshot::Sender<P>
}

impl<P> ResponseSender<P> {
	pub(crate) fn new(inner: oneshot::Sender<P>) -> Self {
		Self { inner }
	}

	/// Sends the packet as a response, adding the correct flags.
	/// 
	/// If this returns an Error it either means the connection was closed
	/// or the requestor does not care about the response anymore.
	pub fn send(
		self,
		packet: P
	) -> Result<(), ResponseError> {
		self.inner.send(packet)
			.map_err(|_| ResponseError::ConnectionClosed)
	}
}

pub enum Response<P> {
	Request(oneshot::Receiver<P>),
	// a request to receive a receiving stream
	Receiver(mpsc::Receiver<P>)
}

/// A list of receivers that wait on a packet.
struct WaitingOnServer<P, B> {
	// hashmap because we need to check if the id is free
	inner: HashMap<u32, Response<P>>,
	marker: PhantomData<B>
}


impl<P, B> WaitingOnServer<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	fn new() -> Self {
		Self {
			inner: HashMap::new(),
			marker: PhantomData
		}
	}

	fn insert(
		&mut self,
		id: u32,
		receiver: Response<P>
	) -> Result<(), TaskError> {
		match self.inner.entry(id) {
			Entry::Occupied(occ) => Err(TaskError::ExistingId(*occ.key())),
			Entry::Vacant(v) => {
				v.insert(receiver);
				Ok(())
			}
		}
	}

	pub async fn to_send(&mut self) -> Option<P> {
		if self.inner.is_empty() {
			return None
		}

		let (packet, rem) = poll_fn(|ctx| {

			for (id, resp) in &mut self.inner {
				match resp {
					Response::Request(resp) => {
						match Pin::new(resp).poll(ctx) {
							Poll::Pending => {},
							Poll::Ready(Ok(mut packet)) => {
								// set kind::Stream and set the id
								let flags = Flags::new(Kind::Response);
								packet.header_mut().set_flags(flags);
								packet.header_mut().set_id(*id);

								return Poll::Ready((packet, Some(*id)))
							},
							Poll::Ready(Err(_)) => {
								// channel closed
								let mut p = P::empty();
								let flags = Flags::new(Kind::NoResponse);
								p.header_mut().set_flags(flags);
								p.header_mut().set_id(*id);

								return Poll::Ready((p, Some(*id)))
							}
						}
					},
					Response::Receiver(resp) => {
						match resp.poll_recv(ctx) {
							Poll::Pending => {},
							Poll::Ready(Some(mut packet)) => {
								// set kind::Stream and set the id
								let flags = Flags::new(Kind::Stream);
								packet.header_mut().set_flags(flags);
								packet.header_mut().set_id(*id);

								return Poll::Ready((packet, None))
							},
							Poll::Ready(None) => {
								// channel closed

								let mut p = P::empty();
								let flags = Flags::new(Kind::StreamClosed);
								p.header_mut().set_flags(flags);
								p.header_mut().set_id(*id);

								return Poll::Ready((p, Some(*id)))
							}
						}
					}
				}
			}

			Poll::Pending
		}).await;

		if let Some(rem) = rem {
			self.inner.remove(&rem);
		}

		Some(packet)
	}

	pub fn close_all(&mut self) {
		for (_, resp) in &mut self.inner {
			match resp {
				Response::Request(resp) => resp.close(),
				Response::Receiver(resp) => resp.close()
			}
		}
	}

	pub fn close(&mut self, id: u32) {
		match self.inner.get_mut(&id) {
			Some(Response::Request(resp)) => resp.close(),
			Some(Response::Receiver(resp)) => resp.close(),
			_ => {}
		}
	}
}

/// A handler that is responsible for the server.
pub struct Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	/// messages that should be sent to the server.
	msg_to_server: mpsc::Sender<Message<P>>,
	/// messages that are waiting on a packet from the client
	waiting_on_client: HashMap<u32, mpsc::Sender<P>>,
	/// messages that are waiting on a packet from the server
	waiting_on_server: WaitingOnServer<P, B>
}

impl<P, B> Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	/// Creates a new handler, return a receiver which can listens on new
	/// messages.
	pub(crate) fn new(
		cfg: Config
	) -> (Receiver<P>, watch::Receiver<Config>, Self) {
		let (tx, rx) = mpsc::channel(10);
		let (cfg_tx, cfg_rx) = watch::channel(cfg);

		(
			Receiver {
				inner: rx,
				cfg: cfg_tx
			},
			cfg_rx,
			Self {
				msg_to_server: tx,
				waiting_on_client: HashMap::new(),
				waiting_on_server: WaitingOnServer::new()
			}
		)
	}

	pub(crate) fn ping_packet(&self) -> P {
		let mut p = P::empty();
		let flags = Flags::new(Kind::Ping);
		p.header_mut().set_flags(flags);
		p
	}

	fn stream_close_packet(&self, id: u32) -> P {
		let mut p = P::empty();
		let flags = Flags::new(Kind::StreamClosed);
		p.header_mut().set_flags(flags);
		p.header_mut().set_id(id);
		p
	}

	/// Should be called with a packet from the client.
	pub(crate) async fn send(
		&mut self,
		packet: P
	) -> Result<SendBack<P>, TaskError> {
		let flags = packet.header().flags();
		let id = packet.header().id();
		let kind = flags.kind();

		match kind {
			Kind::Request => {
				let (tx, rx) = oneshot::channel();

				self.waiting_on_server.insert(id, Response::Request(rx))?;

				let sr = self.msg_to_server.send(Message::Request(
					packet,
					ResponseSender::new(tx)
				)).await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket)
				}
			},
			Kind::RequestReceiver => {
				let (tx, rx) = mpsc::channel(10);	
				self.waiting_on_server.insert(id, Response::Receiver(rx))?;

				let sr = self.msg_to_server.send(Message::RequestReceiver(
					packet,
					StreamSender::new(tx)
				)).await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket)
				}
			},
			Kind::RequestSender => {
				let (tx, rx) = mpsc::channel(10);

				match self.waiting_on_client.entry(id) {
					Entry::Occupied(occ) => {
						return Err(TaskError::ExistingId(*occ.key()))
					},
					Entry::Vacant(v) => {
						v.insert(tx);
					}
				}

				let sr = self.msg_to_server.send(Message::RequestSender(
					packet,
					StreamReceiver::new(rx)
				)).await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket)
				}
			},
			Kind::Stream => {
				match self.waiting_on_client.entry(id) {
					Entry::Occupied(mut occ) => {
						if let Err(_) = occ.get_mut().send(packet).await {
							// since the stream is closed we should remove it
							occ.remove_entry();
							// we should send a response telling the other side
							// that the stream is closed
							let p = self.stream_close_packet(id);
							Ok(SendBack::Packet(p))
						} else {
							Ok(SendBack::None)
						}
					},
					Entry::Vacant(_) => {
						// since the client could send multiple streams
						// before we can send him a streamclosed
						// we just ignore this packet
						let p = self.stream_close_packet(id);
						Ok(SendBack::Packet(p))
					}
				}
			},
			Kind::StreamClosed => {
				let _ = self.waiting_on_client.remove(&id);
				self.waiting_on_server.close(id);
				Ok(SendBack::None)
			},
			Kind::Close => Ok(SendBack::Close),
			Kind::Ping => Ok(SendBack::None),
			k => Err(TaskError::WrongPacketKind(k.to_str()))
		}
	}

	/// returns None if nothing is left to be done
	/// if close=true is once set this cannot be reversed 
	pub async fn to_send(&mut self) -> Option<P> {
		self.waiting_on_server.to_send().await
	}

	fn malformed_request(&self, id: u32) -> P {
		let mut p = P::empty();
		// todo maybe replace with a malformed request
		let flags = Flags::new(Kind::MalformedRequest);
		p.header_mut().set_flags(flags);
		p.header_mut().set_id(id);

		p
	}

	/// we received a packet which had a malformed body
	pub(crate) fn packet_error(
		&mut self,
		header: P::Header,
		e: PacketError
	) -> Result<SendBack<P>, TaskError> {
		let flags = header.flags();
		let id = header.id();
		let kind = flags.kind();

		match kind {
			Kind::Request => Ok(SendBack::Packet(self.malformed_request(id))),
			Kind::RequestSender |
			Kind::RequestReceiver => {
				Ok(SendBack::Packet(self.stream_close_packet(id)))
			},
			Kind::Stream => {
				// ignore a stream packet which had an error
				eprintln!(
					"failed to parse stream packet {} {:?}",
					header.id(),
					e
				);
				Ok(SendBack::None)
			},
			// this should not have a user generated so this should never fail
			Kind::Close |
			Kind::Ping |
			Kind::StreamClosed => Err(TaskError::Packet(e)),
			k => Err(TaskError::WrongPacketKind(k.to_str()))
		}
	}

	pub fn close(&mut self) -> P {
		self.waiting_on_server.close_all();

		let mut p = P::empty();
		let flags = Flags::new(Kind::Close);
		p.header_mut().set_flags(flags);

		p
	}
}