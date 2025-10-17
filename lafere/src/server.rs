use crate::error::{RequestError, TaskError};
use crate::handler::{Configurator, Receiver, Sender, TaskHandle};
use crate::handler::{StreamReceiver, StreamSender};
use crate::packet::{Packet, PlainBytes};
use crate::plain;
use crate::util::ByteStream;

pub use crate::handler::handler::Request;

pub use Request as Message;

#[cfg(feature = "encrypted")]
use crate::{encrypted, packet::EncryptedBytes};
#[cfg(feature = "encrypted")]
use crypto::signature as sign;

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
	pub timeout: Duration,
	/// if the limit is 0 there is no limit
	pub body_limit: u32,
}

pub struct Connection<P> {
	sender: Sender<P>,
	sender_enabled: bool,
	receiver: Option<Receiver<P>>,
	config: Configurator<Config>,
	task: TaskHandle,
}

impl<P> Connection<P> {
	/// Creates a new server connection without any encryption.
	pub fn new<S>(byte_stream: S, cfg: Config) -> Self
	where
		S: ByteStream,
		P: Packet<PlainBytes> + Send + 'static,
		P::Header: Send,
	{
		plain::server(byte_stream, cfg)
	}

	#[cfg(feature = "encrypted")]
	pub fn new_encrypted<S>(
		byte_stream: S,
		cfg: Config,
		sign: sign::Keypair,
	) -> Self
	where
		S: ByteStream,
		P: Packet<EncryptedBytes> + Send + 'static,
		P::Header: Send,
	{
		encrypted::server(byte_stream, cfg, sign)
	}

	pub fn update_config(&self, cfg: Config) {
		self.config.update(cfg);
	}

	pub fn configurator(&self) -> Configurator<Config> {
		self.config.clone()
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
			sender_enabled: false,
			receiver: Some(receiver),
			config,
			task,
		}
	}

	pub fn take_receiver(&mut self) -> Option<Receiver<P>> {
		self.receiver.take()
	}

	/// ## Panics
	/// - If called when there is no receiver (e.g. after calling `take_receiver`)
	pub async fn receive(&mut self) -> Option<Request<P>> {
		let req = self.receiver.as_mut().unwrap().receive().await;
		if let Some(Request::EnableServerRequests) = &req {
			self.sender_enabled = true;
		}

		req
	}

	pub fn is_server_requests_enabled(&self) -> bool {
		self.sender_enabled
	}

	/// ## Panics
	/// - If server requests are not enabled
	pub fn clone_sender(&self) -> Sender<P> {
		assert!(self.sender_enabled);

		self.sender.clone()
	}

	/// The sender will not work as expected if the request is
	/// not enabled
	///
	/// So wait on a Request::EnableServerRequests before using it
	pub fn clone_sender_unchecked(&self) -> Sender<P> {
		self.sender.clone()
	}

	/// Send a request waiting until a response is available or the connection
	/// closes
	///
	/// ## Errors
	/// - Writing the packet failed
	/// - Reading the response packet failed
	/// - Io Error
	///
	/// ## Panics
	/// - If server requests are not enabled
	pub async fn request(&self, packet: P) -> Result<P, RequestError> {
		assert!(self.sender_enabled);

		self.sender.request(packet).await
	}

	/// Create a new stream to send packets.
	///
	/// ## Panics
	/// - If server requests are not enabled
	pub async fn request_sender(
		&self,
		packet: P,
	) -> Result<StreamSender<P>, RequestError> {
		assert!(self.sender_enabled);

		self.sender.request_sender(packet).await
	}

	/// Opens a new stream to listen to packets.
	///
	/// ## Panics
	/// - If server requests are not enabled
	pub async fn request_receiver(
		&self,
		packet: P,
	) -> Result<StreamReceiver<P>, RequestError> {
		assert!(self.sender_enabled);

		self.sender.request_receiver(packet).await
	}

	pub async fn close(self) -> Result<(), TaskError> {
		self.task.close().await
	}

	pub async fn wait(self) -> Result<(), TaskError> {
		self.task.wait().await
	}

	// used for testing
	#[cfg(test)]
	pub(crate) fn abort(self) {
		self.task.abort()
	}
}
