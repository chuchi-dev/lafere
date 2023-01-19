//! Basic client server implementation based on a message with an action.
//!
//! Using fire stream as the communication protocol.

#[cfg(feature = "connection")]
pub mod util;
pub mod message;
#[cfg(feature = "connection")]
pub mod request;
#[cfg(feature = "connection")]
pub mod client;
#[cfg(feature = "connection")]
pub mod server;
pub mod encdec;
pub mod error;

#[doc(hidden)]
pub use stream;

#[doc(hidden)]
pub use bytes;

pub use codegen::*;