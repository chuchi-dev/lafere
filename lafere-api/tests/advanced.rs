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
		message::{FromMessage, IntoMessage, Message, PacketBytes},
		request::Request,
		Action, FromMessage, IntoMessage,
	};

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Action)]
	#[repr(u16)]
	pub enum Action {
		RawReq = 1,
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

	// raw requests
	#[derive(Debug, Clone)]
	pub struct RawReq<B> {
		inner: Message<Action, B>,
	}

	impl<B: PacketBytes> RawReq<B> {
		pub fn new(len: usize) -> Self {
			let mut msg = Message::new();
			msg.body_mut().resize(len);

			Self { inner: msg }
		}

		pub fn body_len(&self) -> usize {
			self.inner.body().len()
		}
	}

	impl<B> IntoMessage<Action, B> for RawReq<B>
	where
		B: PacketBytes,
	{
		fn into_message(self) -> Result<Message<Action, B>, MessageError> {
			Ok(self.inner)
		}
	}

	impl<B> FromMessage<Action, B> for RawReq<B>
	where
		B: PacketBytes,
	{
		fn from_message(msg: Message<Action, B>) -> Result<Self, MessageError> {
			Ok(Self { inner: msg })
		}
	}

	#[derive(
		Debug, Clone, Serialize, Deserialize, IntoMessage, FromMessage,
	)]
	#[message(json)]
	pub struct RawResp {
		pub status: String,
	}

	impl<B> Request for RawReq<B> {
		type Action = Action;
		type Response = RawResp;
		type Error = Error;

		const ACTION: Action = Action::RawReq;
	}
}

mod handlers {
	use crate::api::*;

	use lafere_api::api;

	type Result<T> = std::result::Result<T, Error>;

	#[api(RawReq<B>)]
	pub fn raw_request(req: RawReq<B>) -> Result<RawResp> {
		Ok(RawResp {
			status: format!("worked {}", req.body_len()),
		})
	}
}

#[allow(dead_code)]
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
		server.register_request(handlers::raw_request);

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

	let e = client.request(api::RawReq::new(10)).await.unwrap();
	assert_eq!(e.status, "worked 10");

	client.close().await.unwrap();
}
