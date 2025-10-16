use super::{Configurator, StreamReceiver, StreamSender};
use crate::error::RequestError;
use crate::handler::handler::InternalRequest;
use crate::util::watch;

use tokio::sync::{mpsc, oneshot};

/// A sender that sends messages to the handler.
pub struct Sender<P, C> {
	pub(super) inner: mpsc::Sender<InternalRequest<P>>,
	pub(super) cfg: watch::Sender<C>,
}

impl<P, C> Sender<P, C> {
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

	pub fn update_config(&self, cfg: C) {
		self.cfg.send(cfg);
	}

	pub fn configurator(&self) -> Configurator<C> {
		Configurator::new(self.cfg.clone())
	}
}

impl<P, C> Clone for Sender<P, C> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			cfg: self.cfg.clone(),
		}
	}
}
