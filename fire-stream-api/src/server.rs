
use crate::message::{Action, Message};
use crate::util::PinnedFuture;

use stream::listener::{SocketAddr, Listener, ListenerExt};
use stream::handler;
use stream::packet::{Packet, PacketBytes};
pub use stream::packet::PlainBytes;
use stream::server::Connection;
pub use stream::server::Config;

#[cfg(feature = "encrypted")]
pub use stream::packet::EncryptedBytes;

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

	async fn run_raw<F>(self, new_connection: F) -> stream::Result<()>
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

							if let Some(mut res_msg) = r {
								res_msg.header_mut().set_action(action);
								// i don't care about the response
								let _ = resp.send(res_msg);
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
	
	pub async fn run(self) -> stream::Result<()>
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

	pub async fn run(self) -> stream::Result<()>
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

// we should have a request handler +
// a raw request handler which expects a message
// instead of a response

pub trait RequestHandler<A, B> {

	fn action() -> A
	where Self: Sized;

	/// if the data is not available just panic
	fn validate_data(&self, data: &Data);

	/// handles a message with Self::ACTION as action.
	/// 
	/// if None is returned the request is abandoned and
	/// the requestor receives a RequestDropped error
	fn handle<'a>(
		&'a self,
		msg: Message<A, B>,
		data: &'a Data,
		session: &'a Session
	) -> PinnedFuture<'a, Option<Message<A, B>>>;
}

/// Helper to implement a RequestHandler.
///
/// ## Note
/// Even though you need to specify the type without a reference
/// that actual type will have a reference.
/// 
/// ## Usage
/// ```ignore
/// request_handler! {
/// 	async fn name<Action>(
/// 		req: ReqType,
/// 		some: Data
/// 	) -> Result<Response> {}
/// }
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! inner_request_handler {
	// final handler
	(
		async fn<$($gen:ident, $gen2:ident)?> $name:ident<$a:ty, $b:ty, $d:ty>(
			$req:ident: $req_ty:ty,
			$($data:ident: $data_ty:ty),*
		) -> $ret_ty:ty
		$block:block,
		$request_trait:ident,
		// serialize block
		|$se:ident| $serialize_block:block,
		// deserialize block
		|$de:ident| $deserialize_block:block
	) => (

		#[allow(non_camel_case_types)]
		struct $name;

		impl<$($gen: $crate::message::PacketBytes + Send + 'static)?>
			$crate::server::RequestHandler<$a, $b> for $name
		{


			// const ACTION = <$req_ty as $crate::request::Request<$a, $b>>::ACTION;
			fn action() -> $a {
				<$req_ty as $crate::request::$request_trait<$a, $b>>::ACTION
			}

			fn validate_data(&self, data: &$crate::server::Data) {
				$(
					assert!(
						data.exists::<$data_ty>(),
						concat!("data ", stringify!($data_ty), " does not exists")
					);
				)*
			}

			fn handle<'a>(
				&'a self,
				msg: $crate::message::Message<$a, $b>,
				data: &'a $crate::server::Data,
				session: &'a $crate::server::Session
			) -> $crate::util::PinnedFuture<'a, Option<$crate::message::Message<$a, $b>>> {
				use $crate::error::ApiError as __ApiError;

				async fn __handle(
					$req: $req_ty,
					$($data: &$data_ty),*
				) -> $ret_ty {
					$block
				}

				// use $crate::{
				// 	message::Message,
				// 	server::{Data, Session},
				// 	util::PinnedFuture,
				// 	request::Request
				// };
				// use std::result::Result;

				use $crate::request::$request_trait as __Request;
				use $crate::message::Message as __Message;

				// type __Message = $crate::message::Message<$a, $b>;

				type __Error<C> = <$req_ty as __Request<$a, C>>::Error;
				type __Response<C> = <$req_ty as __Request<$a, C>>::Response;

				async fn __handle_with_error<$($gen2: $crate::message::PacketBytes + Send + 'static)?>(
					msg: __Message<$a, $d>,
					data: &$crate::server::Data,
					session: &$crate::server::Session
				) -> std::result::Result<__Message<$a, $d>, __Error<$d>> {

					// extract all data
					$(
						let $data: &$data_ty = data.get_or_sess(&session)
							.ok_or_else(|| {
								__Error::<$d>::internal(concat!(
									"could not find ",
									stringify!($data_ty)
								))
							})?;
					)*

					// now parse the request with json

					// A request should never contain an error
					if !msg.is_success() {
						return Err(__Error::<$d>::request(concat!(
							"received a message with an error in ",
							stringify!($req_ty)
						)));
					}

					let $de = msg;
					let req: $req_ty = { $deserialize_block }?;

					let res: __Response<$d> = __handle(req, $($data),*).await?;

					let $se = res;

					{ $serialize_block }
				}

				$crate::util::PinnedFuture::new(async move {
		
					let res = __handle_with_error(msg, data, session).await;
					match res {
						Ok(res) => Some(res),
						Err(e) => {
							eprintln!(
								"request handler error {} in {:?}",
								e,
								<$req_ty as __Request<$a, $b>>::ACTION
							);
							// let's try to convert the error into a message
							let mut err = __Message::new();
							err.set_success(false);
							err.body_mut().serialize(&e)
								.map_err(|e| {
									eprintln!("request handler could not \
										serialize error {:?}", e);
								})
								.map(|_| err)
								.ok()
						}
					}

				})
			}
		}
	)
}

