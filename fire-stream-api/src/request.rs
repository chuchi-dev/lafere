
use crate::error::ApiError;
use crate::message::SerdeMessage;

use serde::{Serialize, de::DeserializeOwned};

/// Basic request definition.
/// 
/// The request will be serialized and deserialized
/// via Json to ease updating structures without breaking backwards
/// compatibility.
pub trait Request<A, B>: Serialize + DeserializeOwned {
	type Response: Serialize + DeserializeOwned;
	type Error: ApiError;
	const ACTION: A;
}

/// Request and Response Definition which does not necessariliy implement
/// serialize and deserialize.
///
/// However the error variant will always be serialized or deserialized.
/// 
/// If you wan't to use serde itself use the macro, derive_serde_message!(Type);
pub trait RawRequest<A, B>: SerdeMessage<A, B, Self::Error> {
	type Response: SerdeMessage<A, B, Self::Error>;

	type Error: ApiError;
	const ACTION: A;
}