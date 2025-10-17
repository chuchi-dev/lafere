use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{net::SocketAddr, sync::atomic::AtomicBool};

use lafere_api::{
	client::{Client, Config as ClientConfig},
	request_handlers::RequestHandlers,
	requestor::Requestor,
	server::{Config as ServerConfig, Server},
};

use tokio::{
	net::{TcpListener, TcpStream},
	time::sleep,
};

use crypto::signature::Keypair;

mod api {
	use std::fmt;

	use serde::{Deserialize, Serialize};

	use lafere_api::{
		Action, FromMessage, IntoMessage,
		error::{ApiError, MessageError, RequestError},
		request::Request,
	};

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Action)]
	#[repr(u16)]
	pub enum Action {
		Act1 = 1,
		Act2 = 2,
		MyAddress = 3,
		AlwaysError = 4,
	}

	#[derive(
		Debug,
		Clone,
		PartialEq,
		Eq,
		Serialize,
		Deserialize,
		IntoMessage,
		FromMessage,
	)]
	#[message(json)]
	pub enum Error {
		MyError,
		RequestError(String),
		MessageError(String),
	}

	impl ApiError for Error {
		fn from_request_error(e: RequestError) -> Self {
			Self::RequestError(e.to_string())
		}

		fn from_message_error(e: MessageError) -> Self {
			Self::MessageError(e.to_string())
		}
	}

	impl fmt::Display for Error {
		fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
			fmt::Debug::fmt(self, f)
		}
	}

	impl std::error::Error for Error {}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct Act1Req;

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct Act1 {
		pub hello: String,
	}

	impl Request for Act1Req {
		type Action = Action;
		type Response = Act1;
		type Error = Error;

		const ACTION: Action = Action::Act1;
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct Act2Req {
		pub hi: String,
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct Act2 {
		pub numbers: u64,
	}

	impl Request for Act2Req {
		type Action = Action;
		type Response = Act2;
		type Error = Error;

		const ACTION: Action = Action::Act2;
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct MyAddressReq;

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct MyAddress {
		pub addr: String,
	}

	impl Request for MyAddressReq {
		type Action = Action;
		type Response = MyAddress;
		type Error = Error;

		const ACTION: Action = Action::MyAddress;
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct AlwaysErrorReq;

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct AlwaysError {
		pub addr: String,
	}

	impl Request for AlwaysErrorReq {
		type Action = Action;
		type Response = AlwaysError;
		type Error = Error;

		const ACTION: Action = Action::AlwaysError;
	}
}

mod handlers {
	use super::MyAddr;
	use crate::api::*;

	use lafere_api::api;

	type Result<T> = std::result::Result<T, Error>;

	#[api(Act1Req)]
	pub fn act_1() -> Result<Act1> {
		Ok(Act1 {
			hello: format!("Hello, World!"),
		})
	}

	#[api(Act2Req)]
	pub async fn act_2(req: Act2Req) -> Result<Act2> {
		Ok(Act2 {
			numbers: req.hi.len() as u64,
		})
	}

	#[api(MyAddressReq)]
	pub fn my_address(addr: &MyAddr) -> Result<MyAddress> {
		Ok(MyAddress {
			addr: addr.0.to_string(),
		})
	}

	#[api(AlwaysErrorReq)]
	pub fn always_error() -> Result<AlwaysError> {
		Err(Error::MyError)
	}
}

#[derive(Clone)]
struct OkFlag(Arc<AtomicBool>);

#[derive(Clone)]
struct MyAddr(SocketAddr);

#[lafere_api::enable_server_requests(api::Action)]
async fn enable_server_requests(
	sender: Requestor<api::Action, B>,
	addr: &MyAddr,
	ok_flag: &OkFlag,
) -> Result<(), lafere_api::error::Error> {
	let r = sender.request(api::Act1Req).await.unwrap();
	assert_eq!(r.hello, "Hello, World!");

	let r = sender
		.request(api::Act2Req { hi: "12345".into() })
		.await
		.unwrap();
	assert_eq!(r.numbers, 5);

	let r = sender.request(api::MyAddressReq).await.unwrap();
	assert_eq!(r.addr, addr.0.to_string());

	let e = sender.request(api::AlwaysErrorReq).await.unwrap_err();
	assert_eq!(e, api::Error::MyError);

	ok_flag.0.store(true, Ordering::SeqCst);

	Ok(())
}

#[tokio::test]
async fn main() {
	let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
	let addr = listener.local_addr().unwrap();
	let priv_key = Keypair::new();
	let pub_key = priv_key.public().clone();

	let my_addr = MyAddr(addr.clone());
	let my_addr_clone = my_addr.clone();
	let ok_flag = OkFlag(Arc::new(AtomicBool::new(false)));
	let ok_flag_clone = ok_flag.clone();

	tokio::spawn(async move {
		// spawn server
		let mut server = Server::new_encrypted(
			listener,
			ServerConfig {
				timeout: Duration::from_secs(10),
				body_limit: 0,
			},
			priv_key,
		);

		server.register_data(my_addr_clone);
		server.register_data(ok_flag_clone);
		server.register_enable_server_requests(enable_server_requests);

		server.run().await.unwrap();
	});

	let mut requests = RequestHandlers::builder();
	requests.register_data(my_addr);
	requests.register_request(handlers::act_1);
	requests.register_request(handlers::act_2);
	requests.register_request(handlers::my_address);
	requests.register_request(handlers::always_error);
	let requests = requests.build();

	// now connect
	let stream = TcpStream::connect(addr.clone()).await.unwrap();
	let mut client = Client::new_encrypted(
		stream,
		ClientConfig {
			timeout: Duration::from_secs(10),
			body_limit: 0,
		},
		None,
		pub_key,
	);
	client.attach_request_handlers(requests).await.unwrap();

	sleep(Duration::from_secs(1)).await;

	assert!(ok_flag.0.load(Ordering::SeqCst));
	client.close().await.unwrap();
}
