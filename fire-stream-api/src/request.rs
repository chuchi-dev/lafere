use crate::message::{Message, Action};
use crate::error::{Error, ApiError};
use crate::server::{Data, Session};

use stream::util::PinnedFuture;


/// Basic request definition.
pub trait Request {
	type Action: Action;
	type Response;
	type Error: ApiError;

	const ACTION: Self::Action;
}

pub trait RequestHandler<B> {
	type Action: Action;

	fn action() -> Self::Action
	where Self: Sized;

	/// if the data is not available just panic
	fn validate_data(&self, data: &Data);

	/// handles a message with Self::ACTION as action.
	/// 
	/// if None is returned the request is abandoned and
	/// the requestor receives a RequestDropped error
	fn handle<'a>(
		&'a self,
		msg: Message<Self::Action, B>,
		data: &'a Data,
		session: &'a Session
	) -> PinnedFuture<'a, Result<Message<Self::Action, B>, Error>>;
}