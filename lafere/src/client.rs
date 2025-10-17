use crate::error::{RequestError, TaskError};
use crate::handler::{
	Configurator, Receiver, Sender, StreamReceiver, StreamSender, TaskHandle,
};
use crate::packet::{Packet, PlainBytes};
use crate::plain;
use crate::server::Request;
use crate::util::{ByteStream, PinnedFuture};

#[cfg(feature = "encrypted")]
use crate::{encrypted, packet::EncryptedBytes};
#[cfg(feature = "encrypted")]
use crypto::signature as sign;

use std::io;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
	pub timeout: Duration,
	/// if the limit is 0 there is no limit
	pub body_limit: u32,
}

/// Reconnection strategy
///
/// You should probably add a timeout before reconnecting
pub struct ReconStrat<S> {
	// async fn(error_count: usize) -> io::Result<S>
	pub(crate) inner:
		Box<dyn FnMut(usize) -> PinnedFuture<'static, io::Result<S>> + Send>,
}

impl<S> ReconStrat<S> {
	/// Expects the following fn:
	/// ```ignore
	/// async fn new_stream(error_count: usize) -> io::Result<S>;
	/// ```
	pub fn new<F: 'static>(f: F) -> Self
	where
		F: FnMut(usize) -> PinnedFuture<'static, io::Result<S>> + Send,
	{
		Self { inner: Box::new(f) }
	}
}

/// A connection to a server
pub struct Connection<P> {
	sender: Sender<P>,
	receiver: Option<Receiver<P>>,
	receiver_enabled: bool,
	config: Configurator<Config>,
	task: TaskHandle,
}

impl<P> Connection<P> {
	/// Creates a new connection to a server with an existing stream
	pub fn new<S>(
		byte_stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>,
	) -> Self
	where
		S: ByteStream,
		P: Packet<PlainBytes> + Send + 'static,
		P::Header: Send,
	{
		plain::client(byte_stream, cfg, recon_strat)
	}

	/// Creates a new connection to a server and encrypting all packets sent
	#[cfg(feature = "encrypted")]
	#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
	pub fn new_encrypted<S>(
		byte_stream: S,
		cfg: Config,
		recon_strat: Option<ReconStrat<S>>,
		sign: sign::PublicKey,
	) -> Self
	where
		S: ByteStream,
		P: Packet<EncryptedBytes> + Send + 'static,
		P::Header: Send,
	{
		encrypted::client(byte_stream, cfg, recon_strat, sign)
	}

	/// Creates a new Stream.
	pub(crate) fn new_raw(
		sender: Sender<P>,
		receiver: Receiver<P>,
		config: Configurator<Config>,
		task: TaskHandle,
	) -> Self {
		Self {
			sender,
			receiver: Some(receiver),
			receiver_enabled: false,
			config,
			task,
		}
	}

	/// Update the connection configuration
	pub fn update_config(&self, cfg: Config) {
		self.config.update(cfg);
	}

	/// Get's a `Configurator` which allows to configure this connection
	/// without needing to have access to the connection
	pub fn configurator(&self) -> Configurator<Config> {
		self.config.clone()
	}

	pub async fn enable_server_requests(&mut self) -> Result<(), RequestError> {
		self.sender.enable_server_requests().await?;
		self.receiver_enabled = true;

		Ok(())
	}

	/// ## Panics
	/// - If server requests are not enabled
	pub fn take_receiver(&mut self) -> Option<Receiver<P>> {
		assert!(self.receiver_enabled);

		self.receiver.take()
	}

	/// ## Panics
	/// - If server requests are not enabled
	/// - If called when there is no receiver (e.g. after calling `take_receiver`)
	pub async fn receive(&mut self) -> Option<Request<P>> {
		assert!(self.receiver_enabled);

		self.receiver.as_mut().unwrap().receive().await
	}

	pub fn clone_sender(&self) -> Sender<P> {
		self.sender.clone()
	}

	/// Send a request waiting until a response is available or the connection
	/// closes
	///
	/// ## Errors
	/// - Writing the packet failed
	/// - Reading the response packet failed
	/// - Io Error
	pub async fn request(&self, packet: P) -> Result<P, RequestError> {
		self.sender.request(packet).await
	}

	/// Create a new stream to send packets.
	pub async fn request_sender(
		&self,
		packet: P,
	) -> Result<StreamSender<P>, RequestError> {
		self.sender.request_sender(packet).await
	}

	/// Opens a new stream to listen to packets.
	pub async fn request_receiver(
		&self,
		packet: P,
	) -> Result<StreamReceiver<P>, RequestError> {
		self.sender.request_receiver(packet).await
	}

	// /// Create a new stream to send packets.
	// pub async fn request_stream(&self, packet: P) -> Result<StreamSender<P>> {
	// 	self.sender.create_stream(packet).await
	// }

	/// Wait until the connection has nothing more todo which will then close
	/// the connection.
	pub async fn wait(self) -> Result<(), TaskError> {
		self.task.wait().await
	}

	/// Send a close signal to the background task and wait until it closes.
	pub async fn close(self) -> Result<(), TaskError> {
		self.task.close().await
	}
}
