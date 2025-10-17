//! Basic client server implementation based on a message with an action.
//!
//! Using fire stream as the communication protocol.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "connection")]
pub mod client;
pub mod encdec;
pub mod error;
pub mod message;
pub mod request;
#[cfg(feature = "connection")]
pub mod request_handlers;
#[cfg(feature = "connection")]
pub mod requestor;
#[cfg(feature = "connection")]
pub mod server;
#[cfg(feature = "connection")]
pub mod util;

#[doc(hidden)]
pub use lafere;

#[doc(hidden)]
pub use bytes;

#[doc(hidden)]
pub use tracing;

pub use codegen::*;
