use crate::message::{Action, Message};
use crate::request::RequestHandler;

use stream::util::{SocketAddr, Listener, ListenerExt};
use stream::packet::{Packet, PacketBytes};
pub use stream::packet::PlainBytes;
use stream::server::{self, Connection};
pub use stream::server::Config;

#[cfg(feature = "encrypted")]
pub use stream::packet::EncryptedBytes;

use std::collections::HashMap;
use std::any::{Any, TypeId};
use std::sync::{Arc, Mutex};
use std::io;

#[cfg(feature = "encrypted")]
use crypto::signature::Keypair;


pub struct Data {
	inner: HashMap<TypeId, Box<dyn Any + Send + Sync>>
}

impl Data {
	fn new() -> Self {
		Self {
			inner: HashMap::new()
		}
	}

	pub fn exists<D>(&self) -> bool
	where D: Any {
		TypeId::of::<D>() == TypeId::of::<Session>() ||
		self.inner.contains_key(&TypeId::of::<D>())
	}

	fn insert<D>(&mut self, data: D)
	where D: Any + Send + Sync {
		self.inner.insert(data.type_id(), Box::new(data));
	}

	pub fn get<D>(&self) -> Option<&D>
	where D: Any {
		self.inner.get(&TypeId::of::<D>())
			.and_then(|a| a.downcast_ref())
	}

	pub fn get_or_sess<'a, D>(&'a self, sess: &'a Session) -> Option<&'a D>
	where D: Any {
		if TypeId::of::<D>() == TypeId::of::<Session>() {
			<dyn Any>::downcast_ref(sess)
		} else {
			self.get()
		}
	}
}

struct Requests<A, B> {
	inner: HashMap<A, Box<dyn RequestHandler<B, Action=A> + Send + Sync>>
}

impl<A, B> Requests<A, B>
where A: Action {

	fn new() -> Self {
		Self {
			inner: HashMap::new()
		}
	}

	fn insert<H>(&mut self, handler: H)
	where H: RequestHandler<B, Action=A> + Send + Sync + 'static {
		self.inner.insert(H::action(), Box::new(handler));
	}

	fn get(
		&self,
		action: &A
	) -> Option<&Box<dyn RequestHandler<B, Action=A> + Send + Sync>> {
		self.inner.get(action)
	}

}

pub struct Server<A, B, L, More> {
	inner: L,
	requests: Requests<A, B>,
	data: Data,
	cfg: Config,
	more: More
}

impl<A, B, L, More> Server<A, B, L, More>
where
	A: Action,
	L: Listener
{
	pub fn register_request<H>(&mut self, handler: H)
	where H: RequestHandler<B, Action=A> + Send + Sync + 'static {
		handler.validate_data(&self.data);
		self.requests.insert(handler);
	}

	pub fn register_data<D>(&mut self, data: D)
	where D: Any + Send + Sync {
		self.data.insert(data);
	}

	async fn run_raw<F>(self, new_connection: F) -> io::Result<()>
	where
		A: Action + Send + Sync + 'static,
		B: PacketBytes + Send + 'static,
		F: Fn(&More, L::Stream) -> Connection<Message<A, B>>
	{
		let s = Arc::new(Shared {
			requests: self.requests,
			data: self.data
		});

		loop {

			// should we fail here??
			let (stream, addr) = self.inner.accept().await?;

			let mut con = new_connection(&self.more, stream);
			let session = Arc::new(Session::new(addr));
			session.set(con.configurator());

			let share = s.clone();
			tokio::spawn(async move {
				while let Some(req) = con.receive().await {
					// todo replace with let else
					let (msg, resp) = match req {
						server::Message::Request(msg, resp) => (msg, resp),
						// ignore streams for now
						_ => continue
					};

					let share = share.clone();
					let session = session.clone();

					let action = match msg.action() {
						Some(act) => *act,
						// todo once we bump the version again
						// we need to pass our own errors via packets
						// not only those from the api users
						None => {
							eprintln!("invalid action received");
							continue
						}
					};

					tokio::spawn(async move {
						let handler = match share.requests.get(&action) {
							Some(handler) => handler,
							// todo once we bump the version again
							// we need to pass our own errors via packets
							// not only those from the api users
							None => {
								eprintln!("no handler for {:?}", action);
								return;
							}
						};
						let r = handler.handle(
							msg,
							&share.data,
							&session
						).await;

						match r {
							Ok(mut msg) => {
								msg.header_mut().set_action(action);
								// i don't care about the response
								let _ = resp.send(msg);
							},
							Err(e) => {
								// todo once we bump the version again
								// we need to pass our own errors via packets
								// not only those from the api users
								eprintln!("handler returned an error {:?}", e);
							}
						}
					});
				}
			});
		}
	}

}

pub struct Session {
	// (SocketAddr, S)
	addr: SocketAddr,
	data: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>
}

