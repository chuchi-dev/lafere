// reference: https://docs.rs/tokio-io-timeout/0.4.0/src/tokio_io_timeout/lib.rs.html

use std::pin::Pin;
use std::task::{ Context, Poll };
use std::io::IoSlice;
use std::future::Future;

use tokio::net::{TcpStream, tcp};
use tokio::time::{ Duration, Instant, sleep_until, Sleep };
use tokio::io::{ self, AsyncRead, AsyncWrite, ReadBuf };

#[derive(Debug)]
pub struct TimeoutReader<S> {
	stream: S,
	timeout: Duration,
	timer: Pin<Box<Sleep>>,
	active: bool
}

impl<S> TimeoutReader<S>
where S: AsyncRead {
	pub fn new(stream: S, timeout: Duration) -> Self {
		Self {
			stream,
			timeout,
			timer: Box::pin(sleep_until(Instant::now())),
			active: false
		}
	}

	pub fn timeout(&self) -> Duration {
		self.timeout
	}

	pub fn set_timeout(&mut self, timeout: Duration) {
		self.timeout = timeout;
		self.timer.as_mut().reset(Instant::now());
		self.active = false;
	}

	pub fn poll_timeout(&mut self, cx: &mut Context) -> io::Result<()> {

		// activate if not activated
		if !self.active {
			self.timer.as_mut().reset(Instant::now() + self.timeout);
			self.active = true;
		}

		match self.timer.as_mut().poll(cx) {
			// timer has ended
			Poll::Ready(_) => Err(io::Error::from(io::ErrorKind::TimedOut)),
			// timer is still running
			Poll::Pending => Ok(())
		}
	}

	#[allow(dead_code)]
	pub fn inner_mut(&mut self) -> &mut S {
		&mut self.stream
	}
}

impl TimeoutReader<TcpStream> {
	#[allow(dead_code)]
	pub fn split<'a>(
		&'a mut self
	) -> (TimeoutReader<tcp::ReadHalf<'a>>, tcp::WriteHalf<'a>) {
		let (read, write) = self.stream.split();
		(TimeoutReader::new(read, self.timeout), write)
	}
}

impl<S> AsyncRead for TimeoutReader<S>
where S: AsyncRead + Unpin {
	fn poll_read(
		mut self: Pin<&mut Self>,
		cx: &mut Context,
		buf: &mut ReadBuf<'_>
	) -> Poll< io::Result<()> > {

		// call poll read on stream
		let r = Pin::new(&mut self.stream).poll_read(cx, buf);

		match r {
			Poll::Pending => self.get_mut().poll_timeout(cx)?,
			_ => { self.active = false }
		}

		r
	}
}

impl<S> AsyncWrite for TimeoutReader<S>
where S: AsyncWrite + Unpin {
	#[inline]
	fn poll_write(
		mut self: Pin<&mut Self>,
		cx: &mut Context,
		buf: &[u8]
	) -> Poll< io::Result<usize> > {
		Pin::new(&mut self.stream).poll_write(cx, buf)
	}

	#[inline]
	fn poll_flush(
		mut self: Pin<&mut Self>,
		cx: &mut Context
	) -> Poll< io::Result<()> > {
		Pin::new(&mut self.stream).poll_flush(cx)
	}

	#[inline]
	fn poll_shutdown(
		mut self: Pin<&mut Self>,
		cx: &mut Context
	) -> Poll< io::Result<()> > {
		Pin::new(&mut self.stream).poll_shutdown(cx)
	}

	#[inline]
	fn poll_write_vectored(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>]
	) -> Poll<io::Result<usize>> {
		Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
	}

	#[inline]
	fn is_write_vectored(&self) -> bool {
		self.stream.is_write_vectored()
	}
}


#[cfg(test)]
mod tests {

	use super::*;
	use tokio::io::AsyncReadExt;
	use tokio::time::sleep;

	struct DelayStream(Pin<Box<Sleep>>);

	impl DelayStream {
		fn new(until: Instant) -> Self {
			Self(Box::pin(sleep_until(until)))
		}
	}

	impl AsyncRead for DelayStream {
		fn poll_read(
			mut self: Pin<&mut Self>,
			cx: &mut Context,
			_: &mut ReadBuf<'_>
		) -> Poll<io::Result<()>> {
			self.0.as_mut().poll(cx)
				.map(|_| Ok(()))
		}
	}

	#[tokio::test]
	async fn read_timeout() {
		let reader = DelayStream::new(Instant::now() + Duration::from_millis(500));
		let mut reader = TimeoutReader::new(reader, Duration::from_millis(100));

		let r = reader.read(&mut [0]).await;
		assert_eq!(r.unwrap_err().kind(), io::ErrorKind::TimedOut);
		let r = reader.read(&mut [0]).await;
		assert_eq!(r.unwrap_err().kind(), io::ErrorKind::TimedOut);
		// now around 200ms passed
		sleep(Duration::from_millis(400)).await;
		reader.read(&mut [0]).await.unwrap();
	}

	#[tokio::test]
	async fn read_ok() {
		let reader = DelayStream::new(Instant::now() + Duration::from_millis(100));
		let mut reader = TimeoutReader::new(reader, Duration::from_millis(500));

		reader.read(&mut [0]).await.unwrap();
	}

}