/// Helper to implement a RequestHandler.
///
/// ## Note
/// Even though you need to specify the type without a reference
/// that actual type will have a reference.
/// 
/// ## Usage
/// ```ignore
/// raw_request_handler! {
/// 	async fn name<Action>(
/// 		req: ReqType,
/// 		some: Data
/// 	) -> Result<Response> {}
/// }
/// ```
#[macro_export]
macro_rules! raw_request_handler {
	// catch handler without generic parameters
	(async fn $name:ident<$a:ty> $($tt:tt)*) => (
		$crate::raw_request_handler!(async fn<B, D> $name<$a, B, D> $($tt)*);
	);
	// catch handler with both parameters defined
	(async fn $name:ident<$a:ty, $b:ty> $($tt:tt)*) => (
		$crate::raw_request_handler!(async fn<> $name<$a, $b, $b> $($tt)*);
	);
	// catch handler who has only the request argument
	(
		async fn<$($gen:ident, $gen2:ident)?> $name:ident<$a:ty, $b:ty, $d:ty>(
			$req:ident: $req_ty:ty
		) $($tt:tt)*
	) => (
		$crate::raw_request_handler!(
			async fn<$($gen, $gen2)?> $name<$a, $b, $d>($req: $req_ty, ) $($tt)*
		);
	);
	// final handler
	(
		async fn<$($gen:ident, $gen2:ident)?> $name:ident<$a:ty, $b:ty, $d:ty>(
			$req:ident: $req_ty:ty,
			$($data:ident: $data_ty:ty),*
		) -> $ret_ty:ty
		$block:block
	) => (
		$crate::inner_request_handler!(
			async fn<$($gen, $gen2)?> $name<$a, $b, $d>(
				$req: $req_ty,
				$($data: $data_ty),*
			) -> $ret_ty
			{ $block },
			RawRequest,
			|se| {
				$crate::message::SerdeMessage::into_message(se)
			},
			|de| {
				$crate::message::SerdeMessage::from_message(de)
			}
		);
	)
}

