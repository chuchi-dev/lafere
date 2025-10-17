use std::{
	collections::{HashMap, hash_map::Entry},
	io,
	marker::PhantomData,
	pin::Pin,
	task::Poll,
	time::Duration,
};

use tokio::{
	sync::{mpsc, oneshot},
	time::{MissedTickBehavior, interval},
};

use crate::{
	client::ReconStrat,
	error::{RequestError, ResponseError, TaskError},
	handler::{Receiver, SendBack, Sender, StreamReceiver, StreamSender},
	packet::{
		Flags, Kind, Packet, PacketBytes, PacketError, PacketHeader,
		builder::PacketReceiverError,
	},
	util::{poll_fn, watch},
};

/// A message containing the different kinds of messages.
#[derive(Debug)]
pub(crate) enum InternalRequest<P> {
	EnableServerRequests,
	Request(P, oneshot::Sender<Result<P, RequestError>>),
	// a request to receive a sender stream
	RequestSender(P, mpsc::Receiver<P>),
	// a request to receive a receiving stream
	RequestReceiver(P, mpsc::Sender<P>),
}

/// All different kinds of messages.
#[derive(Debug)]
pub enum Request<P> {
	EnableServerRequests,
	Request(P, ResponseSender<P>),
	// a request to receive a sender stream
	RequestSender(P, StreamReceiver<P>),
	// a request to receive a receiving stream
	RequestReceiver(P, StreamSender<P>),
}

/// A sender used to respond to a request.
#[derive(Debug)]
pub struct ResponseSender<P> {
	pub(crate) inner: oneshot::Sender<P>,
}

impl<P> ResponseSender<P> {
	pub(crate) fn new(inner: oneshot::Sender<P>) -> Self {
		Self { inner }
	}

	/// Sends the packet as a response, adding the correct flags.
	///
	/// If this returns an Error it either means the connection was closed
	/// or the requestor does not care about the response anymore.
	pub fn send(self, packet: P) -> Result<(), ResponseError> {
		self.inner
			.send(packet)
			.map_err(|_| ResponseError::ConnectionClosed)
	}
}

pub enum UserRespone<P> {
	Request(oneshot::Receiver<P>),
	// a request to receive a receiving stream
	Receiver(mpsc::Receiver<P>),
}

/// A list of receivers that wait on a packet.
struct WaitingOnUser<P, B> {
	// hashmap because we need to check if the id is free
	inner: HashMap<u32, UserRespone<P>>,
	marker: PhantomData<B>,
}

impl<P, B> WaitingOnUser<P, B>
where
	P: Packet<B>,
	B: PacketBytes,
{
	fn new() -> Self {
		Self {
			inner: HashMap::new(),
			marker: PhantomData,
		}
	}

	fn insert(
		&mut self,
		id: u32,
		receiver: UserRespone<P>,
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
			return None;
		}

		let (packet, rem) = poll_fn(|ctx| {
			for (id, resp) in &mut self.inner {
				match resp {
					UserRespone::Request(resp) => {
						match Pin::new(resp).poll(ctx) {
							Poll::Pending => {}
							Poll::Ready(Ok(mut packet)) => {
								// set kind::Stream and set the id
								let flags = Flags::new(Kind::Response);
								packet.header_mut().set_flags(flags);
								packet.header_mut().set_id(*id);

								return Poll::Ready((packet, Some(*id)));
							}
							Poll::Ready(Err(_)) => {
								// channel closed
								let mut p = P::empty();
								let flags = Flags::new(Kind::NoResponse);
								p.header_mut().set_flags(flags);
								p.header_mut().set_id(*id);

								return Poll::Ready((p, Some(*id)));
							}
						}
					}
					UserRespone::Receiver(resp) => {
						match resp.poll_recv(ctx) {
							Poll::Pending => {}
							Poll::Ready(Some(mut packet)) => {
								// set kind::Stream and set the id
								let flags = Flags::new(Kind::Stream);
								packet.header_mut().set_flags(flags);
								packet.header_mut().set_id(*id);

								return Poll::Ready((packet, None));
							}
							Poll::Ready(None) => {
								// channel closed

								let mut p = P::empty();
								let flags = Flags::new(Kind::StreamClosed);
								p.header_mut().set_flags(flags);
								p.header_mut().set_id(*id);

								return Poll::Ready((p, Some(*id)));
							}
						}
					}
				}
			}

			Poll::Pending
		})
		.await;

		if let Some(rem) = rem {
			self.inner.remove(&rem);
		}

		Some(packet)
	}

	pub fn close_all(&mut self) {
		for resp in self.inner.values_mut() {
			match resp {
				UserRespone::Request(resp) => resp.close(),
				UserRespone::Receiver(resp) => resp.close(),
			}
		}
	}

	pub fn close(&mut self, id: u32) {
		match self.inner.get_mut(&id) {
			Some(UserRespone::Request(resp)) => resp.close(),
			Some(UserRespone::Receiver(resp)) => resp.close(),
			_ => {}
		}
	}
}

