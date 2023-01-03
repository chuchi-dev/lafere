#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

pub mod poll_fn;
mod timeout;
mod watch;
pub mod handler;
pub mod pinned_future;

pub mod packet;
#[macro_use]
mod plain;
#[cfg(feature = "encrypted")]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
mod encrypted;

pub mod client;
pub mod server;
mod error;
pub mod listener;
#[cfg(feature = "basic")]
#[cfg_attr(docsrs, doc(cfg(feature = "basic")))]
#[deprecated]
pub mod basic;

pub mod traits {
	use tokio::io::{AsyncRead, AsyncWrite};

	/// A trait to simplify using all tokio io traits.
	pub trait ByteStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
	impl<T> ByteStream for T
	where T: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
}

pub use client::Connection as ClientCon;
pub use server::Connection as ServerCon;
pub use error::StreamError;

pub type Result<T> = std::result::Result<T, StreamError>;