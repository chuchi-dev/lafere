//! Basic client server implementation based on a message with an action.
//!
//! Using fire stream as the communication protocol.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "connection")]
pub mod util;
pub mod message;
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

#[doc(hidden)]
pub use tracing;

pub use codegen::*;