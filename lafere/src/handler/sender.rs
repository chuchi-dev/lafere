use super::{StreamReceiver, StreamSender};
use crate::error::RequestError;
use crate::handler::handler::InternalRequest;

use tokio::sync::{mpsc, oneshot};

/// A sender that sends messages to the handler.
pub struct Sender<P> {
	pub(super) inner: mpsc::Sender<InternalRequest<P>>,
}

impl<P> Sender<P> {
	// can only be sent from a client
	pub(crate) async fn enable_server_requests(
		&self,
	) -> Result<(), RequestError> {
		self.inner
			.send(InternalRequest::EnableServerRequests)
			.await
			.map_err(|_| RequestError::ConnectionAlreadyClosed)
	}

	/// Send a request waiting until a response is available.
	pub async fn request(&self, packet: P) -> Result<P, RequestError> {
		let (tx, rx) = oneshot::channel();
		self.inner
			.send(InternalRequest::Request(packet, tx))
			.await
			.map_err(|_| RequestError::ConnectionAlreadyClosed)?;

		rx.await.map_err(|_| RequestError::TaskFailed)?
	}

	/// Create a new stream to send packets.
	pub async fn request_sender(
		&self,
		packet: P,
	) -> Result<StreamSender<P>, RequestError> {
		let (tx, rx) = mpsc::channel(10);
		self.inner
			.send(InternalRequest::RequestSender(packet, rx))
			.await
			.map_err(|_| RequestError::ConnectionAlreadyClosed)?;

		Ok(StreamSender::new(tx))
	}

	/// Opens a new stream to listen to packets.
	pub async fn request_receiver(
		&self,
		packet: P,
	) -> Result<StreamReceiver<P>, RequestError> {
		let (tx, rx) = mpsc::channel(10);
		self.inner
			.send(InternalRequest::RequestReceiver(packet, tx))
			.await
			.map_err(|_| RequestError::ConnectionAlreadyClosed)?;

		Ok(StreamReceiver::new(rx))
	}
}

impl<P> Clone for Sender<P> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
		}
	}
}
