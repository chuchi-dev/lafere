use std::net::SocketAddr;
use std::time::Duration;

use lafere_api::{
	client::{Client, Config as ClientConfig},
	server::{Config as ServerConfig, Server},
};

use tokio::net::{TcpListener, TcpStream};

use crypto::signature::Keypair;

mod api {
	use std::fmt;

	use serde::{Deserialize, Serialize};

	use lafere_api::{
		error::{ApiError, MessageError, RequestError},
		request::Request,
		Action, FromMessage, IntoMessage,
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

struct MyAddr(SocketAddr);

#[tokio::test]
async fn main() {
	let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
	let addr = listener.local_addr().unwrap();
	let priv_key = Keypair::new();
	let pub_key = priv_key.public().clone();

	let my_addr = MyAddr(addr.clone());

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

		server.register_data(my_addr);
		server.register_request(handlers::act_1);
		server.register_request(handlers::act_2);
		server.register_request(handlers::my_address);
		server.register_request(handlers::always_error);

		server.run().await.unwrap();
	});

	// now connect
	let stream = TcpStream::connect(addr.clone()).await.unwrap();
	let client = Client::new_encrypted(
		stream,
		ClientConfig {
			timeout: Duration::from_secs(10),
			body_limit: 0,
		},
		None,
		pub_key,
	);

	let r = client.request(api::Act1Req).await.unwrap();
	assert_eq!(r.hello, "Hello, World!");

	let r = client
		.request(api::Act2Req { hi: "12345".into() })
		.await
		.unwrap();
	assert_eq!(r.numbers, 5);

	let r = client.request(api::MyAddressReq).await.unwrap();
	assert_eq!(r.addr, addr.to_string());

	let e = client.request(api::AlwaysErrorReq).await.unwrap_err();
	assert_eq!(e, api::Error::MyError);

	client.close().await.unwrap();
}
