
use super::message::{Action, Message};
use super::request::{Request, Response};

use crate::Result;
use crate::traits::ByteStream;
use crate::client::{Connection, Config, ReconStrat};
use crate::packet::{Packet, PacketBytes, PlainBytes};

#[cfg(feature = "encrypted")]
use crate::packet::EncryptedBytes;
#[cfg(feature = "encrypted")]
use crypto::signature::PublicKey;

pub struct Client<A, B> {
	inner: Connection<Message<A, B>>
}

impl<A, B> Client<A, B> {

	pub async fn request<R>(&self, req: R) -> Result<R::Response>
	where
		R: Request<A, B>,
		A: Action,
		B: PacketBytes
	{
		let mut req = req.into_message()?;
		req.header_mut().set_action(R::action());
		let res = self.inner.request(req).await?;
		R::Response::from_message(res)
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