enum StreamResponse<P> {
	Request(oneshot::Sender<Result<P, RequestError>>),
	Receiver(mpsc::Sender<P>),
}

// this is the maximum client id inclusive
const U32_MID: u32 = u32::MAX / 2;

enum HandlerState {
	UniServer,
	// 0..=u32::max
	UniClient(Option<u32>),
	// (u32::mid + 1)..u32::max
	BiServer(Option<u32>),
	// 0..=u32::mid
	BiClient(Option<u32>),
}

impl HandlerState {
	fn next_id(&mut self) -> Option<u32> {
		let id;
		match self {
			HandlerState::UniServer => panic!("cannot create id on uni Server"),
			HandlerState::UniClient(counter) => {
				id = *counter;
				*counter = counter.and_then(|c| c.checked_add(1));
			}
			HandlerState::BiServer(counter) => {
				id = *counter;
				debug_assert!(
					counter.is_none()
						|| matches!(*counter, Some(c) if c > U32_MID)
				);
				*counter = counter.and_then(|c| c.checked_add(1));
			}
			HandlerState::BiClient(counter) => {
				id = *counter;

				match counter {
					Some(c) if *c < U32_MID => *c += 1,
					_ => *counter = None,
				}
			}
		}

		id
	}

	fn can_receive_requests(&self) -> bool {
		!matches!(self, HandlerState::UniClient(_))
	}

	fn can_send_requests(&self) -> bool {
		!matches!(self, HandlerState::UniServer)
	}

	fn reset(&mut self) {
		match self {
			HandlerState::UniServer => {}
			HandlerState::UniClient(counter) => *counter = Some(0),
			HandlerState::BiServer(counter) => *counter = Some(U32_MID + 1),
			HandlerState::BiClient(counter) => *counter = Some(0),
		}
	}

	/// Returns true if the state was changed (meaning it was previous not enabled)
	fn enable_server_requests(&mut self) -> bool {
		match self {
			HandlerState::UniServer => {
				*self = HandlerState::BiServer(Some(U32_MID + 1));
			}
			HandlerState::UniClient(Some(id)) => {
				if *id <= U32_MID {
					*self = HandlerState::BiClient(Some(*id));
				} else {
					*self = HandlerState::BiClient(None);
				}
			}
			HandlerState::UniClient(None) => {
				*self = HandlerState::BiClient(None);
			}
			_ => return false,
		}

		true
	}
}

/// A handler that is responsible for the client.
pub struct Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes,
{
	rx_requests: mpsc::Receiver<InternalRequest<P>>,
	tx_requests: mpsc::Sender<Request<P>>,
	waiting_on_stream: HashMap<u32, StreamResponse<P>>,
	waiting_on_user: WaitingOnUser<P, B>,
	state: HandlerState,
}

