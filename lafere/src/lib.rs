#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[cfg(feature = "connection")]
pub mod handler;
#[cfg(feature = "connection")]
#[macro_use]
pub mod util;

pub mod standalone_util;

pub mod packet;
#[cfg(feature = "connection")]
mod plain;
#[cfg(all(feature = "connection", feature = "encrypted"))]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
mod encrypted;

#[cfg(feature = "connection")]
pub mod client;
#[cfg(feature = "connection")]
pub mod server;
pub mod error;