mod action;
mod api;
mod args;
mod enable_server_requests;
mod message;
mod util;

use args::{ApiArgs, EnableServerRequestsArgs};

use proc_macro::TokenStream;
use syn::{DeriveInput, ItemFn, parse_macro_input};

/*
#[api(Request)]
async fn request(req: Request, session: &Session) -> Result<Response> {
	todo!()
}
*/
#[proc_macro_attribute]
pub fn api(attrs: TokenStream, item: TokenStream) -> TokenStream {
	let args = parse_macro_input!(attrs as ApiArgs);
	let item = parse_macro_input!(item as ItemFn);

	let stream = api::expand(args, item);

	stream
		.map(|stream| stream.into())
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

/*
#[enable_server_requests(Action)]
async fn enable_server_requests(
	sender: Requestor<Action, B>,
) -> Result<(), lafere_api::error::Error> {
	todo!()
}

#[enable_server_requests(Action, EncryptedBytes)]
async fn esr_with_bytes(
	_sender: Requestor<Action, EncryptedBytes>,
) -> Result<(), lafere_api::error::Error> {
	todo!()
}
*/
#[proc_macro_attribute]
pub fn enable_server_requests(
	attrs: TokenStream,
	item: TokenStream,
) -> TokenStream {
	let args = parse_macro_input!(attrs as EnableServerRequestsArgs);
	let item = parse_macro_input!(item as ItemFn);

	let stream = enable_server_requests::expand(args, item);
	stream
		.map(|stream| stream.into())
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

/*
#[derive(IntoMessage)]
#[message(json)]
*/
#[proc_macro_derive(IntoMessage, attributes(message))]
pub fn derive_into_message(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);

	message::into_expand(input)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

#[proc_macro_derive(FromMessage, attributes(message))]
pub fn derive_from_message(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);

	message::from_expand(input)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}

#[proc_macro_derive(Action)]
pub fn derive_action(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);

	action::expand(input)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}
