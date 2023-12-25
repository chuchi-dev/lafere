use super::listener::{SocketAddr, Listener};

use std::io;
use std::task::{Poll, Context};
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};


/// Always Panics
pub struct PanicListener;

impl PanicListener {
	pub fn new() -> Self {
		Self
	}
}

impl Listener for PanicListener {
	type Stream = PanicStream;

	fn poll_accept(
		&self,
		_cx: &mut Context<'_>
	) -> Poll<io::Result<(Self::Stream, SocketAddr)>> {
		todo!("poll_accept")
	}
}

pub struct PanicStream;

impl AsyncRead for PanicStream {
	fn poll_read(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		_buf: &mut ReadBuf<'_>
	) -> Poll<io::Result<()>> {
		todo!("poll_read")
	}
}

impl AsyncWrite for PanicStream {
	fn poll_write(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		_buf: &[u8]
	) -> Poll<io::Result<usize>> {
		todo!("poll_write")
	}

	fn poll_flush(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>
	) -> Poll<io::Result<()>> {
		todo!("poll_flush")
	}

	fn poll_shutdown(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>
	) -> Poll<io::Result<()>> {
		todo!("poll_shutdown")
	}
}