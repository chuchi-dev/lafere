use crate::handler::handler::Request;

use tokio::sync::mpsc;

/// A receiver that waits on messages from the handler (client)
#[derive(Debug)]
pub struct Receiver<P> {
	pub(super) inner: mpsc::Receiver<Request<P>>,
}

impl<P> Receiver<P> {
	/// Receive a new message from the client
	pub async fn receive(&mut self) -> Option<Request<P>> {
		self.inner.recv().await
	}

	pub fn is_closed(&self) -> bool {
		self.inner.is_closed()
	}
}
