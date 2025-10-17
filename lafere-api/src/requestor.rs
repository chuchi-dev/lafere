use lafere::{
	handler::Sender,
	packet::{Packet, PacketBytes},
};

use crate::{
	error::ApiError,
	message::{Action, FromMessage, IntoMessage, Message},
	request::Request,
};

pub struct Requestor<A, B> {
	inner: Sender<Message<A, B>>,
}

impl<A, B> Requestor<A, B> {
	pub(crate) fn new(inner: Sender<Message<A, B>>) -> Self {
		Self { inner }
	}
}

impl<A, B> Requestor<A, B>
where
	A: Action,
	B: PacketBytes,
{
	pub async fn request<R>(&self, req: R) -> Result<R::Response, R::Error>
	where
		R: Request<Action = A>,
		R: IntoMessage<A, B>,
		R::Response: FromMessage<A, B>,
		R::Error: FromMessage<A, B>,
	{
		let mut msg =
			req.into_message().map_err(R::Error::from_message_error)?;
		msg.header_mut().set_action(R::ACTION);

		let res = self
			.inner
			.request(msg)
			.await
			.map_err(R::Error::from_request_error)?;

		// now deserialize the response
		if res.is_success() {
			R::Response::from_message(res).map_err(R::Error::from_message_error)
		} else {
			R::Error::from_message(res)
				.map(Err)
				.map_err(R::Error::from_message_error)?
		}
	}
}
