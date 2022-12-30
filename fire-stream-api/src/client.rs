
use crate::message::{Action, Message, SerdeMessage};
use crate::request::{Request, RawRequest};
use crate::error::ApiError;

use stream::traits::ByteStream;
use stream::client::Connection;
pub use stream::client::{Config, ReconStrat};
use stream::packet::{Packet, PacketBytes};
pub use stream::packet::PlainBytes;
use stream::StreamError;

#[cfg(feature = "encrypted")]
pub use stream::packet::EncryptedBytes;
#[cfg(feature = "encrypted")]
use crypto::signature::PublicKey;

pub struct Client<A, B> {
	inner: Connection<Message<A, B>>
}

impl<A, B> Client<A, B> {

	pub async fn request<R>(&self, req: R) -> Result<R::Response, R::Error>
	where
		R: Request<A, B>,
		A: Action,
		B: PacketBytes
	{

		// serialize
		let mut msg = Message::new();
		msg.body_mut()
			.serialize(&req)
			.map_err(|e| R::Error::request(
				format!("malformed request: {}", e)
			))?;
		msg.header_mut().set_action(R::ACTION);

		let res = self.inner.request(msg).await;
		let res = match res {
			Ok(r) => r,
			Err(e) => return Err(match e {
				// those are probably not returned
				StreamError::ConnectionClosed => R::Error::connection_closed(),
				StreamError::RequestDropped => R::Error::request_dropped(),
				// stream closed does not occur in a request
				e => R::Error::other(format!("{}", e))
			})
		};

		// now deserialize the response
		if res.is_success() {
			res.body().deserialize()
				.map_err(|e| R::Error::response(
					format!("malformed response: {}", e)
				))
		} else {
			// deserialize the error or create an error
			let e = res.body().deserialize()
				.unwrap_or_else(|e| R::Error::response(
					format!("malformed error: {}", e)
				));
			Err(e)
		}
	}

	pub async fn raw_request<R>(
		&self,
		req: R
	) -> Result<R::Response, R::Error>
	where
		R: RawRequest<A, B>,
		A: Action,
		B: PacketBytes
	{

		let mut msg = req.into_message()?;
		msg.header_mut().set_action(R::ACTION);

		let res = self.inner.request(msg).await;
		let res = match res {
			Ok(r) => r,
			Err(e) => return Err(match e {
				// those are probably not returned
				StreamError::ConnectionClosed => R::Error::connection_closed(),
				StreamError::RequestDropped => R::Error::request_dropped(),
				// stream closed does not occur in a request
				e => R::Error::other(format!("{}", e))
			})
		};

		// now deserialize the response
		if res.is_success() {
			R::Response::from_message(res)
		} else {
			// deserialize the error or create an error
			let e = res.body().deserialize()
				.unwrap_or_else(|e| R::Error::response(
					format!("malformed error: {}", e)
				));
			Err(e)
		}
	}

	pub async fn close(self) {
		let _ = self.inner.close().await;
	}

}

impl<A> Client<A, PlainBytes>
where A: Action + Send + 'static {

	// plain
	pub fn new<S>(
		stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>
	) -> Self
	where S: ByteStream {
		Self {
			inner: Connection::new(stream, cfg, recon_strat)
		}
	}

}

#[cfg(feature = "encrypted")]
impl<A> Client<A, EncryptedBytes>
where A: Action + Send + 'static {

	pub fn new<S>(
		stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>,
		pub_key: PublicKey
	) -> Self
	where S: ByteStream {
		Self {
			inner: Connection::new_encrypted(stream, cfg, recon_strat, pub_key)
		}
	}

}