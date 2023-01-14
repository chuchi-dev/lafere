mod poll_fn;
mod timeout;
pub(crate) mod watch;
mod pinned_future;
#[macro_use]
pub mod bg_task;
mod listener;

pub(crate) use timeout::TimeoutReader;
pub use poll_fn::{poll_fn, PollFn};
pub use pinned_future::PinnedFuture;
pub use listener::{Listener, ListenerExt};

use tokio::io::{AsyncRead, AsyncWrite};

/// A trait to simplify using all tokio io traits.
pub trait ByteStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T> ByteStream for T
where T: AsyncRead + AsyncWrite + Send + Unpin + 'static {}