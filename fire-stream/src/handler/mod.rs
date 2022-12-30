
pub(crate) mod client;
pub(crate) mod server;

use crate::error::StreamError::{self, StreamClosed, RequestDropped};
use crate::Result;
use crate::watch;

use tokio::sync::{oneshot, mpsc};
use tokio::task::JoinHandle;

/// Used in client and Server
pub(crate) enum SendBack<P> {
	None,
	Packet(P),
	Close
}

/// A Handle to a background task, if this handle is dropped
/// the connection will be dropped.
pub(crate) struct TaskHandle {
	pub close: oneshot::Sender<()>,
	pub task: JoinHandle<Result<()>>
}

impl TaskHandle {

	/// Wait until the connection has nothing more todo which will then close
	/// the connection.
	pub async fn wait(self) -> Result<()> {
		self.task.await
			.map_err(StreamError::JoinError)?
	}

	/// Send a close signal to the background task and wait until it closes.
	pub async fn close(self) -> Result<()> {
		let _ = self.close.send(());
		self.task.await
			.map_err(StreamError::JoinError)?
	}

	// used for testing
	#[cfg(test)]
	pub fn abort(self) {
		self.task.abort();
	}

}

/// A stream of packets which is inside of a connection.
#[derive(Debug)]
pub struct Stream<P> {
	pub(crate) inner: mpsc::Receiver<P>
}

impl<P> Stream<P> {

	pub(crate) fn new(inner: mpsc::Receiver<P>) -> Self {
		Self { inner }
	}

	/// If none is returned this can mean that the connection
	/// was closed or the other side is finished sending.
	pub async fn receive(&mut self) -> Option<P> {
		self.inner.recv().await
	}

	/// Marks the stream as closed but allows to received the remaining
	/// messages.
	pub fn close(&mut self) {
		self.inner.close();
	}

}

/// A sender of packets to an open stream.
#[derive(Debug, Clone)]
pub struct StreamSender<P> {
	pub(crate) inner: mpsc::Sender<P>
}

impl<P> StreamSender<P> {

	pub(crate) fn new(inner: mpsc::Sender<P>) -> Self {
		Self { inner }
	}

	/// Sends a packet to the client or the server.
	pub async fn send(&self, packet: P) -> Result<()> {
		// Todo change the error type
		self.inner.send(packet).await
			.map_err(|_| StreamClosed)
	}

	// pub fn try_send(&self, packet: P) -> Result<()> {
	// 	self.inner.try_send(packet)
	// 		.map_err(|_| StreamClosed)
	// }

}



// /// A message that is sent to the handler.
// #[derive(Debug)]
// pub(crate) enum SendMessage<P> {
// 	Request(P, oneshot::Sender<P>),
// 	// a request to receive a receiving stream
// 	Receiver(P, mpsc::Sender<P>),
// 	// a request to receive a sender stream
// 	Sender(P, mpsc::Receiver<P>)
// }



/// A message containing the different kinds of messages.
#[derive(Debug)]
pub enum Message<P> {
	Request(P, ResponseSender<P>),
	// a request to receive a receiving stream
	OpenStream(P, StreamSender<P>),
	// a request to receive a sender stream
	CreateStream(P, Stream<P>)
}

/// A sender used to respond to a request.
#[derive(Debug)]
pub struct ResponseSender<P> {
	pub(crate) inner: oneshot::Sender<P>
}

impl<P> ResponseSender<P> {
	pub(crate) fn new(inner: oneshot::Sender<P>) -> Self {
		Self { inner }
	}

	/// Sends the packet as a response, adding the correct flags.
	/// 
	/// If this returns an Error it either means the connection was closed
	/// or the requestor does not care about the response anymore.
	pub fn send(self, packet: P) -> Result<()> {
		// Todo change the error type
		self.inner.send(packet)
			.map_err(|_| RequestDropped)
	}
}

#[derive(Debug, Clone)]
pub struct Configurator<C> {
	inner: watch::Sender<C>
}

impl<C> Configurator<C> {

	pub(crate) fn new(inner: watch::Sender<C>) -> Self {
		Self { inner }
	}

	/// It is possible that there are no receivers left.
	/// 
	/// This is not checked
	pub fn update(&self, cfg: C) {
		self.inner.send(cfg);
	}

	pub fn read(&self) -> C
	where C: Clone {
		self.inner.newest()
	}

}