impl Session {
	pub fn new(addr: SocketAddr) -> Self {
		Self {
			addr,
			data: Mutex::new(HashMap::new())
		}
	}

	pub fn addr(&self) -> &SocketAddr {
		&self.addr
	}

	pub fn set<D>(&self, data: D)
	where D: Any + Send + Sync {
		self.data.lock().unwrap()
			.insert(data.type_id(), Box::new(data));
	}

	pub fn get<D>(&self) -> Option<D>
	where D: Any + Clone + Send + Sync {
		self.data.lock().unwrap()
			.get(&TypeId::of::<D>())
			.and_then(|d| d.downcast_ref())
			.map(Clone::clone)
	}

	pub fn take<D>(&self) -> Option<D>
	where D: Any + Send + Sync {
		self.data.lock().unwrap()
			.remove(&TypeId::of::<D>())
			.and_then(|d| d.downcast().ok())
			.map(|b| *b)
	}
}

impl<A, L> Server<A, PlainBytes, L, ()>
where
	A: Action,
	L: Listener
{	
	pub fn new(listener: L, cfg: Config) -> Self {
		Self {
			inner: listener,
			requests: Requests::new(),
			data: Data::new(),
			cfg,
			more: ()
		}
	}
	
	pub async fn run(self) -> io::Result<()>
	where A: Send + Sync + 'static {
		let cfg = self.cfg.clone();
		self.run_raw(|_, stream| {
			Connection::new(stream, cfg.clone())
		}).await
	}

}

#[cfg(feature = "encrypted")]
impl<A, L> Server<A, EncryptedBytes, L, Keypair>
where
	A: Action,
	L: Listener
{
	pub fn new_encrypted(listener: L, cfg: Config, key: Keypair) -> Self {
		Self {
			inner: listener,
			requests: Requests::new(),
			data: Data::new(),
			cfg,
			more: key
		}
	}

	pub async fn run(self) -> io::Result<()>
	where A: Send + Sync + 'static {
		let cfg = self.cfg.clone();
		self.run_raw(move |key, stream| {
			Connection::new_encrypted(stream, cfg.clone(), key.clone())
		}).await
	}
}

// impl

struct Shared<A, B> {
	requests: Requests<A, B>,
	data: Data
}


#[cfg(all(test, feature = "json"))]
mod json_tests {
	use codegen::{IntoMessage, FromMessage, api};
	use crate::request::Request;
	use crate::message;
	use crate::error;

	use std::fmt;

	use serde::{Serialize, Deserialize};


	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestReq {
		hello: u64
	}

	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestReq2 {
		hello: u64
	}

	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestResp {
		hi: u64
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	pub enum Action {
		Empty
	}

	#[derive(Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	pub enum Error {
		RequestError(String),
		MessageError(String)
	}

	impl fmt::Display for Error {
		fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
			fmt::Debug::fmt(self, fmt)
		}
	}

	impl std::error::Error for Error {}

	impl error::ApiError for Error {
		fn from_request_error(e: error::RequestError) -> Self {
			Self::RequestError(e.to_string())
		}

		fn from_message_error(e: error::MessageError) -> Self {
			Self::MessageError(e.to_string())
		}
	}

	impl message::Action for Action {
		fn from_u16(_num: u16) -> Option<Self> { todo!() }
		fn as_u16(&self) -> u16 { todo!() }
	}

	impl Request for TestReq {
		type Action = Action;
		type Response = TestResp;
		type Error = Error;

		const ACTION: Action = Action::Empty;
	}

	impl Request for TestReq2 {
		type Action = Action;
		type Response = TestResp;
		type Error = Error;

		const ACTION: Action = Action::Empty;
	}

	#[api(TestReq)]
	async fn test(req: TestReq) -> Result<TestResp, Error> {
		println!("req {:?}", req);
		Ok(TestResp {
			hi: req.hello
		})
	}

	#[api(TestReq2)]
	async fn test_2(
		req: TestReq2
	) -> Result<TestResp, Error> {
		println!("req {:?}", req);
		Ok(TestResp {
			hi: req.hello
		})
	}
}


#[cfg(all(test, feature = "protobuf"))]
mod protobuf_tests {
	use codegen::{IntoMessage, FromMessage};

	use fire_protobuf::{EncodeMessage, DecodeMessage};


	#[derive(Debug, Default)]
	#[derive(EncodeMessage, DecodeMessage, IntoMessage, FromMessage)]
	#[message(protobuf)]
	struct TestReq {
		#[field(1)]
		hello: u64
	}

	#[derive(Debug, Default)]
	#[derive(EncodeMessage, DecodeMessage, IntoMessage, FromMessage)]
	#[message(protobuf)]
	struct TestReq2 {
		#[field(1)]
		hello: u64
	}
}