
use super::message::Message;
use crate::Result;

pub trait Request<A, B>: Sized {
	type Response: Response<A, B>;
	fn action() -> A;
	fn into_message(self) -> Result<Message<A, B>>;
	fn from_message(msg: Message<A, B>) -> Result<Self>;
}

pub trait Response<A, B>: Sized {
	fn into_message(self) -> Result<Message<A, B>>;
	fn from_message(msg: Message<A, B>) -> Result<Self>;
}