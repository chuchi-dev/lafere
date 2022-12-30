
use super::{
	SendBack, Message, StreamSender, Stream, ResponseSender, Configurator
};
use crate::Result;
use crate::watch;
use crate::packet::{Packet, Kind, Flags, PacketHeader, PacketBytes, PacketError};
use crate::poll_fn::poll_fn;
use crate::StreamError::*;
use crate::client::Config;

use std::collections::HashMap;
use std::task::Poll;
use std::marker::PhantomData;

use tokio::sync::{mpsc, oneshot};

/// A sender that sends messages to the handler.
pub struct Sender<P> {
	inner: mpsc::Sender<Message<P>>,
	cfg: watch::Sender<Config>
}

impl<P> Sender<P> {

	/// Send a request waiting until a response is available.
	pub async fn request(&self, packet: P) -> Result<P> {
		let (tx, rx) = oneshot::channel();
		self.inner.send(Message::Request(
			packet,
			ResponseSender::new(tx)
		)).await.map_err(|_| ConnectionClosed)?;

		rx.await.map_err(|_| RequestDropped)
	}

	/// Opens a new stream to listen to packets.
	pub async fn open_stream(&self, packet: P) -> Result<Stream<P>> {
		let (tx, rx) = mpsc::channel(10);
		self.inner.send(Message::OpenStream(
			packet,
			StreamSender::new(tx)
		)).await.map_err(|_| ConnectionClosed)?;

		Ok(Stream::new(rx))
	}

	/// Create a new stream to send packets.
	pub async fn create_stream(&self, packet: P) -> Result<StreamSender<P>> {
		let (tx, rx) = mpsc::channel(10);
		self.inner.send(Message::CreateStream(
			packet,
			Stream::new(rx)
		)).await.map_err(|_| ConnectionClosed)?;

		Ok(StreamSender::new(tx))
	}

	pub fn update_config(&self, cfg: Config) {
		self.cfg.send(cfg);
	}

	pub fn configurator(&self) -> Configurator<Config> {
		Configurator::new(self.cfg.clone())
	}

}

impl<P> Clone for Sender<P> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			cfg: self.cfg.clone()
		}
	}
}

/// A response that is managed by the handler
enum Response<P> {
	Request(oneshot::Sender<P>),
	// a request to receive a receiving stream
	Receiver(mpsc::Sender<P>)
}

/// A list of all senders that wait on a packet.
struct WaitingOnClient<P, B> {
	// hashmap because we need to check if the id is free
	inner: HashMap<u32, mpsc::Receiver<P>>,
	marker: PhantomData<B>
}


impl<P, B> WaitingOnClient<P, B>
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

	fn insert(&mut self, id: u32, receiver: mpsc::Receiver<P>) {
		self.inner.insert(id, receiver);
	}

	pub async fn to_send(&mut self) -> Option<P> {

		if self.inner.is_empty() {
			return None
		}

		let (packet, rem) = poll_fn(|ctx| {

			for (id, resp) in &mut self.inner {

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

			Poll::Pending
		}).await;

		if let Some(rem) = rem {
			self.inner.remove(&rem);
		}

		Some(packet)
	}

	pub fn close_all(&mut self) {
		for (_, resp) in &mut self.inner {
			resp.close();
		}
	}

	pub fn close(&mut self, id: u32) {
		if let Some(s) = self.inner.get_mut(&id) {
			s.close();
		}
	}

}

/// A handler that is responsible for the client.
pub struct Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	// messages that are received from the client
	msg_from_client: mpsc::Receiver<Message<P>>,
	// messages that are waiting on a packet from the server
	waiting_on_server: HashMap<u32, Response<P>>,
	// messages that are waiting on a packet from the client
	waiting_on_client: WaitingOnClient<P, B>,
	// since we are "the client" we give every message a new id (or packet?)
	counter: u32
}

impl<P, B> Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes
{
	/// Creates a new handler, returning a Sender which can communicate with
	/// this handler.
	pub(crate) fn new(
		cfg: Config
	) -> (Sender<P>, watch::Receiver<Config>, Self) {
		let (tx, rx) = mpsc::channel(10);
		let (cfg_tx, cfg_rx) = watch::channel(cfg);

		(
			Sender {
				inner: tx,
				cfg: cfg_tx
			},
			cfg_rx,
			Self {
				msg_from_client: rx,
				waiting_on_server: HashMap::new(),
				waiting_on_client: WaitingOnClient::new(),
				counter: 0
			}
		)
	}

