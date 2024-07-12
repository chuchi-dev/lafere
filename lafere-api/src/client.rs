use crate::error::ApiError;
use crate::message::{Action, FromMessage, IntoMessage, Message};
use crate::request::Request;

use lafere::client::Connection;
pub use lafere::client::{Config, ReconStrat};
use lafere::error::TaskError;
pub use lafere::packet::PlainBytes;
use lafere::packet::{Packet, PacketBytes};
use lafere::util::ByteStream;

#[cfg(feature = "encrypted")]
use crypto::signature::PublicKey;
#[cfg(feature = "encrypted")]
pub use lafere::packet::EncryptedBytes;

pub struct Client<A, B> {
	inner: Connection<Message<A, B>>,
}

impl<A> Client<A, PlainBytes>
where
	A: Action + Send + 'static,
{
	// plain
	pub fn new<S>(
		stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>,
	) -> Self
	where
		S: ByteStream,
	{
		Self {
			inner: Connection::new(stream, cfg, recon_strat),
		}
	}
}

#[cfg(feature = "encrypted")]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
impl<A> Client<A, EncryptedBytes>
where
	A: Action + Send + 'static,
{
	pub fn new_encrypted<S>(
		stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>,
		pub_key: PublicKey,
	) -> Self
	where
		S: ByteStream,
	{
		Self {
			inner: Connection::new_encrypted(stream, cfg, recon_strat, pub_key),
		}
	}
}

impl<A, B> Client<A, B>
where
	A: Action,
	B: PacketBytes,
{
	pub async fn request<R>(&self, req: R) -> Result<R::Response, R::Error>
	where
		R: Request<Action = A>,
		R: IntoMessage<A, B>,
		R::Response: FromMessage<A, B>,
		R::Error: FromMessage<A, B>,
	{
		let mut msg =
			req.into_message().map_err(R::Error::from_message_error)?;
		msg.header_mut().set_action(R::ACTION);

		let res = self
			.inner
			.request(msg)
			.await
			.map_err(R::Error::from_request_error)?;

		// now deserialize the response
		if res.is_success() {
			R::Response::from_message(res).map_err(R::Error::from_message_error)
		} else {
			R::Error::from_message(res)
				.map(Err)
				.map_err(R::Error::from_message_error)?
		}
	}
}

impl<A, B> Client<A, B> {
	pub async fn close(self) -> Result<(), TaskError> {
		self.inner.close().await
	}
}