impl<P, B> Handler<P, B>
where
	P: Packet<B>,
	B: PacketBytes,
{
	pub fn new(is_server: bool) -> (Sender<P>, Receiver<P>, Self) {
		let (tx1, rx1) = mpsc::channel(10);
		let (tx2, rx2) = mpsc::channel(10);

		(
			Sender { inner: tx1 },
			Receiver { inner: rx2 },
			Self {
				rx_requests: rx1,
				tx_requests: tx2,
				waiting_on_stream: HashMap::new(),
				waiting_on_user: WaitingOnUser::new(),
				state: if is_server {
					HandlerState::UniServer
				} else {
					HandlerState::UniClient(Some(0))
				},
			},
		)
	}

	/// returns a new id.
	///
	/// If the counter is full None is returned
	fn next_id(&mut self) -> Option<u32> {
		self.state.next_id()
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

	fn malformed_request(&self, id: u32) -> P {
		let mut p = P::empty();
		let flags = Flags::new(Kind::MalformedRequest);
		p.header_mut().set_flags(flags);
		p.header_mut().set_id(id);

		p
	}

	/// Should be called with a packet from the otherside
	pub(crate) async fn send(
		&mut self,
		packet: P,
	) -> Result<SendBack<P>, TaskError> {
		let flags = packet.header().flags();
		let id = packet.header().id();
		let kind = flags.kind();

		// todo we need to validate ids here if it is a bidirectional
		// connection
		// the issue is if the client enables server requests (do we check
		// all our existing ids?)

		let can_recv_req = self.state.can_receive_requests();

		match kind {
			Kind::EnableServerRequests => {
				let newly_enabled = self.state.enable_server_requests();
				if newly_enabled {
					let sr = self
						.tx_requests
						.send(Request::EnableServerRequests)
						.await;

					match sr {
						Ok(_) => Ok(SendBack::None),
						// the server has no interest anymore
						// Let's close the connection
						Err(_) => Ok(SendBack::CloseWithPacket),
					}
				} else {
					Ok(SendBack::None)
				}
			}
			Kind::Request if can_recv_req => {
				let (tx, rx) = oneshot::channel();
				self.waiting_on_user.insert(id, UserRespone::Request(rx))?;

				let sr = self
					.tx_requests
					.send(Request::Request(packet, ResponseSender::new(tx)))
					.await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket),
				}
			}
			Kind::RequestReceiver if can_recv_req => {
				let (tx, rx) = mpsc::channel(10);
				self.waiting_on_user.insert(id, UserRespone::Receiver(rx))?;

				let sr = self
					.tx_requests
					.send(Request::RequestReceiver(
						packet,
						StreamSender::new(tx),
					))
					.await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket),
				}
			}
			Kind::RequestSender if can_recv_req => {
				let (tx, rx) = mpsc::channel(10);

				match self.waiting_on_stream.entry(id) {
					Entry::Occupied(occ) => {
						return Err(TaskError::ExistingId(*occ.key()));
					}
					Entry::Vacant(v) => {
						v.insert(StreamResponse::Receiver(tx));
					}
				}

				let sr = self
					.tx_requests
					.send(Request::RequestSender(
						packet,
						StreamReceiver::new(rx),
					))
					.await;

				match sr {
					Ok(_) => Ok(SendBack::None),
					// the server has no interest anymore
					// Let's close the connection
					Err(_) => Ok(SendBack::CloseWithPacket),
				}
			}
			Kind::Response => {
				match self.waiting_on_stream.remove(&id) {
					Some(StreamResponse::Request(r)) => {
						// we don't care if the response could be sent or not
						let _ = r.send(Ok(packet));
						Ok(SendBack::None)
					}
					_ => Err(TaskError::UnknownId(id)),
				}
			}
			Kind::NoResponse | Kind::MalformedRequest => {
				let e = match kind {
					Kind::NoResponse => RequestError::NoResponse,
					Kind::MalformedRequest => RequestError::MalformedRequest,
					_ => unreachable!(),
				};

				match self.waiting_on_stream.remove(&id) {
					Some(StreamResponse::Request(r)) => {
						let _ = r.send(Err(e));
						Ok(SendBack::None)
					}
					_ => Err(TaskError::UnknownId(id)),
				}
			}
			Kind::Stream => {
				match self.waiting_on_stream.get_mut(&id) {
					Some(StreamResponse::Receiver(sender)) => {
						if let Err(_) = sender.send(packet).await {
							// we should send a response telling the other side
							// that the response closed
							let p = self.stream_close_packet(id);
							Ok(SendBack::Packet(p))
						} else {
							Ok(SendBack::None)
						}
					}
					Some(_) => Err(TaskError::UnknownId(id)),
					None => {
						// since the server could send multiple streams
						// before we can send him a stream closed
						// we just respond to every stream message
						let p = self.stream_close_packet(id);
						Ok(SendBack::Packet(p))
					}
				}
			}
			Kind::StreamClosed => {
				// we might receive multiple StreamClosed because we or the
				// server where slow, so just ignore the packet
				let _ = self.waiting_on_stream.remove(&id);
				self.waiting_on_user.close(id);
				Ok(SendBack::None)
			}
			Kind::Close => Ok(SendBack::Close),
			Kind::Ping => Ok(SendBack::None),
			k => Err(TaskError::WrongPacketKind(k.to_str())),
		}
	}

	/// Returns a packet to send to the otherside.
	/// If None is returned there is nothing more to send.
	pub async fn to_send(&mut self) -> Option<P> {
		let can_send_req = self.state.can_send_requests();
		tokio::select! {
			Some(packet) = self.waiting_on_user.to_send() => Some(packet),
			Some(req) = self.rx_requests.recv(), if can_send_req => {

				// todo should this return an error
				// since we will never be able to return another
				// id
				let id = self.next_id()?;

				let (kind, mut packet) = match req {
					InternalRequest::EnableServerRequests => {
						self.state.enable_server_requests();
						(Kind::EnableServerRequests, P::empty())
					},
					InternalRequest::Request(packet, sender) => {
						let existing = self.waiting_on_stream.insert(
							id,
							StreamResponse::Request(sender)
						);
						assert!(existing.is_none(), "generated a duplicate id");

						(Kind::Request, packet)
					},
					InternalRequest::RequestSender(packet, receiver) => {
						self.waiting_on_user.insert(id, UserRespone::Receiver(receiver))
							.expect("generated a duplicate id");

						(Kind::RequestSender, packet)
					},
					InternalRequest::RequestReceiver(packet, sender) => {
						let existing = self.waiting_on_stream.insert(
							id,
							StreamResponse::Receiver(sender)
						);
						assert!(existing.is_none(), "generated a duplicate id");

						(Kind::RequestReceiver, packet)
					}
				};

				let flags = Flags::new(kind);
				packet.header_mut().set_flags(flags);
				packet.header_mut().set_id(id);
				Some(packet)
			},
			else => None
		}
	}

	/// we received a packet which had a malformed body
	pub(crate) fn packet_error(
		&mut self,
		header: P::Header,
		e: PacketError,
	) -> Result<SendBack<P>, TaskError> {
		let flags = header.flags();
		let id = header.id();
		let kind = flags.kind();

		match kind {
			Kind::Request => Ok(SendBack::Packet(self.malformed_request(id))),
			Kind::RequestSender | Kind::RequestReceiver => {
				Ok(SendBack::Packet(self.stream_close_packet(id)))
			}
			Kind::Response => match self.waiting_on_stream.remove(&id) {
				Some(StreamResponse::Request(r)) => {
					let _ = r.send(Err(RequestError::ResponsePacket(e)));
					Ok(SendBack::None)
				}
				_ => Err(TaskError::UnknownId(id)),
			},
			// this should not have a user generated so this should never fail
			Kind::NoResponse
			| Kind::MalformedRequest
			| Kind::Close
			| Kind::Ping
			| Kind::StreamClosed => Err(TaskError::Packet(e)),
			Kind::Stream => {
				// ignore a stream packet which had an error
				tracing::error!(
					"failed to parse stream packet {} {:?}",
					header.id(),
					e
				);
				Ok(SendBack::None)
			}
			k => Err(TaskError::WrongPacketKind(k.to_str())),
		}
	}

	/// Closes all channel.
	pub fn close(&mut self) -> P {
		self.waiting_on_stream.clear();
		self.waiting_on_user.close_all();
		self.rx_requests.close();

		let mut p = P::empty();
		let flags = Flags::new(Kind::Close);
		p.header_mut().set_flags(flags);

		p
	}

	/// close all started requests
	///
	/// this should be called when a connection get's reset
	pub fn close_all_started(&mut self) {
		self.waiting_on_stream.clear();
		self.waiting_on_user.close_all();
		self.state.reset();
	}
}

