use super::Configurator;
use crate::handler::handler::Request;
use crate::util::watch;

use tokio::sync::mpsc;

/// A receiver that waits on messages from the handler (client)
pub(crate) struct Receiver<P, C> {
	pub(super) inner: mpsc::Receiver<Request<P>>,
	pub(super) cfg: watch::Sender<C>,
}

impl<P, C> Receiver<P, C> {
	/// Receive a new message from the client
	pub async fn receive(&mut self) -> Option<Request<P>> {
		self.inner.recv().await
	}

	pub fn update_config(&self, cfg: C) {
		self.cfg.send(cfg);
	}

	pub fn configurator(&self) -> Configurator<C> {
		Configurator::new(self.cfg.clone())
	}
}
