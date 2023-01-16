//! Basic client server implementation based on a message with an action.
//!
//! Using fire stream as the communication protocol.

pub mod util;
pub mod message;
pub mod request;
pub mod client;
pub mod server;
pub mod encdec;
pub mod error;

#[doc(hidden)]
pub use stream;

#[doc(hidden)]
pub use bytes;

pub use codegen::*;