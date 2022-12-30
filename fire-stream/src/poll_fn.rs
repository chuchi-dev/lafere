//!
//! copy of: <https://doc.rust-lang.org/beta/src/core/future/poll_fn.rs.html>
//! TODO: remove when <https://github.com/rust-lang/rust/issues/72302> get's stable

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Creates a future that wraps a function returning [`Poll`].
///
/// Polling the future delegates to the wrapped function.
pub fn poll_fn<T, F>(f: F) -> PollFn<F>
where
	F: FnMut(&mut Context<'_>) -> Poll<T>,
{
	PollFn { f }
}

/// A Future that wraps a function returning [`Poll`].
///
/// This `struct` is created by [`poll_fn()`]. See its
/// documentation for more.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PollFn<F> {
	f: F,
}

impl<F> Unpin for PollFn<F> {}

impl<F> fmt::Debug for PollFn<F> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("PollFn").finish()
	}
}

impl<T, F> Future for PollFn<F>
where
	F: FnMut(&mut Context<'_>) -> Poll<T>,
{
	type Output = T;

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
		(&mut self.f)(cx)
	}
}

