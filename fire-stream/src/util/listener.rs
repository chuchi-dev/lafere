use super::ByteStream;

use std::io;
use std::net::{self, SocketAddrV4, SocketAddrV6};
use std::task::{Poll, Context};
use std::future::Future;
use std::pin::Pin;

use tokio::net::{TcpListener, TcpStream};

/// SocketAddr struct equivalent to the one in stdlib but
/// the unix::SocketAddr was added.
pub enum SocketAddr {
	V4(SocketAddrV4),
	V6(SocketAddrV6),
	#[cfg(unix)]
	Unix(tokio::net::unix::SocketAddr)
}

pub trait Listener {
	type Stream: ByteStream;

	fn poll_accept(
		&self,
		cx: &mut Context<'_>
	) -> Poll<io::Result<(Self::Stream, SocketAddr)>>;
}

impl Listener for TcpListener {
	type Stream = TcpStream;

	fn poll_accept(
		&self,
		cx: &mut Context<'_>
	) -> Poll<io::Result<(Self::Stream, SocketAddr)>> {
		match self.poll_accept(cx) {
			Poll::Ready(Ok((stream, addr))) => Poll::Ready(Ok((
				stream,
				match addr {
					net::SocketAddr::V4(v4) => SocketAddr::V4(v4),
					net::SocketAddr::V6(v6) => SocketAddr::V6(v6)
				}
			))),
			Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
			Poll::Pending => Poll::Pending
		}
	}
}

#[cfg(unix)]
mod unix {

	use super::*;

	use tokio::net::{UnixListener, UnixStream};

	impl Listener for UnixListener {
		type Stream = UnixStream;

		fn poll_accept(
			&self,
			cx: &mut Context<'_>
		) -> Poll<io::Result<(Self::Stream, SocketAddr)>> {
			match self.poll_accept(cx) {
				Poll::Ready(Ok((stream, addr))) => Poll::Ready(Ok((
					stream,
					SocketAddr::Unix(addr)
				))),
				Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
				Poll::Pending => Poll::Pending
			}
		}
	}

}

pub trait ListenerExt: Listener {
	/// Equivalent to
	/// `async fn accept(&self) -> io::Result<(Self::Stream, SocketAddr)>`
	fn accept<'a>(&'a self) -> Accept<'a, Self>
	where Self: Sized;
}

impl<L> ListenerExt for L
where L: Listener {
	fn accept<'a>(&'a self) -> Accept<'a, Self>
	where Self: Sized {
		Accept { inner: self }
	}
}

// this should be !unpin
pub struct Accept<'a, L> {
	inner: &'a L
}

impl<'a, L> Future for Accept<'a, L>
where L: Listener {
	type Output = io::Result<(L::Stream, SocketAddr)>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		self.inner.poll_accept(cx)
	}
}