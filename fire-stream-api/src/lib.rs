//! Basic client server implementation based on a message
//! with an action.
//!
//! Using fire stream as the communication protocol.

pub mod util;
pub mod message;
pub mod request;
pub mod client;
pub mod server;
pub mod error;

// pub use client::Client;
// pub use server::Server;

#[doc(hidden)]
pub use serde_json;

#[doc(hidden)]
pub use stream;