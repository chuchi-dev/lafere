use crate::error::TaskError;
use crate::handler::{Configurator, TaskHandle, server::Receiver};
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
	receiver: Receiver<P, Config>,
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
		self.receiver.update_config(cfg);
	}

	pub fn configurator(&self) -> Configurator<Config> {
		self.receiver.configurator()
	}

	/// Creates a new Stream.
	pub(crate) fn new_raw(
		receiver: Receiver<P, Config>,
		task: TaskHandle,
	) -> Self {
		Self { receiver, task }
	}

	pub async fn receive(&mut self) -> Option<Request<P>> {
		self.receiver.receive().await
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