	/// returns a new id.
	/// 
	/// If the counter is full we sent u32::max messages None is returned
	fn next_id(&mut self) -> Option<u32> {
		self.counter = self.counter.checked_add(1)?;
		Some(self.counter)
	}

	/// Creates a new ping packet
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

	/// Should be called with a packet from the server
	pub(crate) async fn send(
		&mut self,
		packet: P
	) -> Result<SendBack<P>> {
		let flags = packet.header().flags();
		let id = packet.header().id();
		let kind = flags.kind();

		// println!("received {:?} {:?}", kind, packet);

		match kind {
			Kind::Response => {
				match self.waiting_on_server.remove(&id) {
					Some(Response::Request(r)) => {
						// we don't care if the response could be sent or not
						let _ = r.send(packet);
						Ok(SendBack::None)
					},
					_ => Err(
						PacketError::Header("Handler not found".into()).into()
					)
				}
			},
			Kind::NoResponse => {
				self.waiting_on_server.remove(&id);
				Ok(SendBack::None)
			},
			Kind::Stream => {
				match self.waiting_on_server.get_mut(&id) {
					Some(Response::Receiver(sender)) => {
						if let Err(_) = sender.send(packet).await {
							// we should send a response telling the other side
							// that the response closed
							let p = self.stream_close_packet(id);
							Ok(SendBack::Packet(p))
						} else {
							Ok(SendBack::None)
						}
					},
					Some(_) => Err(
						PacketError::Header("Handler not found".into()).into()
					),
					None => {
						// since the server could send multiple streams
						// before we can send him a stream closed
						// we just respond to every stream message
						let p = self.stream_close_packet(id);
						Ok(SendBack::Packet(p))
					}
				}
			},
			Kind::StreamClosed => {
				let _ = self.waiting_on_server.remove(&id);
				self.waiting_on_client.close(id);
				Ok(SendBack::None)
			},
			Kind::Close => Ok(SendBack::Close),
			Kind::Ping => Ok(SendBack::None),
			k => Err(
				PacketError::Header(format!("{:?} not supported", k)).into()
			)
		}
	}

	/// Returns a packet to send to the server.
	/// If None is returned there is nothing more to send.
	/// 
	/// Todo: comment still necessary?
	/// if close=true is once set this cannot be reversed
	pub async fn to_send(&mut self) -> Option<P> {

		tokio::select!{
			Some(packet) = self.waiting_on_client.to_send() => Some(packet),
			Some(req) = self.msg_from_client.recv() => {
				// todo should this return an error
				// since we will never be able to return another
				// id
				let id = self.next_id()?;

				let (kind, mut packet) = match req {
					Message::Request(packet, sender) => {
						self.waiting_on_server.insert(
							id,
							Response::Request(sender.inner)
						);

						(Kind::Request, packet)
					},
					Message::OpenStream(packet, sender) => {
						self.waiting_on_server.insert(
							id,
							Response::Receiver(sender.inner)
						);

						(Kind::NewStream, packet)
					},
					Message::CreateStream(packet, receiver) => {
						self.waiting_on_client.insert(id, receiver.inner);

						(Kind::NewSenderStream, packet)
					}
				};

				let flags = Flags::new(kind);
				packet.header_mut().set_flags(flags);
				packet.header_mut().set_id(id);
				Some(packet)
			},
			else => {
				None
			}
		}
	}

	/// Closes all channel.
	pub fn close(&mut self) -> P {
		self.msg_from_client.close();
		self.waiting_on_client.close_all();

		let mut p = P::empty();
		let flags = Flags::new(Kind::Close);
		p.header_mut().set_flags(flags);

		p
	}

	/// close all started requests
	/// 
	/// this should be called when a connection get's reset
	pub fn close_all_started(&mut self) {
		self.waiting_on_server.clear();
		self.waiting_on_client.close_all();
		// reset counter
		self.counter = 0;
	}

}

