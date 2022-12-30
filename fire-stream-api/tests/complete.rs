use std::time::Duration;

use fire_stream_api::{
	server::{EncryptedBytes, Server as ApiServer, Config as ServerConfig},
	client::{Client as ApiClient, Config as ClientConfig}
};

use tokio::net::{TcpListener, TcpStream};

use crypto::signature::Keypair;

type Server = ApiServer<api::Action, EncryptedBytes, TcpListener, Keypair>;
type Client = ApiClient<api::Action, EncryptedBytes>;

mod api {
	use std::fmt;

	use serde::{Serialize, Deserialize};

	use fire_stream_api::{
		message,
		error::{ApiError, Error as ErrorTrait},
		request::Request
	};

	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	pub enum Action {
		Unknown,
		Act1,
		Act2
	}

	impl message::Action for Action {
		fn empty() -> Self {
			Self::Unknown
		}

		fn from_u16(num: u16) -> Option<Self> {
			println!("message action from {}", num);
			match num {
				1 => Some(Self::Act1),
				2 => Some(Self::Act2),
				_ => None
			}
		}

		fn as_u16(&self) -> u16 {
			match self {
				Self::Unknown => 0,
				Self::Act1 => 1,
				Self::Act2 => 2
			}
		}
	}

	#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
	pub enum Error {
		ConnectionClosed,
		RequestDropped,
		CustomError,
		Internal(String),
		Request(String),
		Response(String),
		Other(String)
	}

	impl ApiError for Error {
		fn connection_closed() -> Self {
			Self::ConnectionClosed
		}

		fn request_dropped() -> Self {
			Self::RequestDropped
		}

		fn internal<E: ErrorTrait>(e: E) -> Self {
			Self::Internal(e.to_string())
		}

		fn request<E: ErrorTrait>(e: E) -> Self {
			Self::Request(e.to_string())
		}

		fn response<E: ErrorTrait>(e: E) -> Self {
			Self::Response(e.to_string())
		}

		fn other<E: ErrorTrait>(e: E) -> Self {
			Self::Other(e.to_string())
		}
	}

	impl fmt::Display for Error {
		fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
			fmt::Debug::fmt(self, f)
		}
	}

	#[derive(Debug, Clone, Serialize, Deserialize)]
	pub struct Act1Req;

	#[derive(Debug, Clone, Serialize, Deserialize)]
	pub struct Act1 {
		pub hello: String
	}

	impl<B> Request<Action, B> for Act1Req {
		type Response = Act1;
		type Error = Error;
		const ACTION: Action = Action::Act1;
	}

	#[derive(Debug, Clone, Serialize, Deserialize)]
	pub struct Act2Req {
		pub hi: String
	}

	#[derive(Debug, Clone, Serialize, Deserialize)]
	pub struct Act2 {
		pub numbers: u64
	}

	impl<B> Request<Action, B> for Act2Req {
		type Response = Act2;
		type Error = Error;
		const ACTION: Action = Action::Act2;
	}
}

mod handlers {
	use crate::api::*;
	use crate::Server;

	use fire_stream_api::{
		request_handler
	};


	type Result<T> = std::result::Result<T, Error>;

	request_handler! {
		async fn act_1<Action>(
			_req: Act1Req
		) -> Result<Act1> {
			Ok(Act1 {
				hello: format!("Hello, World!")
			})
		}
	}

	request_handler! {
		async fn act_2<Action>(
			req: Act2Req
		) -> Result<Act2> {
			Ok(Act2 {
				numbers: req.hi.len() as u64
			})
		}
	}

	pub fn handle(server: &mut Server) {
		server.register_request(act_1);
		server.register_request(act_2);
	}
}

#[tokio::test]
async fn main() {
	let listener = TcpListener::bind(("127.0.0.1", 0)).await
		.expect("could not create listener");
	let addr = listener.local_addr().unwrap();
	let priv_key = Keypair::new();
	let pub_key = priv_key.public().clone();

	tokio::spawn(async move {
		// spawn server
		let mut server = Server::new(listener, ServerConfig {
			timeout: Duration::from_secs(10),
			body_limit: 0
		}, priv_key);

		handlers::handle(&mut server);

		server.run().await
			.expect("server failed");
	});

	// now connect
	let stream = TcpStream::connect(addr).await
		.expect("failed to connect to server");
	let client = Client::new(stream, ClientConfig {
		timeout: Duration::from_secs(10),
		body_limit: 0
	}, None, pub_key);

	let r = client.request(api::Act1Req).await
		.expect("act1 request failed");
	assert_eq!(r.hello, "Hello, World!");

	let r = client.request(api::Act2Req {
		hi: "12345".into()
	}).await.expect("act2 failed");
	assert_eq!(r.numbers, 5);

	client.close().await;
}