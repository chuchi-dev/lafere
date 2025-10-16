mod timeout;
pub(crate) mod watch;
#[macro_use]
pub mod bg_task;
mod listener;
pub mod testing;

pub use crate::standalone_util::*;
pub use listener::{Listener, ListenerExt, SocketAddr};
pub(crate) use timeout::TimeoutReader;

use tokio::io::{AsyncRead, AsyncWrite};

/// A trait to simplify using all tokio io traits.
pub trait ByteStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T> ByteStream for T where T: AsyncRead + AsyncWrite + Send + Unpin + 'static
{}
