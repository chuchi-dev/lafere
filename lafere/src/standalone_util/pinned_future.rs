use std::pin::Pin;
use std::future::Future;
use std::task::{Context, Poll};

pub struct PinnedFuture<'a, O> {
	inner: Pin<Box<dyn Future<Output=O> + Send + 'a>>
}

impl<'a, O> PinnedFuture<'a, O> {
	pub fn new<F>(future: F) -> Self
	where F: Future<Output=O> + Send + 'a {
		Self {
			inner: Box::pin(future)
		}
	}
}

impl<O> Future for PinnedFuture<'_, O> {
	type Output = O;
	fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		self.get_mut().inner.as_mut().poll(cx)
	}
}