/// Helper to implement a RequestHandler.
///
/// ## Note
/// Even though you need to specify the type without a reference
/// that actual type will have a reference.
/// 
/// ## Usage
/// ```ignore
/// request_handler! {
/// 	async fn name<Action>(
/// 		req: ReqType,
/// 		some: Data
/// 	) -> Result<Response> {}
/// }
/// ```
#[macro_export]
macro_rules! request_handler {
	// catch handler without generic parameters
	(async fn $name:ident<$a:ty> $($tt:tt)*) => (
		$crate::request_handler!(async fn<B, D> $name<$a, B, D> $($tt)*);
	);
	// catch handler with both parameters defined
	(async fn $name:ident<$a:ty, $b:ty> $($tt:tt)*) => (
		$crate::request_handler!(async fn<> $name<$a, $b, $b> $($tt)*);
	);
	// catch handler who has only the request argument
	(
		async fn<$($gen:ident, $gen2:ident)?> $name:ident<$a:ty, $b:ty, $d:ty>(
			$req:ident: $req_ty:ty
		) $($tt:tt)*
	) => (
		$crate::request_handler!(
			async fn<$($gen)?> $name<$a, $b, $d>($req: $req_ty, ) $($tt)*
		);
	);
	// final handler
	(
		async fn<$($gen:ident, $gen2:ident)?> $name:ident<$a:ty, $b:ty, $d:ty>(
			$req:ident: $req_ty:ty,
			$($data:ident: $data_ty:ty),*
		) -> $ret_ty:ty
		$block:block
	) => (
		$crate::inner_request_handler!(
			async fn<$($gen, $gen2)?> $name<$a, $b, $d>(
				$req: $req_ty,
				$($data: $data_ty),*
			) -> $ret_ty
			{ $block },
			Request,
			|se| {
				let mut resp = __Message::new();
				resp.body_mut()
					.serialize(&se)
					.map_err(|e| __Error::<$d>::internal(
						format!("malformed response: {}", e)
					))?;
				Ok(resp)
			},
			|de| {
				de.body().deserialize()
					.map_err(|e| __Error::<$d>::request(
						format!("malformed request: {}", e)
					))
			}
		);
	)
}

#[cfg(test)]
mod tests {

	use crate::derive_serde_message;
	use crate::request;
	use crate::message;
	use crate::error;

	use std::fmt;

	use stream::packet::PlainBytes;

	use serde::{Serialize, Deserialize};

	#[derive(Debug, Serialize, Deserialize)]
	struct TestReq {
		hello: u64
	}

	#[derive(Debug, Serialize, Deserialize)]
	struct TestReq2 {
		hello: u64
	}

	derive_serde_message!(TestReq);

	#[derive(Debug, Serialize, Deserialize)]
	struct TestResp {
		hi: u64
	}

	derive_serde_message!(TestResp);

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	pub enum Action {
		Empty
	}

	#[derive(Debug, Clone, Serialize, Deserialize)]
	pub enum Error {}

	impl fmt::Display for Error {
		fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
			fmt::Debug::fmt(self, fmt)
		}
	}

	impl error::ApiError for Error {
		fn connection_closed() -> Self { todo!() }
		fn request_dropped() -> Self { todo!() }
		fn internal<E: error::Error>(_error: E) -> Self { todo!() }
		fn request<E: error::Error>(_error: E) -> Self { todo!() }
		fn response<E: error::Error>(_error: E) -> Self { todo!() }
		fn other<E: error::Error>(_error: E) -> Self { todo!() }
	}

	impl message::Action for Action {
		fn empty() -> Self { Self::Empty }
		fn from_u16(_num: u16) -> Option<Self> { todo!() }
		fn as_u16(&self) -> u16 { todo!() }
	}

	#[derive(Debug)]
	struct Data;

	impl<B> request::Request<Action, B> for TestReq {
		type Response = TestResp;
		type Error = Error;

		const ACTION: Action = Action::Empty;
	}

	impl request::Request<Action, PlainBytes> for TestReq2 {
		type Response = TestResp;
		type Error = Error;

		const ACTION: Action = Action::Empty;
	}

	impl<B> request::RawRequest<Action, B> for TestReq {
		type Response = TestResp;
		type Error = Error;

		const ACTION: Action = Action::Empty;
	}

	// impl<B> Response<Action, B> for TestResp {
	// 	fn into_message(self) -> Result<Message<Action, B>> { todo!() }
	// 	fn from_message(_msg: Message<Action, B>) -> Result<Self> { todo!() }
	// }

	request_handler! {
		async fn test<Action>(
			req: TestReq,
			data: &Data
		) -> Result<TestResp, Error> {
			println!("req {:?} data {:?}", req, data);
			Ok(TestResp {
				hi: req.hello
			})
		}
	}

	request_handler! {
		async fn test_2<Action, PlainBytes>(
			req: TestReq2,
			data: &Data
		) -> Result<TestResp, Error> {
			println!("req {:?} data {:?}", req, data);
			Ok(TestResp {
				hi: req.hello
			})
		}
	}

	raw_request_handler! {
		async fn test_3<Action>(
			req: TestReq,
			data: &Data
		) -> Result<TestResp, Error> {
			println!("req {:?} data {:?}", req, data);
			Ok(TestResp {
				hi: req.hello
			})
		}
	}
}