pub trait PacketStream<P, B>
where
	B: PacketBytes,
	P: Packet<B>,
{
	fn timeout(&self) -> Duration;

	async fn send(&mut self, packet: P) -> io::Result<()>;

	async fn receive(&mut self) -> Result<P, PacketReceiverError<P::Header>>;

	async fn shutdown(&mut self) -> io::Result<()>;
}

pub async fn bg_stream<PS, P, B, C>(
	mut stream: PS,
	handler: &mut Handler<P, B>,
	cfg_rx: &mut watch::Receiver<C>,
	mut close: &mut oneshot::Receiver<()>,
	update_config: impl Fn(&mut PS, C),
) -> Result<(), TaskError>
where
	PS: PacketStream<P, B>,
	P: Packet<B>,
	B: PacketBytes,
	C: Clone,
{
	let mut should_close = false;
	let mut close_packet = None;

	let timeout = stream.timeout();
	let diff = match timeout.as_secs() {
		0..=1 => 0,
		0..=10 => 1,
		_ => 5,
	};
	let mut interval = interval(timeout - Duration::from_secs(diff));
	interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

	loop {
		tokio::select! {
			packet = stream.receive(), if !should_close => {
				let r_packet = match packet {
					Ok(p) => {
						handler.send(p).await?
					},
					Err(PacketReceiverError::Io(e)) => {
						return Err(TaskError::Io(e))
					},
					Err(PacketReceiverError::Hard(e)) => {
						return Err(TaskError::Packet(e))
					},
					Err(PacketReceiverError::Soft(h, e)) => {
						handler.packet_error(h, e)?
					}
				};

				match r_packet {
					SendBack::None => {},
					SendBack::Packet(p) => {
						stream.send(p).await
							.map_err(TaskError::Io)?;
					},
					SendBack::Close => {
						should_close = true;
						let _ = handler.close();
					},
					SendBack::CloseWithPacket => {
						should_close = true;
						let packet = handler.close();
						close_packet = Some(packet);
					}
				}
			},
			Some(packet) = handler.to_send() => {
				// Todo make this not block until everything is sent
				// this can stop receiving
				stream.send(packet).await
					.map_err(TaskError::Io)?;
			},
			_ping = interval.tick(), if !should_close => {
				stream.send(handler.ping_packet()).await
					.map_err(TaskError::Io)?;
			},
			_ = &mut close, if !should_close => {
				should_close = true;
				let packet = handler.close();
				close_packet = Some(packet);
			},
			Some(cfg) = cfg_rx.recv(), if !should_close => {
				update_config(&mut stream, cfg);
			},
			else => {
				if let Some(packet) = close_packet.take() {
					if let Err(e) = stream.send(packet).await {
						tracing::error!(
							"error sending close packet {:?}", e
						);
					}
				}
				if let Err(e) = stream.shutdown().await {
					tracing::error!("error shutting down {:?}", e);
				}

				return Ok(())
			}
		}
	}
}

