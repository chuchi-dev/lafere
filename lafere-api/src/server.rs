use crate::error::{ApiError, RequestError};
use crate::message::{Action, FromMessage, IntoMessage, Message};
use crate::request::{EnableServerRequestsHandler, Request, RequestHandler};
pub use crate::request_handlers::Data;
use crate::request_handlers::{RequestHandlers, RequestHandlersBuilder};

pub use lafere::packet::PlainBytes;
use lafere::packet::{Packet, PacketBytes};
pub use lafere::server::Config;
use lafere::server::Connection;
use lafere::util::{Listener, ListenerExt, SocketAddr};

#[cfg(feature = "encrypted")]
pub use lafere::packet::EncryptedBytes;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

#[cfg(feature = "encrypted")]
use crypto::signature::Keypair;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ServerConfig {
	pub log_errors: bool,
}

pub struct Server<A, B, L, More> {
	inner: L,
	requests: RequestHandlersBuilder<A, B>,
	cfg: Config,
	more: More,
}

impl<A, B, L, More> Server<A, B, L, More>
where
	A: Action,
{
	pub fn register_request<H>(&mut self, handler: H)
	where
		H: RequestHandler<B, Action = A> + Send + Sync + 'static,
	{
		self.requests.register_request(handler);
	}

	pub fn register_enable_server_requests<H>(&mut self, handler: H)
	where
		H: EnableServerRequestsHandler<B, Action = A> + Send + Sync + 'static,
	{
		self.requests.register_enable_server_requests(handler);
	}

	pub fn register_data<D>(&mut self, data: D)
	where
		D: Any + Send + Sync,
	{
		self.requests.register_data(data);
	}
}

impl<A, B, L, More> Server<A, B, L, More>
where
	A: Action,
	L: Listener,
{
	/// optionally or just use run
	pub fn build(self) -> BuiltServer<A, B, L, More> {
		BuiltServer {
			inner: self.inner,
			requests: self.requests.build(),
			more: self.more,
		}
	}
}

impl<A, L> Server<A, PlainBytes, L, ()>
where
	A: Action,
	L: Listener,
{
	pub fn new(listener: L, cfg: Config) -> Self {
		Self {
			inner: listener,
			requests: RequestHandlersBuilder::new(),
			cfg,
			more: (),
		}
	}

	pub async fn run(self) -> io::Result<()>
	where
		A: Send + Sync + 'static,
	{
		let cfg = self.cfg.clone();

		self.build()
			.run_raw(|_, stream| Connection::new(stream, cfg.clone()))
			.await
	}
}

#[cfg(feature = "encrypted")]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
impl<A, L> Server<A, EncryptedBytes, L, Keypair>
where
	A: Action,
	L: Listener,
{
	pub fn new_encrypted(listener: L, cfg: Config, key: Keypair) -> Self {
		Self {
			inner: listener,
			requests: RequestHandlersBuilder::new(),
			cfg,
			more: key,
		}
	}

	pub async fn run(self) -> io::Result<()>
	where
		A: Send + Sync + 'static,
	{
		let cfg = self.cfg.clone();

		self.build()
			.run_raw(move |key, stream| {
				Connection::new_encrypted(stream, cfg.clone(), key.clone())
			})
			.await
	}
}

// impl

pub struct BuiltServer<A, B, L, More> {
	inner: L,
	requests: RequestHandlers<A, B>,
	more: More,
}

impl<A, B, L, More> BuiltServer<A, B, L, More>
where
	A: Action,
	L: Listener,
{
	pub fn get_data<D>(&self) -> Option<&D>
	where
		D: Any,
	{
		self.requests.get_data::<D>()
	}

	pub async fn request<R>(
		&self,
		r: R,
		session: &Arc<Session>,
	) -> Result<R::Response, R::Error>
	where
		R: Request<Action = A>,
		R: IntoMessage<A, B>,
		R::Response: FromMessage<A, B>,
		R::Error: FromMessage<A, B>,
		B: PacketBytes,
	{
		let mut msg = r.into_message().map_err(R::Error::from_message_error)?;
		msg.header_mut().set_action(R::ACTION);

		// handle the request
		let action = *msg.action().unwrap();

		let handler = match self.requests.get_handler(&action) {
			Some(handler) => handler,
			// todo once we bump the version again
			// we need to pass our own errors via packets
			// not only those from the api users
			None => {
				tracing::error!("no handler for {:?}", action);
				return Err(R::Error::from_request_error(
					RequestError::NoResponse,
				));
			}
		};

		let r = handler.handle(msg, self.requests.data(), session).await;

		let res = match r {
			Ok(mut msg) => {
				msg.header_mut().set_action(action);
				msg
			}
			Err(e) => {
				// todo once we bump the version again
				// we need to pass our own errors via packets
				// not only those from the api users
				tracing::error!("handler returned an error {:?}", e);

				return Err(R::Error::from_request_error(
					RequestError::NoResponse,
				));
			}
		};

		// now deserialize the response
		if res.is_success() {
			R::Response::from_message(res).map_err(R::Error::from_message_error)
		} else {
			R::Error::from_message(res)
				.map(Err)
				.map_err(R::Error::from_message_error)?
		}
	}

	async fn run_raw<F>(&mut self, new_connection: F) -> io::Result<()>
	where
		A: Action + Send + Sync + 'static,
		B: PacketBytes + Send + 'static,
		F: Fn(&More, L::Stream) -> Connection<Message<A, B>>,
	{
		loop {
			// should we fail here??
			let (stream, addr) = self.inner.accept().await?;

			let mut con = new_connection(&self.more, stream);
			let session = Arc::new(Session::new(addr));
			session.set(con.configurator());

			let requests = self.requests.clone();
			tokio::spawn(async move {
				requests
					.handle_connection(
						session,
						con.clone_sender_unchecked(),
						con.take_receiver().unwrap(),
					)
					.await;
			});
		}
	}
}

