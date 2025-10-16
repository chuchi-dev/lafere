#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[cfg(feature = "connection")]
pub mod handler;
#[cfg(feature = "connection")]
#[macro_use]
pub mod util;

pub mod standalone_util;

#[cfg(all(feature = "connection", feature = "encrypted"))]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
mod encrypted;
pub mod packet;
#[cfg(feature = "connection")]
mod plain;

#[cfg(feature = "connection")]
pub mod client;
pub mod error;
#[cfg(feature = "connection")]
pub mod server;
