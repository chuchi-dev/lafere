#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

pub mod handler;
#[macro_use]
pub mod util;

pub mod packet;
mod plain;
#[cfg(feature = "encrypted")]
#[cfg_attr(docsrs, doc(cfg(feature = "encrypted")))]
mod encrypted;

pub mod client;
pub mod server;
pub mod error;