use std::{
	any::{Any, TypeId},
	collections::HashMap,
	sync::Arc,
};

use lafere::{
	handler::{Receiver, Sender},
	packet::{Packet, PacketBytes},
	server,
};

use crate::{
	message::{Action, Message},
	request::{EnableServerRequestsHandler, RequestHandler},
	requestor::Requestor,
	server::Session,
};

pub struct Data {
	inner: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Data {
	fn new() -> Self {
		Self {
			inner: HashMap::new(),
		}
	}

	pub fn exists<D>(&self) -> bool
	where
		D: Any,
	{
		TypeId::of::<D>() == TypeId::of::<Session>()
			|| self.inner.contains_key(&TypeId::of::<D>())
	}

	fn insert<D>(&mut self, data: D)
	where
		D: Any + Send + Sync,
	{
		self.inner.insert(data.type_id(), Box::new(data));
	}

	pub fn get<D>(&self) -> Option<&D>
	where
		D: Any,
	{
		self.inner
			.get(&TypeId::of::<D>())
			.and_then(|a| a.downcast_ref())
	}

	pub fn get_or_sess<'a, D>(&'a self, sess: &'a Session) -> Option<&'a D>
	where
		D: Any,
	{
		if TypeId::of::<D>() == TypeId::of::<Session>() {
			<dyn Any>::downcast_ref(sess)
		} else {
			self.get()
		}
	}
}

struct Requests<A, B> {
	inner: HashMap<A, Box<dyn RequestHandler<B, Action = A> + Send + Sync>>,
	enable_server_requests: Option<
		Box<dyn EnableServerRequestsHandler<B, Action = A> + Send + Sync>,
	>,
}

impl<A, B> Requests<A, B>
where
	A: Action,
{
	fn new() -> Self {
		Self {
			inner: HashMap::new(),
			enable_server_requests: None,
		}
	}

	fn insert<H>(&mut self, handler: H)
	where
		H: RequestHandler<B, Action = A> + Send + Sync + 'static,
	{
		self.inner.insert(H::action(), Box::new(handler));
	}

	fn insert_enable_server_requests<H>(&mut self, handler: H)
	where
		H: EnableServerRequestsHandler<B, Action = A> + Send + Sync + 'static,
	{
		self.enable_server_requests = Some(Box::new(handler));
	}

	fn get(
		&self,
		action: &A,
	) -> Option<&Box<dyn RequestHandler<B, Action = A> + Send + Sync>> {
		self.inner.get(action)
	}

	fn get_enable_server_requests(
		&self,
	) -> Option<
		&Box<dyn EnableServerRequestsHandler<B, Action = A> + Send + Sync>,
	> {
		self.enable_server_requests.as_ref()
	}
}

pub struct RequestHandlersBuilder<A, B> {
	requests: Requests<A, B>,
	data: Data,
}

impl<A, B> RequestHandlersBuilder<A, B>
where
	A: Action,
{
	pub fn new() -> Self {
		Self {
			requests: Requests::new(),
			data: Data::new(),
		}
	}

	pub fn register_data<D>(&mut self, data: D)
	where
		D: Any + Send + Sync,
	{
		self.data.insert(data);
	}

	pub fn register_request<H>(&mut self, handler: H)
	where
		H: RequestHandler<B, Action = A> + Send + Sync + 'static,
	{
		handler.validate_data(&self.data);
		self.requests.insert(handler);
	}

	pub(crate) fn register_enable_server_requests<H>(&mut self, handler: H)
	where
		H: EnableServerRequestsHandler<B, Action = A> + Send + Sync + 'static,
	{
		handler.validate_data(&self.data);
		self.requests.insert_enable_server_requests(handler);
	}

	pub fn build(self) -> RequestHandlers<A, B> {
		RequestHandlers(Arc::new(self))
	}
}

pub struct RequestHandlers<A, B>(Arc<RequestHandlersBuilder<A, B>>);

impl<A, B> RequestHandlers<A, B>
where
	A: Action,
{
	pub fn builder() -> RequestHandlersBuilder<A, B> {
		RequestHandlersBuilder::new()
	}

	pub fn get_handler(
		&self,
		action: &A,
	) -> Option<&Box<dyn RequestHandler<B, Action = A> + Send + Sync>> {
		self.0.requests.get(action)
	}

	pub fn data(&self) -> &Data {
		&self.0.data
	}

	pub fn get_data<D>(&self) -> Option<&D>
	where
		D: Any,
	{
		self.0.data.get()
	}

	pub(crate) async fn handle_connection(
		&self,
		session: Arc<Session>,
		sender: Sender<Message<A, B>>,
		mut recv: Receiver<Message<A, B>>,
	) where
		A: Send + Sync + 'static,
		B: PacketBytes + Send + 'static,
	{
		while let Some(req) = recv.receive().await {
			// todo replace with let else
			let (msg, resp) = match req {
				server::Message::Request(msg, resp) => (msg, resp),
				server::Message::EnableServerRequests => {
					self.enable_server_requests(
						session.clone(),
						sender.clone(),
					);
					continue;
				}
				// ignore streams for now
				_ => continue,
			};

			let me = self.clone();
			let session = session.clone();

			let action = match msg.action() {
				Some(act) => *act,
				// todo once we bump the version again
				// we need to pass our own errors via packets
				// not only those from the api users
				None => {
					tracing::error!("invalid action received");
					continue;
				}
			};

			tokio::spawn(async move {
				let handler = match me.get_handler(&action) {
					Some(handler) => handler,
					// todo once we bump the version again
					// we need to pass our own errors via packets
					// not only those from the api users
					None => {
						tracing::error!("no handler for {:?}", action);
						return;
					}
				};
				let r = handler.handle(msg, &me.data(), &session).await;

				match r {
					Ok(mut msg) => {
						msg.header_mut().set_action(action);
						// i don't care about the response
						let _ = resp.send(msg);
					}
					Err(e) => {
						// todo once we bump the version again
						// we need to pass our own errors via packets
						// not only those from the api users
						tracing::error!("handler returned an error {:?}", e);
					}
				}
			});
		}
	}

	fn enable_server_requests(
		&self,
		session: Arc<Session>,
		sender: Sender<Message<A, B>>,
	) where
		A: Send + Sync + 'static,
		B: PacketBytes + Send + 'static,
	{
		let me = self.clone();

		tokio::spawn(async move {
			let handler = match me.0.requests.get_enable_server_requests() {
				Some(handler) => handler,
				// todo once we bump the version again
				// we need to pass our own errors via packets
				// not only those from the api users
				None => {
					tracing::error!(
						"no handler for enable server requests found"
					);
					return;
				}
			};

			let r = handler
				.handle(Requestor::new(sender), me.data(), &session)
				.await;
			if let Err(e) = r {
				// todo once we bump the version again
				// we need to pass our own errors via packets
				// not only those from the api users
				tracing::error!("handler returned an error {:?}", e);
			}
		});
	}
}

impl<A, B> Clone for RequestHandlers<A, B> {
	fn clone(&self) -> Self {
		Self(Arc::clone(&self.0))
	}
}
