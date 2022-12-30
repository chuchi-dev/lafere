
use super::message::{Message, Action};
use crate::Result;
use crate::listener::{SocketAddr, Listener, ListenerExt};
use crate::handler;
use crate::packet::{Packet, PlainBytes, PacketBytes};
use crate::server::{Connection, Config};
use crate::pinned_future::PinnedFuture;

#[cfg(feature = "encrypted")]
use crate::packet::EncryptedBytes;

use std::collections::HashMap;
use std::any::{Any, TypeId};
use std::sync::Arc;
use std::sync::Mutex;

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
	inner: HashMap<A, Box<dyn RequestHandler<A, B> + Send + Sync>>
}

impl<A, B> Requests<A, B>
where A: Action {

	fn new() -> Self {
		Self {
			inner: HashMap::new()
		}
	}

	fn insert<H>(&mut self, handler: H)
	where H: RequestHandler<A, B> + Send + Sync + 'static {
		self.inner.insert(H::action(), Box::new(handler));
	}

	fn get(&self, action: &A) -> Option<&Box<dyn RequestHandler<A, B> + Send + Sync>> {
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
	where H: RequestHandler<A, B> + Send + Sync + 'static {
		handler.validate_data(&self.data);
		self.requests.insert(handler);
	}

	pub fn register_data<D>(&mut self, data: D)
	where D: Any + Send + Sync {
		self.data.insert(data);
	}

	// pub async fn accept(&self) -> Result<Connection> {
	// 	let (stream, addr) = 
	// 	let inner = encrypted::client(stream, TIMEOUT, pub_key);
	// 	Ok(Self { inner })
	// }

	// pub async fn request<R>(&self, req: R) -> Result<R::Response>
	// where R: Request {
	// 	let req = req.into_message()?;
	// 	let res = self.inner.request(req).await
	// 		.map_err(Error::Stream)?;
	// 	R::Response::from_message(res)
	// }

	/// Panics if one of the request handlers panics
	async fn run_raw<F>(self, new_connection: F) -> Result<()>
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

					if let handler::Message::Request(msg, resp) = req {

						// todo should probably spawn another task
						let share = share.clone();
						let session = session.clone();

						tokio::spawn(async move {

							let action = *msg.action();
							let handler = match share.requests.get(&action) {
								Some(handler) => handler,
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
								Ok(mut r) => {
									r.header_mut().set_action(action);
									// i don't care about the response
									let _ = resp.send(r);
								},
								Err(e) => {
									eprintln!("request error {:?}", e);
								}
							}

						});
					}

				}
			});
		}
	}

}

// only
// - set
// - remove
// - get (supported)

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
	
	pub async fn run(self) -> Result<()>
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

	pub fn new(listener: L, cfg: Config, key: Keypair) -> Self {
		Self {
			inner: listener,
			requests: Requests::new(),
			data: Data::new(),
			cfg,
			more: key
		}
	}

	pub async fn run(self) -> Result<()>
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

pub trait RequestHandler<A, B> {
	fn action() -> A
	where Self: Sized;
	/// if the data is not available just panic
	fn validate_data(&self, data: &Data);
	fn handle<'a>(
		&'a self,
		msg: Message<A, B>,
		data: &'a Data,
		session: &'a Session
	) -> PinnedFuture<'a, Result<Message<A, B>>>;
}

#[doc(hidden)]
#[macro_export]
macro_rules! create_impl {
	(($a:ident)) => (
		impl<$a: Send + 'static>
	);
	(()) => (
		impl
	)
}