pub async fn bg_stream_reconnect<PS, S, P, B, C, NS, NSF>(
	stream: S,
	handler: &mut Handler<P, B>,
	cfg_rx: &mut watch::Receiver<C>,
	close: &mut oneshot::Receiver<()>,
	update_config: impl Fn(&mut PS, C),
	mut recon_strat: Option<ReconStrat<S>>,
	new_stream: NS,
) -> Result<(), TaskError>
where
	PS: PacketStream<P, B>,
	P: Packet<B>,
	B: PacketBytes,
	C: Clone,
	NS: Fn(S, C) -> NSF,
	NSF: Future<Output = Result<PS, TaskError>>,
{
	let mut stream = Some(stream);

	loop {
		// reconnect if
		let stream = match (stream.take(), &mut recon_strat) {
			(Some(s), _) => s,
			// no recon and no stream
			// this is not possible since on the first iteration a stream
			// always exists and if the stream failes and there
			// is no recon strategy we return
			(None, None) => unreachable!(),
			(None, Some(recon)) => {
				let mut err_counter = 0;

				loop {
					let stream = (recon.inner)(err_counter).await;
					match stream {
						Ok(s) => break s,
						Err(e) => {
							tracing::error!(
								"reconnect failed attempt {err_counter} {e:?}",
							);
							err_counter += 1;
						}
					}
				}
			}
		};

		let cfg = cfg_rx.newest();
		let stream = new_stream(stream, cfg).await;
		let stream = match stream {
			Ok(s) => s,
			Err(e) => {
				tracing::error!("creating packetstream failed {e:?}");
				// close since we can't reconnect
				if recon_strat.is_none() {
					return Err(e);
				}

				continue;
			}
		};

		let r = bg_stream(stream, handler, cfg_rx, close, |stream, cfg| {
			update_config(stream, cfg)
		})
		.await;

		if let Err(e) = &r {
			tracing::error!("lafere client stream ended with error {e:?}");
		}

		if recon_strat.is_none() {
			// close since we can't reconnect
			return r;
		}

		// now if the stream was ended because of us let's stop reconnecting
		if close.is_terminated() {
			return Ok(());
		}

		match r {
			Ok(o) => return Ok(o),
			Err(e) => {
				tracing::error!("lafere client stream connection failed {e:?}");
			}
		}

		// close all started requests because the connection failed
		handler.close_all_started();
	}
}