pub struct Session {
	// (SocketAddr, S)
	addr: SocketAddr,
	data: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Session {
	pub fn new(addr: SocketAddr) -> Self {
		Self {
			addr,
			data: Mutex::new(HashMap::new()),
		}
	}

	pub fn addr(&self) -> &SocketAddr {
		&self.addr
	}

	pub fn set<D>(&self, data: D)
	where
		D: Any + Send + Sync,
	{
		self.data
			.lock()
			.unwrap()
			.insert(data.type_id(), Box::new(data));
	}

	pub fn get<D>(&self) -> Option<D>
	where
		D: Any + Clone + Send + Sync,
	{
		self.data
			.lock()
			.unwrap()
			.get(&TypeId::of::<D>())
			.and_then(|d| d.downcast_ref())
			.map(Clone::clone)
	}

	pub fn take<D>(&self) -> Option<D>
	where
		D: Any + Send + Sync,
	{
		self.data
			.lock()
			.unwrap()
			.remove(&TypeId::of::<D>())
			.and_then(|d| d.downcast().ok())
			.map(|b| *b)
	}
}

#[cfg(all(test, feature = "json"))]
mod json_tests {
	use super::*;

	use crate::error;
	use crate::message;
	use crate::request::Request;
	use codegen::{FromMessage, IntoMessage, api};

	use std::fmt;

	use lafere::util::testing::PanicListener;

	use serde::{Deserialize, Serialize};

	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestReq {
		hello: u64,
	}

	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestReq2 {
		hello: u64,
	}

	#[derive(Debug, Serialize, Deserialize, IntoMessage, FromMessage)]
	#[message(json)]
	struct TestResp {
		hi: u64,
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	pub enum Action {
		Empty,
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub enum Error {
		RequestError(String),
		MessageError(String),
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
		fn from_u16(_num: u16) -> Option<Self> {
			todo!()
		}
		fn as_u16(&self) -> u16 {
			todo!()
		}
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
		Ok(TestResp { hi: req.hello })
	}

	#[api(TestReq2)]
	async fn test_2(req: TestReq2) -> Result<TestResp, Error> {
		println!("req {:?}", req);
		Ok(TestResp { hi: req.hello })
	}

	#[tokio::test]
	async fn test_direct_request() {
		let mut server = Server::new(
			PanicListener::new(),
			Config {
				timeout: std::time::Duration::from_millis(10),
				body_limit: 4096,
			},
		);

		server.register_data(String::from("global String"));

		server.register_request(test);
		server.register_request(test_2);

		let server = server.build();
		let session = Arc::new(Session::new(SocketAddr::V4(
			"127.0.0.1:8080".parse().unwrap(),
		)));

		let r = server
			.request(TestReq { hello: 100 }, &session)
			.await
			.unwrap();
		assert_eq!(r.hi, 100);

		let r = server
			.request(TestReq2 { hello: 100 }, &session)
			.await
			.unwrap();
		assert_eq!(r.hi, 100);

		assert_eq!(server.get_data::<String>().unwrap(), "global String");
	}
}

#[cfg(all(test, feature = "protobuf"))]
mod protobuf_tests {
	use codegen::{FromMessage, IntoMessage};

	use protopuffer::{DecodeMessage, EncodeMessage};

	#[derive(
		Debug, Default, EncodeMessage, DecodeMessage, IntoMessage, FromMessage,
	)]
	#[message(protobuf)]
	#[allow(dead_code)]
	struct TestReq {
		#[field(1)]
		hello: u64,
	}

	#[derive(
		Debug, Default, EncodeMessage, DecodeMessage, IntoMessage, FromMessage,
	)]
	#[message(protobuf)]
	#[allow(dead_code)]
	struct TestReq2 {
		#[field(1)]
		hello: u64,
	}
}