#[macro_export]
macro_rules! request_handler {
	// prepare the generic parameters
	(async fn $name:ident<$a:ty> $($tt:tt)*) => (
		$crate::request_handler!(async fn<B> $name<$a, B> $($tt)*);
	);
	(async fn $name:ident<$a:ty, $b:ty> $($tt:tt)*) => (
		$crate::request_handler!(async fn<> $name<$a, $b> $($tt)*);
	);
	(async fn<$($gen:ident)?> $name:ident<$a:ty, $b:ty>($req:ident: $req_ty:ty) $($tt:tt)*) => (
		$crate::request_handler!(async fn<$($gen)?> $name<$a, $b>($req:$req_ty, ) $($tt)*);
	);
	(
		async fn<$($gen:ident)?> $name:ident<$a:ty, $b:ty>(
			$req:ident: $req_ty:ty,
			$($data:ident: $data_ty:ty),*
		) -> $ret_ty:ty
		$block:block
	) => (

		#[allow(non_camel_case_types)]
		struct $name;

		impl<$($gen: Send + 'static)?> $crate::basic::server::RequestHandler<$a, $b> for $name {
			fn action() -> $a {
				// make sure req_ty implemts trait
				<$req_ty as $crate::basic::request::Request<$a, $b>>::action()
			}

			fn validate_data(&self, data: &$crate::basic::server::Data) {
				$(
					assert!(
						data.exists::<$data_ty>(),
						concat!("data ", stringify!($data_ty), " does not exists")
					);
				)*
			}

			fn handle<'a>(
				&'a self,
				msg: $crate::basic::message::Message<$a, $b>,
				data: &'a $crate::basic::server::Data,
				session: &'a $crate::basic::server::Session
			) -> $crate::pinned_future::PinnedFuture<'a, $crate::Result<$crate::basic::message::Message<$a, $b>>> {
				// extract all data
				$(
					let $data: &$data_ty = data.get_or_sess(&session)
						.expect(concat!("could not find ", stringify!($data_ty)));
				)*

				async fn __handle(
					$req: $req_ty,
					$($data: &$data_ty),*
				) -> $ret_ty {
					$block
				}

				$crate::pinned_future::PinnedFuture::new(async move {
					// convert the msg
					let $req: $req_ty = $crate::basic::request::Request::from_message(msg)?;

					let resp = __handle($req, $($data),*).await;

					// make sure the type is correct
					let resp: $crate::Result<<$req_ty as $crate::basic::request::Request<$a, $b>>::Response> = resp;

					$crate::basic::request::Response::into_message(resp?)
				})
			}
		}
	)
}


#[cfg(test)]
mod tests {
	use crate::basic::request::{Request, Response};
	use crate::basic::message::{self, Message};
	use crate::basic::server::Session;
	use crate::Result;
	use crate::packet::PlainBytes;

	#[derive(Debug)]
	struct TestReq;

	#[derive(Debug)]
	struct TestResp;

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	pub enum Action {
		Empty
	}

	impl message::Action for Action {
		fn empty() -> Self { Self::Empty }
		fn from_u16(_num: u16) -> Option<Self> { todo!() }
		fn as_u16(&self) -> u16 { todo!() }
	}

	#[derive(Debug)]
	struct Data;

	impl<B> Request<Action, B> for TestReq {
		type Response = TestResp;
		fn action() -> Action { Action::Empty }
		fn into_message(self) -> Result<Message<Action, B>> { todo!() }
		fn from_message(_msg: Message<Action, B>) -> Result<Self> { todo!() }
	}

	impl<B> Response<Action, B> for TestResp {
		fn into_message(self) -> Result<Message<Action, B>> { todo!() }
		fn from_message(_msg: Message<Action, B>) -> Result<Self> { todo!() }
	}

	request_handler!(
		async fn test<Action>(req: TestReq, data: &Data) -> Result<TestResp> {
			println!("req {:?} data {:?}", req, data);
			Ok(TestResp)
		}
	);

	request_handler!(
		async fn test_2<Action, PlainBytes>(req: TestReq, data: &Data) -> Result<TestResp> {
			println!("req {:?} data {:?}", req, data);
			Ok(TestResp)
		}
	);

	request_handler!(
		async fn test_3<Action>(req: TestReq, _data: &Session) -> Result<TestResp> {
			println!("req {:?} data", req);
			Ok(TestResp)
		}
	);
}