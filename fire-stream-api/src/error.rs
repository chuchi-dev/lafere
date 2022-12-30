
use std::fmt::{Debug, Display};

use serde::{Serialize, de::DeserializeOwned};

/// Basic error trait which implements Debug + Display + Serialize
/// 
/// The usefulness is still undecided
pub trait Error: Debug + Display + Serialize {}

impl<T> Error for T
where T: Debug + Display + Serialize {}

/// The error that is sent if something goes wrong while responding
/// to a request.
/// ## Panics
/// If deserialization or serialization failes
/// this will result in a panic
pub trait ApiError: Debug + Display + Serialize + DeserializeOwned {

	fn connection_closed() -> Self;

	// 
	fn request_dropped() -> Self;

	/// ## Server
	/// Get's called if the data that is needed for the request was not
	/// found.
	/// 
	/// Or if the response could not be serialized.
	fn internal<E: Error>(error: E) -> Self;

	/// ## Server
	/// If the server receives an error as a request.
	/// This *should never happen*.
	/// 
	/// If the request could not be serialized.
	/// 
	/// ## Client
	/// Get's called when a request could not be serialized.
	fn request<E: Error>(error: E) -> Self;

	/// ## Client
	/// Get's called when an response from the server could not be 
	/// deserialized.
	fn response<E: Error>(error: E) -> Self;

	/// ## Client
	/// Get's called if a StreamError occured which is not classified
	fn other<E: Error>(other: E) -> Self;

}