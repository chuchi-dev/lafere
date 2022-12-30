//! Basic client server implementation based on a message
//! with an action.

pub mod message;
pub mod request;
pub mod client;
pub mod server;

pub use client::Client;
pub use server::Server;